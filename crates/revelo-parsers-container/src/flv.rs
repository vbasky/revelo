//! FLV (Flash Video) container parser.
//!
//! Mirrors a useful subset of MediaInfoLib's `File_Flv.cpp`. The 9-byte
//! file header gives `General.Format` plus the audio/video presence flags;
//! the tag stream that follows carries the per-track codec detail. This
//! parser walks every tag (script / video / audio), demuxes the AVC and
//! AAC codec-configuration records out of the first sequence-header tag of
//! each stream, counts frames, and derives per-stream timing from the tag
//! timestamps.
//!
//! Layout walked (all big-endian):
//!   0x00  C3  Signature ("FLV")
//!   0x03  B1  Version (typically 0x01)
//!   0x04  B1  TypeFlags (bit 0 = audio present, bit 2 = video present)
//!   0x05  B4  DataOffset (header size, typically 9)
//!   ...       PreviousTagSize0 (B4), then repeating tags:
//!             [TagType B1][DataSize B3][Timestamp B3][TimestampExt B1]
//!             [StreamID B3][data DataSize][PreviousTagSize B4]
//!
//! Everything is derived from the bitstream / AMF metadata; nothing is
//! keyed to a particular sample. Every read is bounds-guarded so truncated
//! or adversarial input can never panic — the dispatcher runs every parser
//! against arbitrary bytes.

use revelo_core::mime::mime_for_container;
use revelo_core::{FileAnalyze, Reader, StreamKind};
use revelo_parsers_video::{
    AvcInfo, EncoderInfo, extract_encoder_from_avc_sei_nalus, parse_avc_sps,
};

const FLV_HEADER_SIZE: usize = 9;
const FLV_SIGNATURE: [u8; 3] = *b"FLV";
const FLV_VERSION: u8 = 0x01;

const TYPE_FLAG_AUDIO: u8 = 0x01;
const TYPE_FLAG_VIDEO: u8 = 0x04;

const TAG_TYPE_AUDIO: u8 = 8;
const TAG_TYPE_VIDEO: u8 = 9;
const TAG_TYPE_SCRIPT: u8 = 18;

const VIDEO_CODEC_AVC: u8 = 7;
const AUDIO_FORMAT_AAC: u8 = 10;

/// AVC / AAC codec headers inside a tag are 5 / 2 bytes respectively:
///   video: FrameType|CodecID (1) + AVCPacketType (1) + CompositionTime (3)
///   audio: SoundFormat|Rate|Size|Type (1) + AACPacketType (1)
const VIDEO_TAG_HEADER: usize = 5;
const AUDIO_TAG_HEADER: usize = 2;

/// Upper bound on payload bytes copied out of a single tag while we still
/// need to inspect its contents (onMetaData AMF, codec config, or the first
/// frame's SEI). Frames we only need to *count* never copy beyond their
/// codec header, so the walk never buffers whole media payloads.
const TAG_INSPECT_LIMIT: usize = 1 << 20;

/// AAC sampling-frequency table (ISO/IEC 14496-3, samplingFrequencyIndex).
const AAC_SAMPLE_RATES: [u32; 13] =
    [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];

/// AAC frames always carry 1024 samples.
const AAC_SAMPLES_PER_FRAME: u64 = 1024;

#[derive(Default)]
struct VideoAcc {
    codec_id: Option<u8>,
    profile_idc: Option<u8>,
    profile_compat: Option<u8>,
    level_idc: Option<u8>,
    sps: Option<AvcInfo>,
    cabac: Option<bool>,
    encoder: Option<EncoderInfo>,
    nal_length_size: u8,
    frame_count: u32,
    first_ts: Option<u32>,
    last_ts: u32,
    min_delta: Option<u32>,
    max_delta: Option<u32>,
    prev_ts: Option<u32>,
    have_config: bool,
    have_sei: bool,
}

#[derive(Default)]
struct AudioAcc {
    sound_format: Option<u8>,
    /// Channels declared by the container (FLV SoundType bit).
    container_channels: Option<u8>,
    aot: Option<u8>,
    sample_rate: Option<u32>,
    /// Channels declared by the codec (AAC channelConfiguration).
    codec_channels: Option<u8>,
    first_ts: Option<u32>,
    last_ts: u32,
    saw_frame: bool,
    have_config: bool,
}

#[derive(Default)]
struct ScriptMeta {
    frame_rate: Option<f64>,
    encoder: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
}

/// Parse Adobe Flash Video container.
///
/// Detection: `FLV\x01` magic.
/// Fills: General format/duration, plus Video (AVC) and Audio (AAC) tracks.
pub fn parse_flv(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    // Require the full 9-byte header so truncated inputs are rejected
    // before any cursor movement, letting sibling parsers try the buffer.
    let header = r.peek_raw(FLV_HEADER_SIZE)?;
    if header[0..3] != FLV_SIGNATURE || header[3] != FLV_VERSION {
        return None;
    }

    r.element_begin("FLV header");
    r.be_u8("Signature[0]");
    r.be_u8("Signature[1]");
    r.be_u8("Signature[2]");
    r.be_u8("Version");
    let type_flags = r.be_u8("TypeFlags").unwrap_or(0);
    r.be_u32("DataOffset");
    r.element_end();

    let has_audio = (type_flags & TYPE_FLAG_AUDIO) != 0;
    let has_video = (type_flags & TYPE_FLAG_VIDEO) != 0;

    let mut video = VideoAcc { nal_length_size: 4, ..Default::default() };
    let mut audio = AudioAcc::default();
    let mut meta = ScriptMeta::default();

    // PreviousTagSize0 precedes the first tag. Its absence just means the
    // file is header-only; emit what we know and stop gracefully.
    if r.be_u32("PreviousTagSize0").is_some() {
        walk_tags(r, &mut video, &mut audio, &mut meta);
    }

    emit_fields(r, has_audio, has_video, &video, &audio, &meta);
    Some(())
}

