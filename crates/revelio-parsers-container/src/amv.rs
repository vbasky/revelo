//! AMV (Chinese MP3-player video) container parser.
//!
//! Mirrors the detection MediaInfoLib performs in `File_Other.cpp` for the
//! AMV format — a RIFF-shaped wrapper used by cheap portable media players.
//! The reference `File_Amv.cpp` in MediaInfoLib only calls `Reject("AMV")`,
//! so this parser purposefully does not attempt to decode the inner
//! "LIST"/"amvh"/"strh" chunks; recognising the form type is enough to
//! populate `General.Format=AMV`, matching the C++ visible output.
//!
//! Magic: ASCII `"RIFF"` followed by a 4-byte little-endian size and the
//! form type `"AMV "` (note the trailing space).
//!
//! Layout walked (RIFF, all sizes little-endian):
//!   0x00  C4  "RIFF"
//!   0x04  L4  Size (file size minus 8)
//!   0x08  C4  Form type ("AMV ")
//!   0x0C  ... RIFF sub-chunks (not parsed — AMV's inner chunks are
//!              proprietary and the reference parser ignores them).

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::Int32u;

const FOURCC_RIFF: Int32u = u32::from_be_bytes(*b"RIFF");
const FOURCC_AMV: Int32u = u32::from_be_bytes(*b"AMV ");

/// Detect a RIFF/AMV container and fill `General.Format`. Returns `true`
/// when the magic + form type match; otherwise the FileAnalyze cursor is
/// left untouched (we only peek before committing).
pub fn parse_amv(fa: &mut FileAnalyze) -> bool {
    // Peek both fourcc fields without consuming so non-AMV RIFF variants
    // (WAVE, AVI, etc.) can still be tried by sibling parsers.
    let header = match fa.peek_raw(12) {
        Some(b) => b,
        None => return false,
    };
    let magic = Int32u::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let form = Int32u::from_be_bytes([header[8], header[9], header[10], header[11]]);
    if magic != FOURCC_RIFF || form != FOURCC_AMV {
        return false;
    }

    fa.element_begin("RIFF");
    let mut riff_id: Int32u = 0;
    fa.get_c4(&mut riff_id, "ID");
    let mut riff_size: Int32u = 0;
    fa.get_l4(&mut riff_size, "Size");
    let mut form_type: Int32u = 0;
    fa.get_c4(&mut form_type, "Type");
    fa.element_end();

    let _ = riff_size;

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "AMV", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_amv(payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(12 + payload.len());
        buf.extend_from_slice(b"RIFF");
        // RIFF size = bytes after this field. Form type (4) + payload.
        let size = (4 + payload.len()) as u32;
        buf.extend_from_slice(&size.to_le_bytes());
        buf.extend_from_slice(b"AMV ");
        buf.extend_from_slice(payload);
        buf
    }

    #[test]
    fn parses_minimal_amv_header() {
        let buf = make_amv(&[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_amv(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("AMV")
        );
    }

    #[test]
    fn ignores_inner_payload_but_still_recognises_format() {
        // Real AMV files contain LIST/hdrl etc. — we don't parse those,
        // but their presence must not break recognition.
        let mut payload = Vec::new();
        payload.extend_from_slice(b"LIST");
        payload.extend_from_slice(&8u32.to_le_bytes());
        payload.extend_from_slice(b"hdrl");
        payload.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
        let buf = make_amv(&payload);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_amv(&mut fa));
    }

    #[test]
    fn rejects_riff_wave_buffer() {
        // A WAV-shaped RIFF must not be misidentified as AMV.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_amv(&mut fa));
    }

    #[test]
    fn rejects_non_riff_buffer() {
        let buf = b"not a riff at all";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_amv(&mut fa));
    }

    #[test]
    fn rejects_truncated_header() {
        // Fewer than 12 bytes — can't even read the form type.
        let buf = b"RIFF\x04\x00\x00";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_amv(&mut fa));
    }
}
