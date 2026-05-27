//! Matroska / WebM parser — EBML-based container.
//!
//! Mirrors a subset of MediaInfoLib's `File_Mk.cpp` covering plain
//! single-audio-track Matroska files. The container is structurally
//! similar to MP4 boxes — a tree of `[id][size][payload]` elements —
//! but the headers use Variable-length Integer encoding (VINT) where
//! the byte length is signaled by the position of the leading 1-bit.
//!
//! VINT encoding:
//! - First byte's leading zeros count + 1 = total VINT length N (1..=8).
//! - For element IDs, the value includes the leading 1-bit (so IDs
//!   look like 0x1A45DFA3, 0x18538067, etc — distinguishable).
//! - For sizes, the leading 1-bit is stripped and the remaining bits
//!   plus N-1 subsequent bytes form the integer.

use mediainfo_core::{FileAnalyze, StreamKind};

// EBML root + segment.
const EBML_HEADER: u64 = 0x1A45DFA3;
const SEGMENT: u64 = 0x18538067;

// EBML header children. (EBMLVersion/ReadVersion declared but unused
// in this MVP — kept for future schema awareness.)
#[allow(dead_code)]
const EBML_VERSION: u64 = 0x4286;
#[allow(dead_code)]
const EBML_READ_VERSION: u64 = 0x42F7;
const DOC_TYPE: u64 = 0x4282;
const DOC_TYPE_VERSION: u64 = 0x4287;

// Segment top-level.
const SEGMENT_INFO: u64 = 0x1549A966;
const TRACKS: u64 = 0x1654AE6B;
const CLUSTER: u64 = 0x1F43B675;
const SEEK_HEAD: u64 = 0x114D9B74;

// Info children.
const SEGMENT_UUID: u64 = 0x73A4;
const TIMECODE_SCALE: u64 = 0x2AD7B1;
const DURATION: u64 = 0x4489;
const MUXING_APP: u64 = 0x4D80;
const WRITING_APP: u64 = 0x5741;

// Tracks > TrackEntry.
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_UID: u64 = 0x73C5;
const TRACK_TYPE: u64 = 0x83;
const FLAG_DEFAULT: u64 = 0x88;
const FLAG_FORCED: u64 = 0x55AA;
const CODEC_ID: u64 = 0x86;
const AUDIO_BLOCK: u64 = 0xE1;
const SAMPLING_FREQUENCY: u64 = 0xB5;
const CHANNELS: u64 = 0x9F;
const BIT_DEPTH: u64 = 0x6264;

pub fn parse_mkv(fa: &mut FileAnalyze) -> bool {
    // Detect EBML header by leading 4 bytes.
    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };
    if h != [0x1A, 0x45, 0xDF, 0xA3] {
        return false;
    }

    let file_size = fa.Remain();
    let mut doc_type: Option<String> = None;
    let mut doc_type_version: u64 = 0;
    let mut movie = MovieInfo::default();
    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut seekhead_seen = false;
    let mut cluster_seen = false;
    let mut seekhead_before_cluster = true;

    walk_elements(fa, file_size, &mut |fa, id, size, _start| {
        match id {
            EBML_HEADER => {
                walk_elements(fa, size, &mut |fa, id, sz, _| match id {
                    DOC_TYPE => {
                        let bytes = fa.read_raw(sz).to_vec();
                        doc_type = Some(strip_nuls(&bytes));
                    }
                    DOC_TYPE_VERSION => {
                        doc_type_version = read_uint(fa, sz);
                    }
                    _ => {
                        fa.Skip_Hexa(sz, "ebml_child");
                    }
                });
            }
            SEGMENT => {
                walk_elements(fa, size, &mut |fa, id, sz, _| match id {
                    SEEK_HEAD => {
                        seekhead_seen = true;
                        if cluster_seen {
                            seekhead_before_cluster = false;
                        }
                        fa.Skip_Hexa(sz, "seekhead");
                    }
                    SEGMENT_INFO => {
                        parse_segment_info(fa, sz, &mut movie);
                    }
                    TRACKS => {
                        parse_tracks(fa, sz, &mut tracks);
                    }
                    CLUSTER => {
                        cluster_seen = true;
                        fa.Skip_Hexa(sz, "cluster");
                    }
                    _ => fa.Skip_Hexa(sz, "segment_child"),
                });
            }
            _ => {
                fa.Skip_Hexa(size, "top_level");
            }
        }
    });

    if doc_type.as_deref() != Some("matroska") && doc_type.as_deref() != Some("webm") {
        return false;
    }

    fill_streams(
        fa,
        &movie,
        &tracks,
        doc_type_version,
        seekhead_seen && seekhead_before_cluster,
    );
    true
}