/// Walk the tag stream. Every read is optional; any short read ends the
/// walk without discarding what was parsed so far.
fn walk_tags(r: &mut Reader, video: &mut VideoAcc, audio: &mut AudioAcc, meta: &mut ScriptMeta) {
    while let Some(tag_type) = r.be_u8("TagType") {
        let Some(data_size) = r.be_u24("DataSize") else { break };
        let data_size = data_size as usize;
        let Some(ts_low) = r.be_u24("Timestamp") else { break };
        let Some(ts_ext) = r.be_u8("TimestampExtended") else { break };
        let timestamp = ((ts_ext as u32) << 24) | ts_low;
        if r.be_u24("StreamID").is_none() {
            break;
        }

        let tt = tag_type & 0x1F;
        // Copy the full (bounded) payload only while there is still config,
        // metadata, or SEI to extract; otherwise the codec header is enough
        // to classify and count the frame.
        let want_full = match tt {
            TAG_TYPE_SCRIPT => true,
            TAG_TYPE_VIDEO => !video.have_config || !video.have_sei,
            TAG_TYPE_AUDIO => !audio.have_config,
            _ => false,
        };
        let peek_len = if want_full {
            data_size.min(TAG_INSPECT_LIMIT)
        } else {
            data_size.min(VIDEO_TAG_HEADER)
        };
        let payload = r.peek_raw(peek_len).map(<[u8]>::to_vec);
        let advanced = r.skip(data_size).is_some();

        if let Some(p) = payload.as_deref() {
            match tt {
                TAG_TYPE_SCRIPT => parse_script(p, meta),
                TAG_TYPE_VIDEO => handle_video_tag(p, timestamp, video),
                TAG_TYPE_AUDIO => handle_audio_tag(p, timestamp, audio),
                _ => {}
            }
        }

        if !advanced {
            break;
        }
        if r.be_u32("PreviousTagSize").is_none() {
            break;
        }
    }
}

fn handle_video_tag(p: &[u8], timestamp: u32, video: &mut VideoAcc) {
    let Some(&first) = p.first() else { return };
    let codec_id = first & 0x0F;
    video.codec_id.get_or_insert(codec_id);
    // Container-sourced Delay is the first tag timestamp of the stream —
    // the sequence header (t=0), not the first coded picture.
    video.first_ts.get_or_insert(timestamp);

    if codec_id != VIDEO_CODEC_AVC {
        // Non-AVC video: every tag is a frame; nothing further to extract.
        video.have_config = true;
        video.have_sei = true;
        count_video_frame(timestamp, video);
        return;
    }

    let Some(&avc_packet_type) = p.get(1) else { return };
    if avc_packet_type == 0 {
        // Sequence header — an AVCDecoderConfigurationRecord (== avcC).
        if !video.have_config && p.len() > VIDEO_TAG_HEADER {
            parse_avc_config(&p[VIDEO_TAG_HEADER..], video);
            video.have_config = true;
        }
        return;
    }

    count_video_frame(timestamp, video);
    if !video.have_sei {
        // Scan only the first coded frame for the encoder SEI.
        if p.len() > VIDEO_TAG_HEADER
            && let Some(enc) = extract_sei_encoder(&p[VIDEO_TAG_HEADER..], video.nal_length_size)
        {
            video.encoder = Some(enc);
        }
        video.have_sei = true;
    }
}

fn count_video_frame(timestamp: u32, video: &mut VideoAcc) {
    video.frame_count += 1;
    video.last_ts = video.last_ts.max(timestamp);
    if let Some(prev) = video.prev_ts {
        let delta = timestamp.saturating_sub(prev);
        video.min_delta = Some(video.min_delta.map_or(delta, |m| m.min(delta)));
        video.max_delta = Some(video.max_delta.map_or(delta, |m| m.max(delta)));
    }
    video.prev_ts = Some(timestamp);
}

fn handle_audio_tag(p: &[u8], timestamp: u32, audio: &mut AudioAcc) {
    let Some(&first) = p.first() else { return };
    let sound_format = (first >> 4) & 0x0F;
    // SoundType bit: 0 = mono, 1 = stereo → container channel count.
    let sound_type = first & 0x01;
    if audio.sound_format.is_none() {
        audio.sound_format = Some(sound_format);
        audio.container_channels = Some(sound_type + 1);
    }
    // Delay is the first tag timestamp (the config tag at t=0).
    audio.first_ts.get_or_insert(timestamp);

    if sound_format != AUDIO_FORMAT_AAC {
        // Non-AAC audio: no AudioSpecificConfig to demux.
        audio.have_config = true;
        track_audio_frame(timestamp, audio);
        return;
    }

    let Some(&aac_packet_type) = p.get(1) else { return };
    if aac_packet_type == 0 {
        if !audio.have_config && p.len() >= AUDIO_TAG_HEADER + 2 {
            parse_audio_specific_config(&p[AUDIO_TAG_HEADER..], audio);
            audio.have_config = true;
        }
        return;
    }
    track_audio_frame(timestamp, audio);
}

fn track_audio_frame(timestamp: u32, audio: &mut AudioAcc) {
    audio.saw_frame = true;
    audio.last_ts = audio.last_ts.max(timestamp);
}

