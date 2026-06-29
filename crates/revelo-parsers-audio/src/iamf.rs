//! IAMF (Immersive Audio Model and Formats) / Eclipsa Audio parser.
//!
//! Implements OBU-level parsing of the IA Sequence per the IAMF v1.0.0
//! specification (AOM Final Deliverable, 3 April 2024). Eclipsa Audio is
//! Google/Samsung's brand name for IAMF.
//!
//! OBU types:
//!   0 = Codec Config      1 = Audio Element      2 = Mix Presentation
//!   3 = Parameter Block    4 = Temporal Delimiter  5..23 = Audio Frame
//!  24..30 = Reserved      31 = IA Sequence Header

use revelo_core::{FileAnalyze, StreamKind};

// ── OBU type constants ──────────────────────────────────────────────
const OBU_IA_CODEC_CONFIG: u8 = 0;
const OBU_IA_AUDIO_ELEMENT: u8 = 1;
const OBU_IA_SEQUENCE_HEADER: u8 = 31;
const IAMF_OBU_SCAN_LIMIT: usize = 1024 * 1024;

// ── Audio element types ─────────────────────────────────────────────
const AUDIO_ELEMENT_TYPE_CHANNEL_BASED: u8 = 0;
const AUDIO_ELEMENT_TYPE_SCENE_BASED: u8 = 1;

// ── Codec IDs (4CCs stored as u32) ──────────────────────────────────
const CODEC_ID_OPUS: u32 = u32::from_be_bytes(*b"Opus");
const CODEC_ID_MP4A: u32 = u32::from_be_bytes(*b"mp4a");
const CODEC_ID_FLAC: u32 = u32::from_be_bytes(*b"fLaC");
const CODEC_ID_IPCM: u32 = u32::from_be_bytes(*b"ipcm");

// ── Decoder config ID for Opus ──────────────────────────────────────
#[allow(dead_code)]
const OPUS_DECODER_CONFIG_ID: u8 = 0;

/// Parse a single IAMF OBU header from the data at `pos`.
/// Returns (obu_type, obu_size_in_bytes, header_size, redundant_copy).
fn parse_obu_header(data: &[u8], pos: usize) -> Option<(u8, usize, usize, bool)> {
    if pos >= data.len() {
        return None;
    }
    let b0 = data[pos];
    let obu_type = (b0 >> 3) & 0x1F;
    let redundant_copy = ((b0 >> 2) & 1) != 0;
    let trimming_status_flag = ((b0 >> 1) & 1) != 0;
    let extension_flag = (b0 & 1) != 0;

    let mut hpos = pos + 1;

    // obu_size (LEB128)
    let (obu_size, leb_bytes) = read_leb128(data, hpos)?;
    hpos += leb_bytes;

    if trimming_status_flag {
        let (_trim_end, leb) = read_leb128(data, hpos)?;
        hpos += leb;
        let (_trim_start, leb) = read_leb128(data, hpos)?;
        hpos += leb;
    }

    if extension_flag {
        let (ext_size, leb) = read_leb128(data, hpos)?;
        hpos += leb;
        hpos += ext_size as usize;
    }

    // A header that claims to extend past the buffer is malformed; reject it
    // here so downstream payload slices (`&data[pos + hdr_size..]`) stay safe.
    if hpos > data.len() {
        return None;
    }

    let total_header_size = hpos - pos;
    Some((obu_type, obu_size as usize, total_header_size, redundant_copy))
}

/// Decode a LEB128-encoded unsigned integer.
fn read_leb128(data: &[u8], pos: usize) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut i = pos;
    loop {
        if i >= data.len() || shift > 56 {
            return None;
        }
        let byte = data[i];
        i += 1;
        result |= ((byte & 0x7F) as u64) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            break;
        }
    }
    Some((result, i - pos))
}

/// Read `n` bytes at `pos` interpreted as a big-endian u32.
fn read_u32_be(data: &[u8], pos: usize) -> Option<u32> {
    if pos + 4 > data.len() {
        return None;
    }
    Some(u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]))
}

/// Read a single big-endian u16.
fn read_u16_be(data: &[u8], pos: usize) -> Option<u16> {
    if pos + 2 > data.len() {
        return None;
    }
    Some(u16::from_be_bytes([data[pos], data[pos + 1]]))
}