#[derive(Default)]
struct MovieInfo {
    segment_uuid: Option<Vec<u8>>,
    timecode_scale: Option<u64>,
    duration_units: Option<f64>,
    muxing_app: Option<String>,
    writing_app: Option<String>,
}

#[derive(Default)]
struct TrackInfo {
    number: Option<u64>,
    uid: Option<u64>,
    track_type: Option<u64>,
    flag_default: Option<bool>,
    flag_forced: Option<bool>,
    codec_id: Option<String>,
    audio_sampling_rate: Option<f64>,
    audio_channels: Option<u64>,
    audio_bit_depth: Option<u64>,
}

fn parse_segment_info(fa: &mut FileAnalyze, size: usize, movie: &mut MovieInfo) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        SEGMENT_UUID => {
            movie.segment_uuid = Some(fa.read_raw(sz).to_vec());
        }
        TIMECODE_SCALE => {
            movie.timecode_scale = Some(read_uint(fa, sz));
        }
        DURATION => {
            movie.duration_units = Some(read_float(fa, sz));
        }
        MUXING_APP => {
            let bytes = fa.read_raw(sz).to_vec();
            movie.muxing_app = Some(strip_nuls(&bytes));
        }
        WRITING_APP => {
            let bytes = fa.read_raw(sz).to_vec();
            movie.writing_app = Some(strip_nuls(&bytes));
        }
        _ => fa.Skip_Hexa(sz, "info_child"),
    });
}

fn parse_tracks(fa: &mut FileAnalyze, size: usize, tracks: &mut Vec<TrackInfo>) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        TRACK_ENTRY => {
            let mut entry = TrackInfo::default();
            parse_track_entry(fa, sz, &mut entry);
            tracks.push(entry);
        }
        _ => fa.Skip_Hexa(sz, "tracks_child"),
    });
}

fn parse_track_entry(fa: &mut FileAnalyze, size: usize, entry: &mut TrackInfo) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        TRACK_NUMBER => entry.number = Some(read_uint(fa, sz)),
        TRACK_UID => entry.uid = Some(read_uint(fa, sz)),
        TRACK_TYPE => entry.track_type = Some(read_uint(fa, sz)),
        FLAG_DEFAULT => entry.flag_default = Some(read_uint(fa, sz) != 0),
        FLAG_FORCED => entry.flag_forced = Some(read_uint(fa, sz) != 0),
        CODEC_ID => {
            let bytes = fa.read_raw(sz).to_vec();
            entry.codec_id = Some(strip_nuls(&bytes));
        }
        AUDIO_BLOCK => {
            walk_elements(fa, sz, &mut |fa, id, sz, _| match id {
                SAMPLING_FREQUENCY => entry.audio_sampling_rate = Some(read_float(fa, sz)),
                CHANNELS => entry.audio_channels = Some(read_uint(fa, sz)),
                BIT_DEPTH => entry.audio_bit_depth = Some(read_uint(fa, sz)),
                _ => fa.Skip_Hexa(sz, "audio_child"),
            });
        }
        _ => fa.Skip_Hexa(sz, "trackentry_child"),
    });
}