/// Parse an AVCDecoderConfigurationRecord (identical layout to MP4 `avcC`):
/// pull profile/compat/level, the NAL length size, the first SPS (→ SPS
/// geometry/colour), and the first PPS (→ CABAC flag).
fn parse_avc_config(cfg: &[u8], video: &mut VideoAcc) {
    if cfg.len() < 6 {
        return;
    }
    video.profile_idc = Some(cfg[1]);
    video.profile_compat = Some(cfg[2]);
    video.level_idc = Some(cfg[3]);
    video.nal_length_size = (cfg[4] & 0x03) + 1;

    let num_sps = (cfg[5] & 0x1F) as usize;
    let mut pos = 6;
    let mut first_sps: Option<&[u8]> = None;
    for i in 0..num_sps {
        if pos + 2 > cfg.len() {
            return;
        }
        let len = u16::from_be_bytes([cfg[pos], cfg[pos + 1]]) as usize;
        pos += 2;
        if pos + len > cfg.len() {
            return;
        }
        if i == 0 {
            first_sps = Some(&cfg[pos..pos + len]);
        }
        pos += len;
    }
    if let Some(sps) = first_sps
        && let Some(info) = parse_avc_sps(sps)
    {
        video.sps = Some(info);
    }

    if pos < cfg.len() {
        let num_pps = cfg[pos] as usize;
        pos += 1;
        for i in 0..num_pps {
            if pos + 2 > cfg.len() {
                return;
            }
            let len = u16::from_be_bytes([cfg[pos], cfg[pos + 1]]) as usize;
            pos += 2;
            if pos + len > cfg.len() {
                return;
            }
            if i == 0 {
                video.cabac = parse_pps_cabac(&cfg[pos..pos + len]);
            }
            pos += len;
        }
    }
}

/// Collect SEI NAL units (type 6) from a length-prefixed AVC access unit
/// and pull the encoder string (x264 user_data_unregistered) out of them.
fn extract_sei_encoder(data: &[u8], nal_length_size: u8) -> Option<EncoderInfo> {
    let nls = nal_length_size as usize;
    if !(1..=4).contains(&nls) {
        return None;
    }
    let mut pos = 0;
    let mut sei: Vec<&[u8]> = Vec::new();
    while pos + nls <= data.len() {
        let mut len = 0usize;
        for &b in &data[pos..pos + nls] {
            len = (len << 8) | b as usize;
        }
        pos += nls;
        if len == 0 || pos + len > data.len() {
            break;
        }
        let nal = &data[pos..pos + len];
        if nal.first().is_some_and(|h| h & 0x1F == 6) {
            sei.push(nal);
        }
        pos += len;
    }
    if sei.is_empty() {
        return None;
    }
    extract_encoder_from_avc_sei_nalus(&sei)
}

/// AudioSpecificConfig (ISO/IEC 14496-3):
///   5 bits audioObjectType (31 → +6 bits)
///   4 bits samplingFrequencyIndex (15 → 24 bits explicit rate)
///   4 bits channelConfiguration
fn parse_audio_specific_config(asc: &[u8], audio: &mut AudioAcc) {
    let mut c = BitCursor::new(asc);
    let Some(mut aot) = c.read(5) else { return };
    if aot == 31 {
        let Some(ext) = c.read(6) else { return };
        aot += ext;
    }
    audio.aot = Some(aot as u8);

    let Some(freq_idx) = c.read(4) else { return };
    let sample_rate =
        if freq_idx == 15 { c.read(24) } else { AAC_SAMPLE_RATES.get(freq_idx as usize).copied() };
    if let Some(sr) = sample_rate.filter(|s| *s > 0) {
        audio.sample_rate = Some(sr);
    }

    if let Some(ch) = c.read(4).filter(|c| *c > 0) {
        audio.codec_channels = Some(ch as u8);
    }
}

/// Read the `entropy_coding_mode_flag` from a PPS NAL (1 = CABAC).
fn parse_pps_cabac(pps: &[u8]) -> Option<bool> {
    if pps.len() < 2 {
        return None;
    }
    let clean = remove_emulation_bytes(&pps[1..]);
    let mut c = BitCursor::new(&clean);
    c.read_ue()?; // pic_parameter_set_id
    c.read_ue()?; // seq_parameter_set_id
    Some(c.read_bit()? == 1)
}

/// Strip 0x000003 emulation-prevention bytes (collapse to 0x0000).
fn remove_emulation_bytes(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len());
    let mut i = 0;
    while i < rbsp.len() {
        if i + 2 < rbsp.len() && rbsp[i] == 0 && rbsp[i + 1] == 0 && rbsp[i + 2] == 3 {
            out.push(0);
            out.push(0);
            i += 3;
        } else {
            out.push(rbsp[i]);
            i += 1;
        }
    }
    out
}

// ── AMF0 (onMetaData) ──────────────────────────────────────────────────

/// Parse the script tag: an AMF0 string ("onMetaData") followed by an
/// ECMA array / object carrying the metadata key/value pairs we care about.
fn parse_script(p: &[u8], meta: &mut ScriptMeta) {
    let mut a = Amf::new(p);
    // First value is the event name string; skip it, then read the map.
    let _ = a.skip_value(0);
    let _ = a.read_metadata(meta);
}

