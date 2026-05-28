//! LXF (Leitch / Harris broadcast) container parser.
//!
//! LXF is a broadcast server format from Leitch (later Harris/Imagine
//! Communications). The reference implementation lives in
//! `File_Lxf.cpp` (MediaInfoLib). This Rust port mirrors only the
//! `FileHeader_Begin` slice: magic detection and General.Format.
//! Per-packet video/audio demux is deferred — the C++ parser delegates
//! to File_Avc / File_Mpegv / File_Aac / File_Ac3 / etc., none of which
//! have Rust counterparts wired into this header-only path.
//!
//! Magic: 8 bytes "LEITCH\0\0" (0x4C 0x45 0x49 0x54 0x43 0x48 0x00 0x00)
//! at file offset 0.

use revelio_core::{FileAnalyze, StreamKind};

const SIGNATURE: &[u8; 8] = b"LEITCH\0\0";

pub fn parse_lxf(fa: &mut FileAnalyze) -> bool {
    let head = match fa.peek_raw(fa.Remain().min(8)) {
        Some(b) if b.len() == 8 => b,
        _ => return false,
    };
    if head != SIGNATURE {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "LXF", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_lxf_header() {
        let mut buf = Vec::from(*SIGNATURE);
        // Append a plausible packet-stream tail; the parser ignores it.
        buf.extend_from_slice(&[0u8; 32]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_lxf(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("LXF"),
        );
    }

    #[test]
    fn rejects_non_lxf_buffer() {
        let mut fa = FileAnalyze::new(b"NOT AN LXF FILE AT ALL");
        assert!(!parse_lxf(&mut fa));
    }

    #[test]
    fn rejects_truncated_buffer() {
        // Fewer than 8 bytes — peek_raw can't read a full signature.
        let mut fa = FileAnalyze::new(b"LEITCH");
        assert!(!parse_lxf(&mut fa));
    }
}
