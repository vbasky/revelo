//! RLE (Run-Length Encoded) parser.
//!
//! RLE has no file-level magic. MediaInfo only instantiates it as a
//! sub-parser for a DVD subpicture private stream
//! (`File_MpegPs::ChooseParser_RLE`). It is not in the file-detection
//! table: a catch-all probe would claim WebVTT and every other
//! unrecognized file.

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_rle(fa: &mut FileAnalyze) -> bool {
    if fa.remain() == 0 {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "RLE");

    fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, 0, "Format", "RLE");
    fa.set_field(StreamKind::Text, 0, "Codec", "RLE");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_buffer() {
        let mut fa = FileAnalyze::new(b"");
        assert!(!parse_rle(&mut fa));
    }

    #[test]
    fn accepts_any_non_empty_buffer() {
        let mut fa = FileAnalyze::new(&[0x00, 0x01, 0x02, 0x03]);
        assert!(parse_rle(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("RLE"));
        assert_eq!(t("Format").as_deref(), Some("RLE"));
        assert_eq!(t("Codec").as_deref(), Some("RLE"));
    }

    #[test]
    fn fills_one_text_stream() {
        let mut fa = FileAnalyze::new(&[0xFFu8; 32]);
        assert!(parse_rle(&mut fa));
        assert_eq!(fa.stream_count(StreamKind::Text), 1);
        assert_eq!(fa.stream_count(StreamKind::General), 1);
        assert_eq!(fa.stream_count(StreamKind::Image), 0);
    }
}