struct Amf<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Amf<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.p)?;
        self.p += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<usize> {
        let hi = *self.b.get(self.p)? as usize;
        let lo = *self.b.get(self.p + 1)? as usize;
        self.p += 2;
        Some((hi << 8) | lo)
    }
    fn u32(&mut self) -> Option<()> {
        if self.p + 4 > self.b.len() {
            return None;
        }
        self.p += 4;
        Some(())
    }
    fn f64(&mut self) -> Option<f64> {
        if self.p + 8 > self.b.len() {
            return None;
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&self.b[self.p..self.p + 8]);
        self.p += 8;
        Some(f64::from_be_bytes(buf))
    }
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.p + n > self.b.len() {
            return None;
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Some(s)
    }

    /// Read the metadata container (ECMA array `0x08` or object `0x03`)
    /// and capture the known keys.
    fn read_metadata(&mut self, meta: &mut ScriptMeta) -> Option<()> {
        match self.u8()? {
            0x08 => {
                self.u32()?; // approximate element count — advisory only
            }
            0x03 => {}
            _ => return None,
        }
        loop {
            let n = self.u16()?;
            let key = self.bytes(n)?;
            if key.is_empty() {
                let _ = self.u8(); // object-end marker (0x09)
                return Some(());
            }
            let marker = self.u8()?;
            match marker {
                0x00 => {
                    let v = self.f64()?;
                    match key {
                        b"framerate" => meta.frame_rate = Some(v),
                        b"width" => meta.width = Some(v),
                        b"height" => meta.height = Some(v),
                        _ => {}
                    }
                }
                0x02 => {
                    let sn = self.u16()?;
                    let sb = self.bytes(sn)?;
                    if key == b"encoder"
                        && let Ok(s) = std::str::from_utf8(sb)
                    {
                        meta.encoder = Some(s.to_owned());
                    }
                }
                _ => self.skip_value_body(marker, 1)?,
            }
        }
    }

    fn skip_value(&mut self, depth: u32) -> Option<()> {
        let marker = self.u8()?;
        self.skip_value_body(marker, depth)
    }

    fn skip_value_body(&mut self, marker: u8, depth: u32) -> Option<()> {
        if depth > 32 {
            return None;
        }
        match marker {
            0x00 => {
                self.f64()?;
            }
            0x01 => {
                self.u8()?;
            }
            0x02 => {
                let n = self.u16()?;
                self.bytes(n)?;
            }
            0x03 => self.skip_object(depth)?,
            0x08 => {
                self.u32()?;
                self.skip_object(depth)?;
            }
            0x0A => {
                // strict array: u32 count, then that many values
                let hi = *self.b.get(self.p)? as usize;
                let b1 = *self.b.get(self.p + 1)? as usize;
                let b2 = *self.b.get(self.p + 2)? as usize;
                let b3 = *self.b.get(self.p + 3)? as usize;
                self.p += 4;
                let count = (hi << 24) | (b1 << 16) | (b2 << 8) | b3;
                for _ in 0..count {
                    self.skip_value(depth + 1)?;
                }
            }
            0x0B => {
                self.f64()?; // date value
                self.u16()?; // timezone
            }
            0x0C => {
                // long string: u32 length
                let hi = *self.b.get(self.p)? as usize;
                let b1 = *self.b.get(self.p + 1)? as usize;
                let b2 = *self.b.get(self.p + 2)? as usize;
                let b3 = *self.b.get(self.p + 3)? as usize;
                self.p += 4;
                let n = (hi << 24) | (b1 << 16) | (b2 << 8) | b3;
                self.bytes(n)?;
            }
            0x05 | 0x06 | 0x09 => {} // null / undefined / object-end
            _ => return None,
        }
        Some(())
    }

    fn skip_object(&mut self, depth: u32) -> Option<()> {
        loop {
            let n = self.u16()?;
            let key = self.bytes(n)?;
            if key.is_empty() {
                let _ = self.u8(); // object-end marker (0x09)
                return Some(());
            }
            self.skip_value(depth + 1)?;
        }
    }
}

// ── bit reader (ASC + PPS) ─────────────────────────────────────────────

struct BitCursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> BitCursor<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn read_bit(&mut self) -> Option<u32> {
        let byte = self.pos / 8;
        if byte >= self.buf.len() {
            return None;
        }
        let bit = 7 - (self.pos % 8);
        let v = (self.buf[byte] >> bit) & 1;
        self.pos += 1;
        Some(v as u32)
    }
    fn read(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.read_bit()?;
        }
        Some(v)
    }
    fn read_ue(&mut self) -> Option<u32> {
        let mut zeros = 0u32;
        while self.read_bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let mut val = 0u32;
        for _ in 0..zeros {
            val = (val << 1) | self.read_bit()?;
        }
        Some((1u32 << zeros) - 1 + val)
    }
}

// ── field emission ─────────────────────────────────────────────────────

fn emit_fields(
    r: &mut Reader,
    has_audio: bool,
    has_video: bool,
    video: &VideoAcc,
    audio: &AudioAcc,
    meta: &ScriptMeta,
) {
    let frame_rate = meta.frame_rate.filter(|f| *f > 0.0);
    let video_frame_dur_ms = frame_rate.map(|fr| (1000.0 / fr).round() as u64);
    let video_duration_ms =
        (video.frame_count > 0).then(|| video.last_ts as u64 + video_frame_dur_ms.unwrap_or(0));

    let audio_frame_dur_ms =
        audio.sample_rate.map(|sr| (AAC_SAMPLES_PER_FRAME * 1000 + sr as u64 / 2) / sr as u64);
    let audio_duration_ms =
        (audio.saw_frame).then(|| audio.last_ts as u64 + audio_frame_dur_ms.unwrap_or(0));

    let general_duration_ms = match (video_duration_ms, audio_duration_ms) {
        (Some(v), Some(a)) => Some(v.max(a)),
        (v, a) => v.or(a),
    };

    // ── General ────────────────────────────────────────────────────
    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "Flash Video");
    if let Some(m) = mime_for_container("FLV ") {
        r.set_field(StreamKind::General, 0, "InternetMediaType", m);
    }
    r.set_field(StreamKind::General, 0, "VideoCount", if has_video { "1" } else { "0" });
    r.set_field(StreamKind::General, 0, "AudioCount", if has_audio { "1" } else { "0" });
    if let Some(ms) = general_duration_ms {
        r.set_field(StreamKind::General, 0, "Duration", ms.to_string());
    }
    if let Some(app) = meta.encoder.as_deref() {
        r.set_field(StreamKind::General, 0, "Encoded_Application", app.to_owned());
    }

    let make_video = video.codec_id.is_some() || has_video;
    let make_audio = audio.sound_format.is_some() || has_audio;

    if make_video {
        emit_video(r, video, meta, frame_rate, video_duration_ms);
    }
    if make_audio {
        emit_audio(r, audio, video, audio_duration_ms, make_video);
    }
}

