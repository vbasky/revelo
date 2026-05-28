//! IAMF (Immersive Audio Model and Formats) parser.
//!
//! Mirrors the `FileHeader_Begin` gate in MediaInfoLib's `File_Iamf.cpp`.
//! An IAMF bitstream is a sequence of OBUs; a standalone (non-sub) stream
//! must begin with `OBU_IA_Sequence_Header` (obu_type = 31).
//!
//! First byte layout (big-endian bit fields):
//!   bits 7..3 : obu_type            (must be 31 = 0b11111)
//!   bit  2    : obu_redundant_copy  (0 or 1)
//!   bit  1    : reserved            (must be 0)
//!   bit  0    : obu_extension_flag  (0 or 1, unused in IAMF v2)
//!
//! That yields exactly four valid first-byte patterns: 0xF8, 0xF9, 0xFC, 0xFD.

use revelio_core::{FileAnalyze, StreamKind};

const MIN_HEADER_LEN: usize = 6;

pub fn parse_iamf(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < MIN_HEADER_LEN {
        return false;
    }
    let head = match fa.peek_raw(fa.Remain().min(MIN_HEADER_LEN)) {
        Some(h) if h.len() >= 1 => h,
        _ => return false,
    };
    let b0 = head[0];
    // Gate: obu_type=31 sequence header, reserved bit clear.
    if b0 != 0xF8 && b0 != 0xF9 && b0 != 0xFC && b0 != 0xFD {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "IAMF", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "IAMF", false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_iamf(first_byte: u8) -> Vec<u8> {
        // 6 bytes minimum to satisfy the header-size gate; payload contents
        // beyond the first byte are not inspected by FileHeader_Begin.
        let mut buf = vec![first_byte];
        buf.extend_from_slice(&[0u8; 8]);
        buf
    }

    #[test]
    fn parses_iamf_sequence_header() {
        for &b in &[0xF8u8, 0xF9, 0xFC, 0xFD] {
            let buf = make_iamf(b);
            let mut fa = FileAnalyze::new(&buf);
            assert!(parse_iamf(&mut fa), "first byte {:#X} should be accepted", b);
            let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
            let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
            assert_eq!(g("Format").as_deref(), Some("IAMF"));
            assert_eq!(g("AudioCount").as_deref(), Some("1"));
            assert_eq!(a("Format").as_deref(), Some("IAMF"));
            assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
        }
    }

    #[test]
    fn rejects_non_iamf_buffer() {
        // 0x00 = obu_type=0 (Codec Config) — not a sequence header.
        let buf = make_iamf(0x00);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_iamf(&mut fa));
        // ASCII text should also be rejected.
        let mut fa2 = FileAnalyze::new(b"NOT IAMF........");
        assert!(!parse_iamf(&mut fa2));
    }

    #[test]
    fn rejects_when_reserved_bit_set() {
        // 0xFA = obu_type=31, redundant=0, reserved=1 (invalid), ext=0.
        // 0xFE = obu_type=31, redundant=1, reserved=1 (invalid), ext=0.
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
}