fn codec_id_to_str(codec_id: u32) -> &'static str {
    match codec_id {
        CODEC_ID_OPUS => "Opus",
        CODEC_ID_MP4A => "AAC",
        CODEC_ID_FLAC => "FLAC",
        CODEC_ID_IPCM => "PCM",
        _ => "Unknown",
    }
}

#[derive(Default)]
#[allow(dead_code)]
struct IaSequenceInfo {
    primary_profile: u8,
    additional_profile: u8,
}

#[derive(Default, Clone)]
#[allow(dead_code)]
struct CodecConfigInfo {
    codec_config_id: u64,
    codec_id: u32,
    num_samples_per_frame: u64,
    audio_roll_distance: i16,
}

#[derive(Default)]
#[allow(dead_code)]
struct AudioElementInfo {
    audio_element_id: u64,
    audio_element_type: u8,
    codec_config_id: u64,
    num_substreams: u64,
    substream_ids: Vec<u64>,
    channel_count: u8,
    channel_layout: String,
    ambisonics_order: u8,
}

/// Parse IA Sequence Header OBU payload.
fn parse_ia_sequence_header(data: &[u8], pos: usize, hdr_size: usize) -> Option<IaSequenceInfo> {
    let payload = &data[pos + hdr_size..];
    if payload.len() < 6 {
        return None;
    }
    let ia_code = read_u32_be(payload, 0)?;
    if ia_code != u32::from_be_bytes(*b"iamf") {
        return None;
    }
    Some(IaSequenceInfo { primary_profile: payload[4], additional_profile: payload[5] })
}

/// Parse Codec Config OBU payload.
fn parse_codec_config(data: &[u8], pos: usize, hdr_size: usize) -> Option<CodecConfigInfo> {
    let payload = &data[pos + hdr_size..];
    if payload.len() < 8 {
        return None;
    }
    let mut off = 0usize;
    let (codec_config_id, leb) = read_leb128(payload, off)?;
    off += leb;

    let codec_id = read_u32_be(payload, off)?;
    off += 4;

    let (num_samples_per_frame, leb) = read_leb128(payload, off)?;
    off += leb;

    let audio_roll_distance = read_u16_be(payload, off).map(|v| v as i16)?;

    Some(CodecConfigInfo { codec_config_id, codec_id, num_samples_per_frame, audio_roll_distance })
}

/// Parse Scalable Channel Layout Config.
fn parse_scalable_channel_layout(data: &[u8], off: &mut usize) -> (u8, String) {
    // num_layers (leb128), followed by channel groups per layer
    let (num_layers, leb) = match read_leb128(data, *off) {
        Some(v) => v,
        None => return (2, "Stereo".to_owned()),
    };
    *off += leb;

    let mut total_channels: u8 = 0;

    for _ in 0..num_layers {
        let (num_groups, leb) = match read_leb128(data, *off) {
            Some(v) => v,
            None => break,
        };
        *off += leb;

        for _ in 0..num_groups {
            if *off >= data.len() * 8 {
                break;
            }
            // channel_group_id (leb128), num_channels_in_group (leb128),
            // redundant_copy (1), reserved (3), channel_azimuth (4) x N
            let (_cg_id, leb) = match read_leb128(data, *off) {
                Some(v) => v,
                None => break,
            };
            *off += leb;

            let (num_ch, leb) = match read_leb128(data, *off) {
                Some(v) => v,
                None => break,
            };
            *off += leb;

            total_channels += num_ch as u8;

            // Skip redundant_copy (1), reserved (3), channel_azimuth (4 * num_ch)
            for _ in 0..num_ch {
                if *off >= data.len() * 8 {
                    break;
                }
                *off += 1; // each channel has azimuth (4 bits) + elevation (4 bits)
            }
        }
    }

    let layout = if total_channels <= 1 {
        "Mono".to_owned()
    } else if total_channels <= 2 {
        "Stereo".to_owned()
    } else if total_channels <= 6 {
        format!("{}.1", total_channels - 1)
    } else {
        format!("{}.1.{}", total_channels - 2, total_channels / 10)
    };

    (total_channels, layout)
}