fn emit_video(
    r: &mut Reader,
    video: &VideoAcc,
    meta: &ScriptMeta,
    frame_rate: Option<f64>,
    duration_ms: Option<u64>,
) {
    let pos = r.stream_prepare(StreamKind::Video);

    if video.codec_id == Some(VIDEO_CODEC_AVC) {
        r.set_field(StreamKind::Video, pos, "Format", "AVC");
    }
    if let Some(cid) = video.codec_id {
        r.set_field(StreamKind::Video, pos, "CodecID", cid.to_string());
    }

    if let Some(idc) = video.profile_idc {
        let constrained = video.profile_compat.map(|c| (c & 0x40) != 0).unwrap_or(false);
        if let Some(profile) = avc_profile_name(idc, constrained) {
            r.set_field(StreamKind::Video, pos, "Format_Profile", profile);
        }
        // Prefer PPS-derived CABAC; Baseline (0x42) can only be CAVLC.
        let cabac = match video.cabac {
            Some(true) => Some("Yes"),
            Some(false) => Some("No"),
            None if idc == 0x42 => Some("No"),
            None => None,
        };
        if let Some(c) = cabac {
            r.set_field(StreamKind::Video, pos, "Format_Settings_CABAC", c);
        }
    }
    if let Some(lvl) = video.level_idc {
        r.set_field(StreamKind::Video, pos, "Format_Level", format_avc_level(lvl));
    }
    if let Some(sps) = video.sps.as_ref() {
        r.set_field(
            StreamKind::Video,
            pos,
            "Format_Settings_RefFrames",
            sps.ref_frames.to_string(),
        );
    }

    // Geometry: SPS is authoritative; fall back to onMetaData width/height.
    let width = video
        .sps
        .as_ref()
        .map(|s| s.width)
        .filter(|w| *w > 0)
        .or_else(|| meta.width.filter(|w| *w > 0.0).map(|w| w as u32));
    let height = video
        .sps
        .as_ref()
        .map(|s| s.height)
        .filter(|h| *h > 0)
        .or_else(|| meta.height.filter(|h| *h > 0.0).map(|h| h as u32));
    if let Some(w) = width {
        r.set_field(StreamKind::Video, pos, "Width", w.to_string());
    }
    if let Some(h) = height {
        r.set_field(StreamKind::Video, pos, "Height", h.to_string());
    }
    if let (Some(w), Some(h)) = (width, height) {
        let (h_sp, v_sp) = video
            .sps
            .as_ref()
            .and_then(|s| s.sar)
            .filter(|(a, b)| *a > 0 && *b > 0)
            .map(|(a, b)| (a as f64, b as f64))
            .unwrap_or((1.0, 1.0));
        let par = h_sp / v_sp;
        let sampled_w = (w as f64 * par).round() as u64;
        let dar = sampled_w as f64 / h as f64;
        r.set_field(StreamKind::Video, pos, "Sampled_Width", sampled_w.to_string());
        r.set_field(StreamKind::Video, pos, "Sampled_Height", h.to_string());
        r.set_field(StreamKind::Video, pos, "PixelAspectRatio", format!("{par:.3}"));
        r.set_field(StreamKind::Video, pos, "DisplayAspectRatio", format!("{dar:.3}"));
    }

    // AVC Baseline/Main/Extended/High are always 8-bit YUV 4:2:0
    // progressive per the H.264 spec; higher profiles would need SPS
    // chroma_format_idc to be safe, so skip the defaults there.
    if matches!(video.profile_idc, Some(0x42) | Some(0x4D) | Some(0x58) | Some(0x64)) {
        r.set_field(StreamKind::Video, pos, "ColorSpace", "YUV");
        r.set_field(StreamKind::Video, pos, "ChromaSubsampling", "4:2:0");
        r.set_field(StreamKind::Video, pos, "BitDepth", "8");
        r.set_field(StreamKind::Video, pos, "ScanType", "Progressive");
    }

    if let Some(fr) = frame_rate {
        r.set_field(StreamKind::Video, pos, "FrameRate", format!("{fr:.3}"));
        // A nominal metadata frame rate is treated as CFR; when the tag
        // timestamps are not evenly spaced the underlying stream is VFR.
        r.set_field(StreamKind::Video, pos, "FrameRate_Mode", "CFR");
        let varies = matches!((video.min_delta, video.max_delta), (Some(a), Some(b)) if a != b);
        if varies {
            r.set_field(StreamKind::Video, pos, "FrameRate_Mode_Original", "VFR");
        }
    } else if let (Some(dur), fc) = (duration_ms, video.frame_count)
        && dur > 0
        && fc > 0
    {
        let fr = fc as f64 * 1000.0 / dur as f64;
        r.set_field(StreamKind::Video, pos, "FrameRate", format!("{fr:.3}"));
    }
    if video.frame_count > 0 {
        r.set_field(StreamKind::Video, pos, "FrameCount", video.frame_count.to_string());
    }
    if let Some(dur) = duration_ms {
        r.set_field(StreamKind::Video, pos, "Duration", dur.to_string());
    }

    // Container-sourced presentation delay = first frame timestamp.
    let delay_ms = video.first_ts.unwrap_or(0);
    r.set_field(StreamKind::Video, pos, "Delay", format_ms_seconds(delay_ms));
    r.set_field(StreamKind::Video, pos, "Delay_Source", "Container");

    if let Some(enc) = video.encoder.as_ref() {
        r.set_field(StreamKind::Video, pos, "Encoded_Library", enc.library.clone());
        if let Some(name) = enc.name.as_deref() {
            r.set_field(StreamKind::Video, pos, "Encoded_Library_Name", name.to_owned());
        }
        if let Some(ver) = enc.version.as_deref() {
            r.set_field(StreamKind::Video, pos, "Encoded_Library_Version", ver.to_owned());
        }
        if let Some(settings) = enc.settings.as_deref() {
            r.set_field(StreamKind::Video, pos, "Encoded_Library_Settings", settings.to_owned());
        }
    }
}