fn fill_streams(
    fa: &mut FileAnalyze,
    movie: &MovieInfo,
    tracks: &[TrackInfo],
    doc_type_version: u64,
    is_streamable: bool,
) {
    fa.Stream_Prepare(StreamKind::General);
    if let Some(uuid) = movie.segment_uuid.as_ref() {
        if uuid.len() == 16 {
            let mut v: u128 = 0;
            for b in uuid {
                v = (v << 8) | (*b as u128);
            }
            fa.Fill(StreamKind::General, 0, "UniqueID", v.to_string(), false);
        }
    }
    fa.Fill(StreamKind::General, 0, "Format", "Matroska", false);
    if doc_type_version > 0 {
        fa.Fill(
            StreamKind::General,
            0,
            "Format_Version",
            doc_type_version.to_string(),
            false,
        );
    }
    if let Some(app) = movie.muxing_app.as_deref() {
        fa.Fill(StreamKind::General, 0, "Encoded_Application", app, false);
    }
    if let Some(app) = movie.writing_app.as_deref() {
        fa.Fill(StreamKind::General, 0, "Encoded_Library", app, false);
    }
    fa.Fill(
        StreamKind::General,
        0,
        "IsStreamable",
        if is_streamable { "Yes" } else { "No" },
        false,
    );

    let timecode_scale_ns: f64 = movie.timecode_scale.unwrap_or(1_000_000) as f64;
    let duration_seconds: Option<f64> = movie
        .duration_units
        .map(|units| units * timecode_scale_ns / 1_000_000_000.0);
    let duration_ms: Option<u64> = duration_seconds.map(|s| (s * 1000.0).round() as u64);

    let mut audio_count: u32 = 0;
    let mut video_count: u32 = 0;
    let mut stream_order: u32 = 0;
    for track in tracks {
        match track.track_type {
            Some(2) => {
                let pos = fa.Stream_Prepare(StreamKind::Audio);
                fa.Fill(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string(), false);
                if let Some(n) = track.number {
                    fa.Fill(StreamKind::Audio, pos, "ID", n.to_string(), false);
                }
                if let Some(uid) = track.uid {
                    fa.Fill(StreamKind::Audio, pos, "UniqueID", uid.to_string(), false);
                }
                stream_order += 1;
                if let Some(c) = track.codec_id.as_deref() {
                    if let Some(fmt) = mkv_codec_to_format(c) {
                        fa.Fill(StreamKind::Audio, pos, "Format", fmt, false);
                    }
                    fa.Fill(StreamKind::Audio, pos, "CodecID", c, false);
                }
                if let Some(ch) = track.audio_channels {
                    fa.Fill(StreamKind::Audio, pos, "Channels", ch.to_string(), false);
                    let (positions, layout) = channel_layout(ch as u16);
                    if let Some(p) = positions {
                        fa.Fill(StreamKind::Audio, pos, "ChannelPositions", p, false);
                    }
                    if let Some(l) = layout {
                        fa.Fill(StreamKind::Audio, pos, "ChannelLayout", l, false);
                    }
                }
                if let Some(sr) = track.audio_sampling_rate {
                    let sr_int = sr.round() as u64;
                    fa.Fill(StreamKind::Audio, pos, "SamplingRate", sr_int.to_string(), false);
                    if let Some(ms) = duration_ms {
                        let sampling_count = (sr * ms as f64 / 1000.0).round() as u64;
                        fa.Fill(
                            StreamKind::Audio,
                            pos,
                            "SamplingCount",
                            sampling_count.to_string(),
                            false,
                        );
                    }
                }
                if let Some(bd) = track.audio_bit_depth {
                    fa.Fill(StreamKind::Audio, pos, "BitDepth", bd.to_string(), false);
                }
                if let Some(c) = track.codec_id.as_deref() {
                    if codec_is_lossy(c) {
                        fa.Fill(StreamKind::Audio, pos, "Compression_Mode", "Lossy", false);
                    } else if codec_is_lossless(c) {
                        fa.Fill(StreamKind::Audio, pos, "Compression_Mode", "Lossless", false);
                    }
                }
                if let Some(d) = track.flag_default {
                    fa.Fill(StreamKind::Audio, pos, "Default", if d { "Yes" } else { "No" }, false);
                }
                // FlagForced defaults to 0 in MKV; oracle always
                // emits this field for audio tracks, so default
                // missing values to "No".
                let forced = track.flag_forced.unwrap_or(false);
                fa.Fill(StreamKind::Audio, pos, "Forced", if forced { "Yes" } else { "No" }, false);
                // MKV oracle emits Audio.Duration with 9 fractional
                // digits (the file's float precision). Store the
                // pre-formatted string here so the exporter's
                // ms-to-seconds conversion doesn't touch it.
                if let Some(s) = duration_seconds {
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "Duration",
                        format!("{:.9}", s),
                        false,
                    );
                }
                audio_count += 1;
            }
            Some(1) => video_count += 1,
            _ => {}
        }
    }

    if audio_count > 0 {
        fa.Fill(StreamKind::General, 0, "AudioCount", audio_count.to_string(), false);
    }
    if video_count > 0 {
        fa.Fill(StreamKind::General, 0, "VideoCount", video_count.to_string(), false);
    }
    if let Some(ms) = duration_ms {
        fa.Fill(StreamKind::General, 0, "Duration", ms.to_string(), false);
    }
}

