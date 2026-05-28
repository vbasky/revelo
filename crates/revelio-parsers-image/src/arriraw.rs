//! ARRI Raw (.ari) parser.
//!
//! Detection by 8-byte magic at offset 0:
//!   0..4    "ARRI" (0x41 0x52 0x52 0x49)
//!   4..8    0x12 0x34 0x56 0x78
//!
//! The C++ reference parser only fills Format and StreamSize — the
//! rest of the ARRI header is opaque payload for the purposes of this
//! engine, so we mirror that exactly.

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_arriraw(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.remain().min(8));
    let Some(h) = head else { return false };
    if h.len() < 8 {
        return false;
    }
    if &h[0..4] != b"ARRI" || h[4] != 0x12 || h[5] != 0x34 || h[6] != 0x56 || h[7] != 0x78 {
        return false;
    }

    let file_size = fa.remain();

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "Arri Raw", false);

    fa.stream_prepare(StreamKind::Image);
    fa.fill(StreamKind::Image, 0, "Format", "Arri Raw", false);
    fa.fill(StreamKind::Image, 0, "StreamSize", file_size.to_string(), false);
    fa.fill(StreamKind::General, 0, "StreamSize", "0", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_arriraw(extra_bytes: usize) -> Vec<u8> {
        let mut buf = vec![0u8; 8 + extra_bytes];
        buf[0..4].copy_from_slice(b"ARRI");
        buf[4] = 0x12;
        buf[5] = 0x34;
        buf[6] = 0x56;
        buf[7] = 0x78;
        buf
    }

    #[test]
    fn rejects_non_arriraw() {
        let mut fa = FileAnalyze::new(b"NOT an ARRI raw file...");
        assert!(!parse_arriraw(&mut fa));
    }

    #[test]
    fn rejects_partial_magic() {
        // First 4 bytes match but trailer is wrong.
        let mut buf = vec![0u8; 32];
        buf[0..4].copy_from_slice(b"ARRI");
        buf[4..8].copy_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_arriraw(&mut fa));
    }

    #[test]
    fn parses_minimal_arriraw() {
        let buf = build_minimal_arriraw(1024);
        let expected_size = buf.len();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_arriraw(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("Arri Raw"));
        assert_eq!(i("Format").as_deref(), Some("Arri Raw"));
        assert_eq!(i("StreamSize").as_deref(), Some(expected_size.to_string().as_str()));
    }
}