fn emit_audio(
    r: &mut Reader,
    audio: &AudioAcc,
    video: &VideoAcc,
    duration_ms: Option<u64>,
    video_present: bool,
) {
    let pos = r.stream_prepare(StreamKind::Audio);
    let is_aac = audio.sound_format == Some(AUDIO_FORMAT_AAC);

    if is_aac {
        r.set_field(StreamKind::Audio, pos, "Format", "AAC");
    }
    if let (Some(sf), Some(aot)) = (audio.sound_format, audio.aot) {
        r.set_field(StreamKind::Audio, pos, "CodecID", format!("{sf}-{aot}"));
    }
    if let Some(aot) = audio.aot {
        if let Some(profile) = aac_profile_name(aot) {
            r.set_field(StreamKind::Audio, pos, "Format_AdditionalFeatures", profile);
        }
        // AAC LC (AOT 2) has no SBR signalling — reported "No (Explicit)".
        if aot == 2 {
            r.set_field(StreamKind::Audio, pos, "Format_Settings_SBR", "No (Explicit)");
        }
    }

    // Channels: the container SoundType bit is authoritative for the
    // reported value; the codec's channelConfiguration is the original.
    match (audio.container_channels, audio.codec_channels) {
        (Some(cont), codec) => {
            r.set_field(StreamKind::Audio, pos, "Channels", cont.to_string());
            if let Some(codec) = codec.filter(|c| *c != cont) {
                r.set_field(StreamKind::Audio, pos, "Channels_Original", codec.to_string());
                let (positions, layout) = aac_channel_layout(codec);
                if let Some(p) = positions {
                    r.set_field(StreamKind::Audio, pos, "ChannelPositions_Original", p);
                }
                if let Some(l) = layout {
                    r.set_field(StreamKind::Audio, pos, "ChannelLayout_Original", l);
                }
            }
        }
        (None, Some(codec)) => {
            r.set_field(StreamKind::Audio, pos, "Channels", codec.to_string());
        }
        (None, None) => {}
    }

    if let Some(sr) = audio.sample_rate {
        r.set_field(StreamKind::Audio, pos, "SamplingRate", sr.to_string());
        if is_aac {
            r.set_field(
                StreamKind::Audio,
                pos,
                "SamplesPerFrame",
                AAC_SAMPLES_PER_FRAME.to_string(),
            );
            r.set_field(
                StreamKind::Audio,
                pos,
                "FrameRate",
                format!("{:.3}", sr as f64 / AAC_SAMPLES_PER_FRAME as f64),
            );
        }
    }

    if let Some(dur) = duration_ms {
        r.set_field(StreamKind::Audio, pos, "Duration", dur.to_string());
        if let Some(sr) = audio.sample_rate {
            let sampling_count = dur * sr as u64 / 1000;
            r.set_field(StreamKind::Audio, pos, "SamplingCount", sampling_count.to_string());
        }
    }
    if is_aac {
        r.set_field(StreamKind::Audio, pos, "Compression_Mode", "Lossy");
    }

    let delay_ms = audio.first_ts.unwrap_or(0);
    r.set_field(StreamKind::Audio, pos, "Delay", format_ms_seconds(delay_ms));
    r.set_field(StreamKind::Audio, pos, "Delay_Source", "Container");
    if video_present {
        r.set_field(
            StreamKind::Audio,
            pos,
            "Video_Delay",
            format_ms_seconds(video.first_ts.unwrap_or(0)),
        );
    }
}

/// Render a millisecond count as seconds with 3 fraction digits. Used for
/// Delay-family fields, which — unlike Duration — are stored verbatim.
fn format_ms_seconds(ms: u32) -> String {
    format!("{}.{:03}", ms / 1000, ms % 1000)
}

/// AVC profile_idc → MediaInfo Format_Profile. `constrained` is the
/// profile_compatibility bit 6 (meaningful for Baseline).
fn avc_profile_name(idc: u8, constrained: bool) -> Option<&'static str> {
    match idc {
        0x42 => Some(if constrained { "Constrained Baseline" } else { "Baseline" }),
        0x4D => Some("Main"),
        0x58 => Some("Extended"),
        0x64 => Some("High"),
        0x6E => Some("High 10"),
        0x7A => Some("High 4:2:2"),
        0x90 | 0xF4 => Some("High 4:4:4"),
        _ => None,
    }
}

