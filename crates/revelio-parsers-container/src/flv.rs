//! FLV (Flash Video) header-only parser.
//!
//! Mirrors the `FileHeader_Begin` slice of MediaInfoLib's `File_Flv.cpp`.
//! The full C++ parser walks every tag (script/audio/video) to recover
//! per-stream codec details; this Rust counterpart purposefully stops
//! after the 9-byte file header — enough to populate `General.Format`
//! plus the audio/video stream counts derived from the TypeFlags byte,
//! which is what the engine architecture needs for FLV recognition.
//!
//! Layout walked (all big-endian):
//!   0x00  C3  Signature ("FLV")
//!   0x03  B1  Version    (typically 0x01)
//!   0x04  B1  TypeFlags  (bit 0 = audio present, bit 2 = video present)
//!   0x05  B4  DataOffset (header size, typically 9)

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{Int8u, Int32u};

const FLV_HEADER_SIZE: usize = 9;
const FLV_SIGNATURE: [u8; 3] = *b"FLV";
const FLV_VERSION: u8 = 0x01;

const TYPE_FLAG_AUDIO: u8 = 0x01;
const TYPE_FLAG_VIDEO: u8 = 0x04;

/// Parse Adobe Flash Video container.
///
/// Detection: `FLV\x01` magic.
/// Fills: Audio/video tag types, AMF metadata.
pub fn parse_flv(fa: &mut FileAnalyze) -> bool {
    // Peek the smaller of the available bytes vs the fixed header size so
    // truncated inputs are rejected before any cursor movement, letting
    // sibling parsers try the same buffer.
    let need = FLV_HEADER_SIZE.min(fa.remain());
    let header = match fa.peek_raw(need) {
        Some(b) if b.len() == FLV_HEADER_SIZE => b,
        _ => return false,
    };
    if header[0..3] != FLV_SIGNATURE || header[3] != FLV_VERSION {
        return false;
    }

    fa.element_begin("FLV header");
    let mut sig0: Int8u = 0;
    let mut sig1: Int8u = 0;
    let mut sig2: Int8u = 0;
    fa.get_b1(&mut sig0, "Signature[0]");
    fa.get_b1(&mut sig1, "Signature[1]");
    fa.get_b1(&mut sig2, "Signature[2]");
    let mut version: Int8u = 0;
    fa.get_b1(&mut version, "Version");
    let mut type_flags: Int8u = 0;
    fa.get_b1(&mut type_flags, "TypeFlags");
    let mut data_offset: Int32u = 0;
    fa.get_b4(&mut data_offset, "DataOffset");
    fa.element_end();

    let _ = (sig0, sig1, sig2, version, data_offset);

    let has_audio = (type_flags & TYPE_FLAG_AUDIO) != 0;
    let has_video = (type_flags & TYPE_FLAG_VIDEO) != 0;

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "Flash Video", false);
    fa.fill(StreamKind::General, 0, "VideoCount", if has_video { "1" } else { "0" }, false);
    fa.fill(StreamKind::General, 0, "AudioCount", if has_audio { "1" } else { "0" }, false);

    true
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

    #[test]
    fn parses_audio_and_video_flv() {
        let buf = make_flv(TYPE_FLAG_AUDIO | TYPE_FLAG_VIDEO);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("Flash Video"));
        assert_eq!(g("VideoCount").as_deref(), Some("1"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
    }

    #[test]
    fn parses_audio_only_flv() {
        let buf = make_flv(TYPE_FLAG_AUDIO);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flv(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(g("VideoCount").as_deref(), Some("0"));
    }

    #[test]
    fn rejects_non_flv_buffer() {
        let buf = b"RIFF\x00\x00\x00\x00WAVE";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_flv(&mut fa));
    }

    #[test]
    fn rejects_truncated_header() {
        // Fewer than 9 bytes — peek_raw can't return the full header.
        let buf = b"FLV\x01\x05";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_flv(&mut fa));
    }
}