fn channel_layout(channels: u16) -> (Option<&'static str>, Option<&'static str>) {
    match channels {
        1 => (Some("Front: C"), Some("C")),
        2 => (Some("Front: L R"), Some("L R")),
        _ => (None, None),
    }
}

fn mkv_codec_to_format(codec_id: &str) -> Option<&'static str> {
    match codec_id {
        "A_OPUS" => Some("Opus"),
        "A_VORBIS" => Some("Vorbis"),
        "A_AAC" | "A_AAC/MPEG2/MAIN" | "A_AAC/MPEG2/LC" | "A_AAC/MPEG4/LC" | "A_AAC/MPEG4/LC/SBR" => {
            Some("AAC")
        }
        "A_AC3" => Some("AC-3"),
        "A_EAC3" => Some("E-AC-3"),
        "A_DTS" => Some("DTS"),
        "A_FLAC" => Some("FLAC"),
        "A_MPEG/L3" => Some("MPEG Audio"),
        "A_MPEG/L2" => Some("MPEG Audio"),
        "A_PCM/INT/LIT" | "A_PCM/INT/BIG" | "A_PCM/FLOAT/IEEE" => Some("PCM"),
        "V_MPEG4/ISO/AVC" => Some("AVC"),
        "V_MPEGH/ISO/HEVC" => Some("HEVC"),
        "V_VP9" => Some("VP9"),
        "V_AV1" => Some("AV1"),
        _ => None,
    }
}

fn codec_is_lossy(codec_id: &str) -> bool {
    matches!(
        codec_id,
        "A_OPUS"
            | "A_VORBIS"
            | "A_AAC"
            | "A_AAC/MPEG2/MAIN"
            | "A_AAC/MPEG2/LC"
            | "A_AAC/MPEG4/LC"
            | "A_AAC/MPEG4/LC/SBR"
            | "A_AC3"
            | "A_EAC3"
            | "A_DTS"
            | "A_MPEG/L3"
            | "A_MPEG/L2"
    )
}

fn codec_is_lossless(codec_id: &str) -> bool {
    matches!(
        codec_id,
        "A_FLAC" | "A_PCM/INT/LIT" | "A_PCM/INT/BIG" | "A_PCM/FLOAT/IEEE"
    )
}

fn strip_nuls(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).into_owned();
    s.trim_end_matches('\0').to_string()
}