/// Parse Ambisonics Config.
fn parse_ambisonics_config(data: &[u8], off: &mut usize) -> (u8, u8) {
    // ambisonics_mode (1), [ambisonics_order (leb128) if mode == 0]
    let mode = match data.get(*off / 8).map(|&b| (b >> (7 - (*off % 8))) & 1) {
        Some(v) => v,
        None => return (1, 1),
    };
    *off += 1;

    if mode == 0 {
        let (order, _leb) = match read_leb128(data, *off) {
            Some(v) => v,
            None => return (1, 1),
        };
        let channels = (order as u8 + 1).pow(2);
        return (channels, order as u8);
    }

    (4, 1) // Default: first-order ambisonics, 4 channels
}

/// Parse Audio Element OBU payload.
fn parse_audio_element(data: &[u8], pos: usize, hdr_size: usize) -> Option<AudioElementInfo> {
    let payload = &data[pos + hdr_size..];
    if payload.len() < 4 {
        return None;
    }

    let mut off = 0usize;

    let (audio_element_id, leb) = read_leb128(payload, off)?;
    off += leb;

    if off >= payload.len() {
        return None;
    }
    let type_byte = payload[off];
    let audio_element_type = (type_byte >> 5) & 0x07;
    off += 1;

    let (codec_config_id, leb) = read_leb128(payload, off)?;
    off += leb;

    let (num_substreams, leb) = read_leb128(payload, off)?;
    off += leb;

    let mut substream_ids = Vec::with_capacity(num_substreams as usize);
    for _ in 0..num_substreams {
        let (sid, leb) = read_leb128(payload, off)?;
        off += leb;
        substream_ids.push(sid);
    }

    // Skip parameters
    let (num_params, leb) = read_leb128(payload, off)?;
    off += leb;
    for _ in 0..num_params {
        let (_param_type, leb) = read_leb128(payload, off)?;
        off += leb;
    }

    let (mut channel_count, mut channel_layout, mut ambisonics_order) = (0u8, String::new(), 0u8);

    if audio_element_type == AUDIO_ELEMENT_TYPE_CHANNEL_BASED {
        let (cc, cl) = parse_scalable_channel_layout(payload, &mut off);
        channel_count = cc;
        channel_layout = cl;
    } else if audio_element_type == AUDIO_ELEMENT_TYPE_SCENE_BASED {
        let (cc, order) = parse_ambisonics_config(payload, &mut off);
        channel_count = cc;
        ambisonics_order = order;
        channel_layout = format!("Ambisonics (Order {})", order);
    }

    Some(AudioElementInfo {
        audio_element_id,
        audio_element_type,
        codec_config_id,
        num_substreams,
        substream_ids,
        channel_count,
        channel_layout,
        ambisonics_order,
    })
}

