//! RealMedia (RM/RMVB) container parser.
//!
//! RealMedia files begin with the ".RMF" four-byte signature followed by a
//! chunked structure (PROP, MDPR, CONT, DATA, INDX, ...). The reference
//! implementation is `File_Rm.cpp` (MediaInfoLib). This Rust port mirrors
//! the `FileHeader_Begin` slice: magic detection and General.Format. The
//! per-chunk MDPR/PROP demux that produces audio/video streams is deferred
//! — the C++ parser delegates codec parsing to per-codec files
//! (Cook/RealVideo/etc.) with no Rust counterparts yet.
//!
//! Magic: 4 bytes ".RMF" (0x2E 0x52 0x4D 0x46) at file offset 0.
//!
//! Layout walked:
//!   ".RMF" signature (4 bytes)
//!   [chunked body — not parsed here]

use revelio_core::{FileAnalyze, StreamKind};

const SIGNATURE: &[u8; 4] = b".RMF";

pub fn parse_rm(fa: &mut FileAnalyze) -> bool {
    let head = match fa.peek_raw(fa.remain().min(4)) {
        Some(b) if b.len() == 4 => b,
        _ => return false,
    };
    if head != SIGNATURE {
        return false;
    }

    fa.element_begin("RM");
    fa.skip_b4("Signature");
    fa.element_end();

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "RealMedia", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_rm_header() {
        let mut buf = Vec::from(*SIGNATURE);
        // Append a plausible trailing chunk header; the parser ignores it.
        buf.extend_from_slice(&[0u8; 16]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_rm(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("RealMedia"),
        );
    }

    #[test]
    fn rejects_non_rm_buffer() {
        let mut fa = FileAnalyze::new(b"NOT A REALMEDIA FILE");
        assert!(!parse_rm(&mut fa));
    }

    #[test]
    fn rejects_truncated_buffer() {
        // Fewer than 4 bytes — peek_raw can't read a full signature.
        let mut fa = FileAnalyze::new(b".RM");
        assert!(!parse_rm(&mut fa));
    }
}