fn read_uint(fa: &mut FileAnalyze, size: usize) -> u64 {
    let bytes = fa.read_raw(size);
    let mut v: u64 = 0;
    for b in bytes {
        v = (v << 8) | (*b as u64);
    }
    v
}

fn read_float(fa: &mut FileAnalyze, size: usize) -> f64 {
    match size {
        4 => {
            let mut v: zenlib::float32 = 0.0;
            fa.Get_BF4(&mut v, "f32");
            v as f64
        }
        8 => {
            let mut v: zenlib::float64 = 0.0;
            fa.Get_BF8(&mut v, "f64");
            v
        }
        _ => {
            fa.Skip_Hexa(size, "unknown_float_size");
            0.0
        }
    }
}

/// Read an EBML VINT representing an element ID — keeps the leading
/// 1-bit so the returned value matches the canonical IDs like
/// 0x1A45DFA3.
fn read_vint_id(fa: &mut FileAnalyze) -> Option<u64> {
    let first_bytes = fa.peek_raw(1)?;
    let first = first_bytes[0];
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let bytes = fa.read_raw(len);
    if bytes.len() < len {
        return None;
    }
    let mut v: u64 = 0;
    for b in bytes {
        v = (v << 8) | (*b as u64);
    }
    Some(v)
}

/// Read an EBML VINT representing a size — strips the leading 1-bit
/// to return the actual numeric value.
fn read_vint_size(fa: &mut FileAnalyze) -> Option<u64> {
    let first_bytes = fa.peek_raw(1)?;
    let first = first_bytes[0];
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let bytes = fa.read_raw(len);
    if bytes.len() < len {
        return None;
    }
    // Strip the leading 1 in the first byte (at bit position 8-len).
    let marker_mask: u8 = if len == 8 { 0 } else { !(0xFF << (8 - len)) };
    let mut v: u64 = (bytes[0] & marker_mask) as u64;
    for b in &bytes[1..] {
        v = (v << 8) | (*b as u64);
    }
    Some(v)
}

/// Walk EBML elements within a region. Visitor receives
/// (fa, element_id, body_size, element_start_offset).
fn walk_elements(
    fa: &mut FileAnalyze,
    region_size: usize,
    visit: &mut dyn FnMut(&mut FileAnalyze, u64, usize, usize),
) {
    let region_end = fa.Element_Offset() + region_size;
    while fa.Element_Offset() < region_end && fa.Remain() > 0 {
        let elem_start = fa.Element_Offset();
        let Some(id) = read_vint_id(fa) else { break };
        let Some(size) = read_vint_size(fa) else { break };
        let body_size = size as usize;
        if fa.Element_Offset() + body_size > region_end {
            // Truncated — bail.
            break;
        }
        let body_end = fa.Element_Offset() + body_size;
        visit(fa, id, body_size, elem_start);
        if fa.Element_Offset() < body_end {
            fa.Skip_Hexa(body_end - fa.Element_Offset(), "element_tail");
        } else if fa.Element_Offset() > body_end {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vint_single_byte_id() {
        let buf = [0x82];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_id(&mut fa), Some(0x82));
    }

    #[test]
    fn vint_two_byte_id() {
        let buf = [0x42, 0x86];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_id(&mut fa), Some(0x4286));
    }

    #[test]
    fn vint_four_byte_id() {
        let buf = [0x1A, 0x45, 0xDF, 0xA3];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_id(&mut fa), Some(0x1A45DFA3));
    }

    #[test]
    fn vint_size_strips_marker_bit() {
        // 0x82 = 10000010, len=1, value with marker stripped = 0b0000010 = 2
        let buf = [0x82];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_size(&mut fa), Some(2));

        // 0x40 0x05 = 01000000 00000101, len=2, value = 5
        let buf = [0x40, 0x05];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_size(&mut fa), Some(5));
    }

    #[test]
    fn rejects_non_ebml_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a Matroska file at all");
        assert!(!parse_mkv(&mut fa));
    }
}
