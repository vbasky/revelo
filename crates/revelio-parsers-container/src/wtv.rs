//! WTV (Windows Recorded TV Show) parser.
//!
//! Matches MediaInfoLib's `File_Wtv.cpp`: the file begins with a fixed
//! 16-byte GUID identifying the WTV container. The C++ parser does
//! magic-only detection and fills General.Format = "WTV"; no codec or
//! stream walking is performed at this level.

use revelio_core::{FileAnalyze, StreamKind};

const WTV_MAGIC: [u8; 16] = [
    0xB7, 0xD8, 0x00, 0x20, 0x37, 0x49, 0xDA, 0x11, 0xA6, 0x4E, 0x00, 0x07, 0xE9, 0x5E, 0xAD, 0x8D,
];

pub fn parse_wtv(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.Remain().min(16)) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 16 {
        return false;
    }
    if buf[..16] != WTV_MAGIC {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "WTV", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    fn build_wtv_header() -> Vec<u8> {
        let mut v = Vec::with_capacity(128);
        v.extend_from_slice(&WTV_MAGIC);
        // Pad out with zeros to a plausible header size.
        v.resize(128, 0);
        v
    }

    #[test]
    fn rejects_non_wtv() {
        let mut fa = FileAnalyze::new(b"This is definitely not a WTV file, just garbage data here!!");
        assert!(!parse_wtv(&mut fa));
    }

    #[test]
    fn rejects_truncated_buffer() {
        // Only 8 bytes — not enough for the 16-byte GUID.
        let mut fa = FileAnalyze::new(&WTV_MAGIC[..8]);
        assert!(!parse_wtv(&mut fa));
    }

    #[test]
    fn accepts_wtv_header() {
        let buf = build_wtv_header();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wtv(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("WTV".to_owned())
        );
    }
}