/// AVC level_idc → "X" / "X.Y" (level number is idc/10).
fn format_avc_level(idc: u8) -> String {
    let major = idc / 10;
    let minor = idc % 10;
    if minor == 0 { format!("{major}") } else { format!("{major}.{minor}") }
}

/// AAC audioObjectType → MediaInfo Format_AdditionalFeatures.
fn aac_profile_name(aot: u8) -> Option<&'static str> {
    match aot {
        1 => Some("Main"),
        2 => Some("LC"),
        3 => Some("SSR"),
        4 => Some("LTP"),
        5 => Some("SBR"),
        17 | 23 => Some("LC ER"),
        20 => Some("LTP ER"),
        29 => Some("PS"),
        _ => None,
    }
}

/// AAC channel positions / layout. Mono uses the AAC "M" layout label.
fn aac_channel_layout(channels: u8) -> (Option<&'static str>, Option<&'static str>) {
    match channels {
        1 => (Some("Front: C"), Some("M")),
        2 => (Some("Front: L R"), Some("L R")),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_flv(type_flags: u8) -> Vec<u8> {
        let mut buf = Vec::with_capacity(FLV_HEADER_SIZE);
        buf.extend_from_slice(b"FLV");
        buf.push(0x01);
        buf.push(type_flags);
        buf.extend_from_slice(&9u32.to_be_bytes());
        buf
    }

    fn tag(tag_type: u8, timestamp: u32, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(tag_type);
        let ds = data.len() as u32;
        out.extend_from_slice(&ds.to_be_bytes()[1..]); // u24 DataSize
        out.extend_from_slice(&timestamp.to_be_bytes()[1..]); // u24 Timestamp
        out.push((timestamp >> 24) as u8); // TimestampExtended
        out.extend_from_slice(&[0, 0, 0]); // StreamID
        out.extend_from_slice(data);
        let prev = 11 + data.len() as u32;
        out.extend_from_slice(&prev.to_be_bytes());
        out
    }

    // Real AVCDecoderConfigurationRecord from the reference sample:
    // Constrained Baseline, level 1.3, 320x240.
    const AVCC: &[u8] = &[
        0x01, 0x42, 0xc0, 0x0d, 0xff, 0xe1, 0x00, 0x16, 0x67, 0x42, 0xc0, 0x0d, 0xda, 0x05, 0x07,
        0xec, 0x04, 0x40, 0x00, 0x00, 0x03, 0x00, 0x40, 0x00, 0x00, 0x0c, 0x83, 0xc5, 0x0a, 0xa8,
        0x01, 0x00, 0x04, 0x68, 0xce, 0x0f, 0xc8,
    ];

    fn amf_onmetadata() -> Vec<u8> {
        let mut d = Vec::new();
        // string "onMetaData"
        d.push(0x02);
        d.extend_from_slice(&10u16.to_be_bytes());
        d.extend_from_slice(b"onMetaData");
        // ECMA array, 3 entries
        d.push(0x08);
        d.extend_from_slice(&3u32.to_be_bytes());
        let num = |d: &mut Vec<u8>, k: &[u8], v: f64| {
            d.extend_from_slice(&(k.len() as u16).to_be_bytes());
            d.extend_from_slice(k);
            d.push(0x00);
            d.extend_from_slice(&v.to_be_bytes());
        };
        num(&mut d, b"framerate", 25.0);
        num(&mut d, b"width", 320.0);
        // string encoder
        d.extend_from_slice(&(b"encoder".len() as u16).to_be_bytes());
        d.extend_from_slice(b"encoder");
        d.push(0x02);
        d.extend_from_slice(&(b"Lavf62.12.102".len() as u16).to_be_bytes());
        d.extend_from_slice(b"Lavf62.12.102");
        // object end
        d.extend_from_slice(&[0x00, 0x00, 0x09]);
        d
    }

    fn full_flv() -> Vec<u8> {
        let mut buf = make_flv(TYPE_FLAG_AUDIO | TYPE_FLAG_VIDEO);
        buf.extend_from_slice(&0u32.to_be_bytes()); // PreviousTagSize0

        buf.extend(tag(TAG_TYPE_SCRIPT, 0, &amf_onmetadata()));

        // video sequence header: 0x17, AVCPacketType 0, CompositionTime 0
        let mut vseq = vec![0x17, 0x00, 0x00, 0x00, 0x00];
        vseq.extend_from_slice(AVCC);
        buf.extend(tag(TAG_TYPE_VIDEO, 0, &vseq));

        // audio sequence header: 0xaf (AAC/44k/16/stereo), AACPacketType 0,
        // AudioSpecificConfig 0x12 0x08 (AOT 2, freq idx 4 = 44100, mono).
        buf.extend(tag(TAG_TYPE_AUDIO, 0, &[0xaf, 0x00, 0x12, 0x08]));

        // Three video frames with non-uniform timestamps (→ VFR original:
        // deltas 40 then 41).
        let vframe = [0x27u8, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x09, 0x30];
        buf.extend(tag(TAG_TYPE_VIDEO, 0, &vframe));
        buf.extend(tag(TAG_TYPE_VIDEO, 40, &vframe));
        buf.extend(tag(TAG_TYPE_VIDEO, 81, &vframe));

        // Two audio frames.
        buf.extend(tag(TAG_TYPE_AUDIO, 0, &[0xaf, 0x01, 0xde, 0x02]));
        buf.extend(tag(TAG_TYPE_AUDIO, 23, &[0xaf, 0x01, 0xde, 0x02]));

        buf
    }

    fn field(fa: &FileAnalyze, kind: StreamKind, key: &str) -> Option<String> {
        fa.retrieve(kind, 0, key).map(|z| z.as_str().to_owned())
    }

    #[test]
    fn parses_audio_and_video_flv() {
        let buf = make_flv(TYPE_FLAG_AUDIO | TYPE_FLAG_VIDEO);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        assert_eq!(field(&fa, StreamKind::General, "Format").as_deref(), Some("Flash Video"));
        assert_eq!(field(&fa, StreamKind::General, "VideoCount").as_deref(), Some("1"));
        assert_eq!(field(&fa, StreamKind::General, "AudioCount").as_deref(), Some("1"));
    }

    #[test]
    fn parses_audio_only_flv() {
        let buf = make_flv(TYPE_FLAG_AUDIO);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        assert_eq!(field(&fa, StreamKind::General, "AudioCount").as_deref(), Some("1"));
        assert_eq!(field(&fa, StreamKind::General, "VideoCount").as_deref(), Some("0"));
    }

    #[test]
    fn rejects_non_flv_buffer() {
        let buf = b"RIFF\x00\x00\x00\x00WAVE";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_flv(&mut fa));
    }

    #[test]
    fn rejects_truncated_header() {
        // Fewer than 9 bytes — the parser must decline without panicking.
        let buf = b"FLV\x01\x05";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_flv(&mut fa));
    }

    #[test]
    fn extracts_avc_video_track() {
        let buf = full_flv();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        assert_eq!(field(&fa, StreamKind::Video, "Format").as_deref(), Some("AVC"));
        assert_eq!(field(&fa, StreamKind::Video, "CodecID").as_deref(), Some("7"));
        assert_eq!(
            field(&fa, StreamKind::Video, "Format_Profile").as_deref(),
            Some("Constrained Baseline")
        );
        assert_eq!(field(&fa, StreamKind::Video, "Format_Level").as_deref(), Some("1.3"));
        assert_eq!(field(&fa, StreamKind::Video, "Format_Settings_CABAC").as_deref(), Some("No"));
        assert_eq!(
            field(&fa, StreamKind::Video, "Format_Settings_RefFrames").as_deref(),
            Some("1")
        );
        assert_eq!(field(&fa, StreamKind::Video, "Width").as_deref(), Some("320"));
        assert_eq!(field(&fa, StreamKind::Video, "Height").as_deref(), Some("240"));
        assert_eq!(field(&fa, StreamKind::Video, "ColorSpace").as_deref(), Some("YUV"));
        assert_eq!(field(&fa, StreamKind::Video, "ChromaSubsampling").as_deref(), Some("4:2:0"));
        assert_eq!(field(&fa, StreamKind::Video, "BitDepth").as_deref(), Some("8"));
        assert_eq!(field(&fa, StreamKind::Video, "ScanType").as_deref(), Some("Progressive"));
        assert_eq!(field(&fa, StreamKind::Video, "FrameRate").as_deref(), Some("25.000"));
        assert_eq!(field(&fa, StreamKind::Video, "FrameCount").as_deref(), Some("3"));
        assert_eq!(field(&fa, StreamKind::Video, "FrameRate_Mode").as_deref(), Some("CFR"));
        assert_eq!(
            field(&fa, StreamKind::Video, "FrameRate_Mode_Original").as_deref(),
            Some("VFR")
        );
    }

    #[test]
    fn extracts_aac_audio_track() {
        let buf = full_flv();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        assert_eq!(field(&fa, StreamKind::Audio, "Format").as_deref(), Some("AAC"));
        assert_eq!(field(&fa, StreamKind::Audio, "CodecID").as_deref(), Some("10-2"));
        assert_eq!(
            field(&fa, StreamKind::Audio, "Format_AdditionalFeatures").as_deref(),
            Some("LC")
        );
        assert_eq!(
            field(&fa, StreamKind::Audio, "Format_Settings_SBR").as_deref(),
            Some("No (Explicit)")
        );
        assert_eq!(field(&fa, StreamKind::Audio, "SamplingRate").as_deref(), Some("44100"));
        assert_eq!(field(&fa, StreamKind::Audio, "SamplesPerFrame").as_deref(), Some("1024"));
        assert_eq!(field(&fa, StreamKind::Audio, "Channels").as_deref(), Some("2"));
        assert_eq!(field(&fa, StreamKind::Audio, "Channels_Original").as_deref(), Some("1"));
        assert_eq!(field(&fa, StreamKind::Audio, "ChannelLayout_Original").as_deref(), Some("M"));
        assert_eq!(field(&fa, StreamKind::Audio, "Compression_Mode").as_deref(), Some("Lossy"));
    }

    #[test]
    fn reads_onmetadata_encoder() {
        let buf = full_flv();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        assert_eq!(
            field(&fa, StreamKind::General, "Encoded_Application").as_deref(),
            Some("Lavf62.12.102")
        );
    }

    #[test]
    fn tag_loop_survives_truncated_tag() {
        // Valid header + PreviousTagSize0, then a tag claiming a huge
        // DataSize with no payload. Must not panic and must still emit
        // the General stream.
        let mut buf = make_flv(TYPE_FLAG_VIDEO);
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.push(TAG_TYPE_VIDEO);
        buf.extend_from_slice(&[0xff, 0xff, 0xff]); // absurd DataSize
        buf.extend_from_slice(&[0, 0, 0]); // timestamp
        buf.push(0); // ts ext
        buf.extend_from_slice(&[0, 0, 0]); // stream id
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        assert_eq!(field(&fa, StreamKind::General, "Format").as_deref(), Some("Flash Video"));
    }
}