/// Parse an IAMF bitstream to extract metadata fields.
///
/// Detection: first OBU must be IA Sequence Header (obu_type 31).
/// Walks descriptor OBUs to fill Format, codec info, sample rate,
/// channel layout, profiles, and element types.
pub fn parse_iamf(fa: &mut FileAnalyze) -> bool {
    let scan_len = fa.remain().min(IAMF_OBU_SCAN_LIMIT);
    let data = match fa.peek_raw(scan_len) {
        Some(d) => d.to_vec(),
        None => return false,
    };

    if data.len() < 8 {
        return false;
    }

    // ── Parse first OBU header: must be IA Sequence Header ──────
    let (obu_type, _obu_size, hdr_size, _redundant) = match parse_obu_header(&data, 0) {
        Some(h) => h,
        None => return false,
    };
    if obu_type != OBU_IA_SEQUENCE_HEADER {
        return false;
    }

    // ── Validate the "iamf" magic ───────────────────────────────
    let seq_info = match parse_ia_sequence_header(&data, 0, hdr_size) {
        Some(s) => s,
        None => return false,
    };

    // ── Walk OBUs to collect descriptors ────────────────────────
    let mut pos = 0usize;
    let mut codec_configs: Vec<CodecConfigInfo> = Vec::new();
    let mut audio_elements: Vec<AudioElementInfo> = Vec::new();

    while pos < data.len() {
        let (oty, osize, hsize, _redundant) = match parse_obu_header(&data, pos) {
            Some(h) => h,
            None => break,
        };

        match oty {
            OBU_IA_CODEC_CONFIG => {
                if let Some(cc) = parse_codec_config(&data, pos, hsize) {
                    codec_configs.push(cc);
                }
            }
            OBU_IA_AUDIO_ELEMENT => {
                if let Some(ae) = parse_audio_element(&data, pos, hsize) {
                    audio_elements.push(ae);
                }
            }
            OBU_IA_SEQUENCE_HEADER if pos > 0 => {
                // Re-sync sequence header: reset descriptor state
                codec_configs.clear();
                audio_elements.clear();
            }
            _ => {}
        }

        pos += hsize + osize;
        if osize == 0 {
            break;
        }
    }

    // ── Fill metadata fields ────────────────────────────────────
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "IAMF");
    fa.set_field(StreamKind::General, 0, "Format_Commercial_IfAny", "Eclipsa Audio");
    fa.set_field(StreamKind::General, 0, "AudioCount", audio_elements.len().to_string());

    let profile_str = match seq_info.primary_profile {
        0 => "Simple",
        1 => "Base",
        _ => "Unknown",
    };
    fa.set_field(StreamKind::General, 0, "Format_Profile", profile_str);
    fa.set_field(StreamKind::General, 0, "CodecID", "iamf");
    fa.set_field(StreamKind::General, 0, "InternetMediaType", "audio/iamf");

    // First audio stream
    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "IAMF");
    fa.set_field(StreamKind::Audio, 0, "Format_Profile", profile_str);
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");

    // If we have codec config, fill per-codec details
    if let Some(cc) = codec_configs.first() {
        let codec_name = codec_id_to_str(cc.codec_id);
        fa.set_field(StreamKind::Audio, 0, "CodecID", codec_name);
        fa.set_field(
            StreamKind::Audio,
            0,
            "CodecID_Description",
            format!("IAMF codec: {}", codec_name),
        );
        if cc.num_samples_per_frame > 0 {
            fa.set_field(
                StreamKind::Audio,
                0,
                "SamplesPerFrame",
                cc.num_samples_per_frame.to_string(),
            );
        }
        // Infer sample rate from frame details if possible
        if cc.num_samples_per_frame > 0 {
            // Default AAC frame is 1024 samples; Opus is 960 in IAMF
            // MediaInfo doesn't report sample rate here, but we can show the frame size
        }
    }

    if let Some(ae) = audio_elements.first() {
        let element_type_str = match ae.audio_element_type {
            AUDIO_ELEMENT_TYPE_CHANNEL_BASED => "Channel-Based",
            AUDIO_ELEMENT_TYPE_SCENE_BASED => "Scene-Based (Ambisonics)",
            _ => "Unknown",
        };
        fa.set_field(StreamKind::Audio, 0, "Format_Settings", element_type_str);
        if ae.channel_count > 0 {
            fa.set_field(StreamKind::Audio, 0, "Channels", ae.channel_count.to_string());
        }
        if !ae.channel_layout.is_empty() {
            fa.set_field(StreamKind::Audio, 0, "ChannelLayout", ae.channel_layout.as_str());
        }
        fa.set_field(StreamKind::Audio, 0, "StreamCount", ae.num_substreams.to_string());
    }

    // Additional audio streams per Audio Element
    if audio_elements.len() > 1 {
        // MediaInfo reports multiple audio streams for multi-element IAMF
        for (_i, ae) in audio_elements.iter().enumerate().skip(1) {
            let pos = fa.stream_prepare(StreamKind::Audio);
            fa.set_field(StreamKind::Audio, pos, "Format", "IAMF");
            fa.set_field(StreamKind::Audio, pos, "ID", ae.audio_element_id.to_string());
            if ae.channel_count > 0 {
                fa.set_field(StreamKind::Audio, pos, "Channels", ae.channel_count.to_string());
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_iamf(first_byte: u8) -> Vec<u8> {
        let extension = (first_byte & 1) != 0;

        let mut seq_payload = b"iamf".to_vec();
        seq_payload.push(0);
        seq_payload.push(0);

        let mut buf = vec![first_byte];
        buf.push(seq_payload.len() as u8);
        if extension {
            buf.push(0);
        }
        buf.extend_from_slice(&seq_payload);

        let ae_payload = [0u8; 5];
        buf.push(0x08);
        buf.push(ae_payload.len() as u8);
        buf.extend_from_slice(&ae_payload);

        buf
    }

    fn make_iamf_with_codec_config(first_byte: u8, codec_4cc: &[u8; 4]) -> Vec<u8> {
        let extension = (first_byte & 1) != 0;

        let mut seq_payload = b"iamf".to_vec();
        seq_payload.push(0);
        seq_payload.push(0);

        let mut buf = vec![first_byte];
        buf.push(seq_payload.len() as u8);
        if extension {
            buf.push(0);
        }
        buf.extend_from_slice(&seq_payload);

        // Codec Config OBU (type=0): codec_config_id(LEB128) + codec_id(4) + num_samples_per_frame(LEB128) + audio_roll_distance(u16)
        let cc_id: u64 = 0;
        let cc_id_leb = vec![cc_id as u8];
        let cc_payload = {
            let mut p = cc_id_leb.clone();
            p.extend_from_slice(codec_4cc);
            // num_samples_per_frame = 960 (Opus default)
            p.push(0xC0);
            p.push(0x07); // LEB128 960
            // audio_roll_distance = 0
            p.push(0x00);
            p.push(0x00);
            p
        };
        // obu header: type=0, no flags
        buf.push(0x00);
        buf.push(cc_payload.len() as u8);
        buf.extend_from_slice(&cc_payload);

        // Audio Element OBU (type=1): references codec_config_id=0
        let mut ae_payload = vec![0u8; 1]; // ae_id = 0
        ae_payload.push(0x00); // type_byte: audio_element_type=0 (channel-based), reserved bits
        ae_payload.push(0x00); // codec_config_id = 0
        ae_payload.push(0x01); // num_substreams = 1
        ae_payload.push(0x00); // substream_id[0] = 0
        ae_payload.push(0x00); // num_params = 0
        // scalable channel layout: num_layers=1
        ae_payload.push(0x01); // num_layers = 1
        // layer 0: num_groups=1
        ae_payload.push(0x01);
        // group 0: cg_id=0, num_channels=2
        ae_payload.push(0x00); // cg_id = 0
        ae_payload.push(0x02); // num_channels = 2
        // skip channel azimuth/elevation for each channel (1 byte each)
        ae_payload.push(0x00); // ch0 azimuth/elev
        ae_payload.push(0x00); // ch1 azimuth/elev

        buf.push(0x08);
        buf.push(ae_payload.len() as u8);
        buf.extend_from_slice(&ae_payload);

        buf
    }

    fn make_iamf_ambisonics(first_byte: u8) -> Vec<u8> {
        let extension = (first_byte & 1) != 0;

        let mut seq_payload = b"iamf".to_vec();
        seq_payload.push(0);
        seq_payload.push(0);

        let mut buf = vec![first_byte];
        buf.push(seq_payload.len() as u8);
        if extension {
            buf.push(0);
        }
        buf.extend_from_slice(&seq_payload);

        // Audio Element OBU: scene-based (ambisonics)
        let mut ae_payload = vec![0u8; 1]; // ae_id = 0
        // type_byte: audio_element_type=1 (scene-based)
        ae_payload.push(0x20);
        ae_payload.push(0x00); // codec_config_id = 0
        ae_payload.push(0x01); // num_substreams = 1
        ae_payload.push(0x00); // substream_id[0] = 0
        ae_payload.push(0x00); // num_params = 0
        // ambisonics config: ambisonics_mode=0 (normal), order=1
        ae_payload.push(0x00); // mode=0 at bit 0
        ae_payload.push(0x01); // order = 1

        buf.push(0x08);
        buf.push(ae_payload.len() as u8);
        buf.extend_from_slice(&ae_payload);

        buf
    }

    fn make_iamf_multi_element(first_byte: u8) -> Vec<u8> {
        // Two Audio Element OBUs
        let extension = (first_byte & 1) != 0;

        let mut seq_payload = b"iamf".to_vec();
        seq_payload.push(0);
        seq_payload.push(0);

        let mut buf = vec![first_byte];
        buf.push(seq_payload.len() as u8);
        if extension {
            buf.push(0);
        }
        buf.extend_from_slice(&seq_payload);

        // Audio Element OBU 1: ae_id=0, channel-based, 2 channels
        let mut ae1 = vec![0u8; 1]; // ae_id = 0
        ae1.push(0x00); // channel-based
        ae1.push(0x00); // codec_config_id = 0
        ae1.push(0x01); // num_substreams = 1
        ae1.push(0x00); // substream_id = 0
        ae1.push(0x00); // num_params = 0
        ae1.push(0x01); // num_layers = 1
        ae1.push(0x01); // num_groups = 1
        ae1.push(0x00); // cg_id = 0
        ae1.push(0x02); // num_channels = 2
        ae1.push(0x00); // ch0
        ae1.push(0x00); // ch1

        buf.push(0x08);
        buf.push(ae1.len() as u8);
        buf.extend_from_slice(&ae1);

        // Audio Element OBU 2: ae_id=1, channel-based, 6 channels
        let mut ae2 = vec![0x01u8; 1]; // ae_id = 1
        ae2.push(0x00); // channel-based
        ae2.push(0x00); // codec_config_id = 0
        ae2.push(0x01); // num_substreams = 1
        ae2.push(0x00); // substream_id = 0
        ae2.push(0x00); // num_params = 0
        ae2.push(0x01); // num_layers = 1
        ae2.push(0x01); // num_groups = 1
        ae2.push(0x00); // cg_id = 0
        ae2.push(0x06); // num_channels = 6
        for _ in 0..6 {
            ae2.push(0x00);
        }

        buf.push(0x08);
        buf.push(ae2.len() as u8);
        buf.extend_from_slice(&ae2);

        buf
    }

    #[test]
    fn parses_iamf_sequence_header() {
        for &b in &[0xF8u8, 0xF9, 0xFC, 0xFD] {
            let buf = make_iamf(b);
            let mut fa = FileAnalyze::new(&buf);
            assert!(parse_iamf(&mut fa), "first byte {:#X} should be accepted", b);
            let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
            let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
            assert_eq!(g("Format").as_deref(), Some("IAMF"));
            assert_eq!(g("AudioCount").as_deref(), Some("1"));
            assert_eq!(a("Format").as_deref(), Some("IAMF"));
            assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
        }
    }

    #[test]
    fn parses_codec_config_opus() {
        let buf = make_iamf_with_codec_config(0xF8, b"Opus");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("CodecID").as_deref(), Some("Opus"));
        assert!(a("CodecID_Description").unwrap_or_default().contains("Opus"));
        assert!(a("SamplesPerFrame").as_deref() == Some("960"));
    }

    #[test]
    fn parses_codec_config_aac() {
        let buf = make_iamf_with_codec_config(0xF8, b"mp4a");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("CodecID").as_deref(), Some("AAC"));
    }

    #[test]
    fn parses_codec_config_flac() {
        let buf = make_iamf_with_codec_config(0xF8, b"fLaC");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("CodecID").as_deref(), Some("FLAC"));
    }

    #[test]
    fn parses_channel_based_element() {
        let buf = make_iamf_with_codec_config(0xF8, b"Opus");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_Settings").as_deref(), Some("Channel-Based"));
        assert_eq!(a("StreamCount").as_deref(), Some("1"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
    }

    #[test]
    fn parses_ambisonics_element() {
        let buf = make_iamf_ambisonics(0xF8);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert!(a("Format_Settings").unwrap_or_default().contains("Ambisonics"));
        assert_eq!(a("Channels").as_deref(), Some("4"));
        assert_eq!(a("StreamCount").as_deref(), Some("1"));
    }

    #[test]
    fn parses_multi_element_stream() {
        let buf = make_iamf_multi_element(0xF8);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("AudioCount").as_deref(), Some("2"));
        let a0 = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        let a1 = |k: &str| fa.retrieve(StreamKind::Audio, 1, k).map(|z| z.as_str().to_owned());
        assert_eq!(a0("Format").as_deref(), Some("IAMF"));
        assert_eq!(a1("Format").as_deref(), Some("IAMF"));
        assert_eq!(a1("Channels").as_deref(), Some("6"));
    }

    #[test]
    fn iamf_does_not_request_full_payload() {
        let mut buf = make_iamf(0xF8);
        buf.resize(IAMF_OBU_SCAN_LIMIT + 1024, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, IAMF_OBU_SCAN_LIMIT);
    }

    #[test]
    fn parses_simple_profile() {
        let buf = make_iamf(0xF8);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iamf(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format_Profile").as_deref(), Some("Simple"));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_Profile").as_deref(), Some("Simple"));
    }

    #[test]
    fn parses_scalable_channel_layout_mono() {
        let mut off = 0usize;
        let data = [0x01, 0x01, 0x00, 0x01, 0x00];
        let (count, layout) = parse_scalable_channel_layout(&data, &mut off);
        assert_eq!(count, 1);
        assert_eq!(layout, "Mono");
    }

    #[test]
    fn parses_scalable_channel_layout_stereo() {
        let mut off = 0usize;
        let data = [0x01, 0x01, 0x00, 0x02, 0x00, 0x00];
        let (count, layout) = parse_scalable_channel_layout(&data, &mut off);
        assert_eq!(count, 2);
        assert_eq!(layout, "Stereo");
    }

    #[test]
    fn parses_scalable_channel_layout_5dot1() {
        let mut off = 0usize;
        let data = [0x01, 0x01, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let (count, _layout) = parse_scalable_channel_layout(&data, &mut off);
        assert_eq!(count, 6);
    }

    #[test]
    fn rejects_non_iamf_buffer() {
        let buf = make_iamf(0x00);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_iamf(&mut fa));
        let mut fa2 = FileAnalyze::new(b"NOT IAMF........");
        assert!(!parse_iamf(&mut fa2));
    }

    #[test]
    fn rejects_when_reserved_bit_set() {
        for &b in &[0xFAu8, 0xFB, 0xFE, 0xFF] {
            let buf = make_iamf(b);
            let mut fa = FileAnalyze::new(&buf);
            assert!(!parse_iamf(&mut fa), "reserved-bit-set byte {:#X} should be rejected", b);
        }
    }

    #[test]
    fn rejects_short_buffer() {
        let mut fa = FileAnalyze::new(&[0xF8u8, 0x00]);
        assert!(!parse_iamf(&mut fa));
    }

    #[test]
    fn obu_header_rejects_empty() {
        assert!(parse_obu_header(&[], 0).is_none());
    }

    #[test]
    fn obu_header_parses_sequence_header_type() {
        // 0xF8 = obu_type=31 (11111), redundant=0, reserved=0, extension=0
        let result = parse_obu_header(&[0xF8, 0x04, b'i', b'a', b'm', b'f', 0x00, 0x00], 0);
        assert!(result.is_some());
        let (ty, size, hdr_size, redundant) = result.unwrap();
        assert_eq!(ty, 31);
        assert_eq!(size, 4);
        assert_eq!(hdr_size, 2);
        assert!(!redundant);
    }

    #[test]
    fn obu_header_handles_leb128_extension() {
        // obu_type=31, extension=1, extension_header_size=0
        let result = parse_obu_header(&[0xF9, 0x04, 0x00, b'i', b'a', b'm', b'f', 0x00, 0x00], 0);
        assert!(result.is_some());
        let (_ty, _size, hdr_size, _redundant) = result.unwrap();
        // header includes: obu_header(1) + leb128-size(1) + ext_size_leb(1) + ext_data(0) = 3
        assert_eq!(hdr_size, 3);
    }

    #[test]
    fn read_leb128_zero() {
        let (val, bytes) = read_leb128(&[0x00], 0).unwrap();
        assert_eq!(val, 0);
        assert_eq!(bytes, 1);
    }

    #[test]
    fn read_leb128_multi_byte() {
        // 960 = 0x3C0, LEB128 = [0xC0, 0x07]
        let (val, bytes) = read_leb128(&[0xC0, 0x07], 0).unwrap();
        assert_eq!(val, 960);
        assert_eq!(bytes, 2);
    }

    #[test]
    fn read_leb128_overflow() {
        assert!(read_leb128(&[0x80; 20], 0).is_none());
    }
}
