//! RLE (Run-Length Encoded) parser.
//!
//! RLE has no file-level magic — in the C++ engine it is only ever
//! instantiated as a sub-parser from MPEG-PS for DVD subtitle streams
//! (`File_MpegPs::ChooseParser_RLE`). It does not inspect any bytes;
//! it simply declares Format=RLE on the General and Text streams.
//!
//! This parser mirrors that behavior: any non-empty buffer is accepted
//! and the same two streams are filled. Because there is no magic to
//! validate against, callers must place it at the very end of the
//! dispatch chain so a real format isn't overridden.

use revelio_core::{FileAnalyze, StreamKind};

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
