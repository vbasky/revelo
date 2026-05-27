//! EBU Tech 3264 (N19) / STL subtitle file parser.
//!
//! Detection mirrors `File_N19::FileHeader_Begin`: the first 1024 bytes are a
//! General Subtitle Information (GSI) block. Bytes 0..3 are the CPN
//! (ASCII Code Page Number) and bytes 3..11 are the DFC (Disk Format Code,
//! e.g. "STL30.01"). We accept the file when DFC begins with "STL" — the
//! C++ also checks bytes 8..11 == ".01" but the project spec only requires
//! the "STL" magic.

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_n19(fa: &mut FileAnalyze) -> bool {
    // GSI block is 1024 bytes, but we only need the first 11 to identify.
    let head = fa.peek_raw(fa.Remain().min(11));
    let Some(h) = head else { return false };
    if h.len() < 11 || &h[3..6] != b"STL" {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "N19", false);

    fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, 0, "Format", "N19", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_gsi(dfc: &[u8; 8]) -> Vec<u8> {
        let mut v = Vec::with_capacity(1024);
        v.extend_from_slice(b"850"); // CPN
        v.extend_from_slice(dfc); // DFC
        v.resize(1024, 0x20);
        v
    }

    #[test]
    fn accepts_stl30_01() {
        let buf = make_gsi(b"STL30.01");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_n19(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("N19"));
        assert_eq!(t("Format").as_deref(), Some("N19"));
    }

    #[test]
    fn rejects_non_n19() {
        let mut fa = FileAnalyze::new(b"RIFF....WAVEfmt some other binary format here padding");
        assert!(!parse_n19(&mut fa));
    }

    #[test]
    fn rejects_truncated_header() {
        let mut fa = FileAnalyze::new(b"850STL");
        assert!(!parse_n19(&mut fa));
    }
}
