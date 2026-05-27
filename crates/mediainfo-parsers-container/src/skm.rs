//! SKM (Sky Korean Mobilephone) container parser.
//!
//! SKM is an FLV-derived container used by the iSky Korean satellite
//! broadcast service. The reference implementation lives in
//! `File_Skm.cpp` (MediaInfoLib). This Rust port mirrors the
//! `FileHeader_Begin` + `FileHeader_Parse` slice: magic detection and
//! General.Format. Per-tag MPEG-4 Visual demux is deferred — the C++
//! parser delegates Data_Parse to File_Mpeg4v, which has no Rust
//! counterpart yet.
//!
//! Magic: 5 bytes "DMSKM" (0x44 0x4D 0x53 0x4B 0x4D) at file offset 0.
//!
//! Layout walked:
//!   "DMSKM" signature (5 bytes)
//!   [FLV-style tag stream — not parsed here]

use mediainfo_core::{FileAnalyze, StreamKind};

const SIGNATURE: &[u8; 5] = b"DMSKM";

pub fn parse_skm(fa: &mut FileAnalyze) -> bool {
    let head = match fa.peek_raw(fa.Remain().min(5)) {
        Some(b) if b.len() == 5 => b,
        _ => return false,
    };
    if head != SIGNATURE {
        return false;
    }

    fa.Element_Begin("SKM");
    fa.Skip_B5("Signature");
    fa.Element_End();

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "SKM", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_skm_header() {
        let mut buf = Vec::from(*SIGNATURE);
        // Append a plausible empty tag-stream tail; the parser ignores it.
        buf.extend_from_slice(&[0u8; 16]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_skm(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("SKM"),
        );
    }

    #[test]
    fn rejects_non_skm_buffer() {
        let mut fa = FileAnalyze::new(b"NOT A SKM FILE EITHER");
        assert!(!parse_skm(&mut fa));
    }

    #[test]
    fn rejects_truncated_buffer() {
        // Fewer than 5 bytes — peek_raw can't read a full signature.
        let mut fa = FileAnalyze::new(b"DMSK");
        assert!(!parse_skm(&mut fa));
    }
}
