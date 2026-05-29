//! EIA-608 (CEA-608) closed caption parser.
//!
//! EIA-608 has no file-level magic — in the C++ engine (`File_Eia608.cpp`)
//! the parser is only ever instantiated as a sub-parser once a container
//! (MPEG-2 user_data, SCTE-20/21, MOV `c608`, ancillary VANC, etc.) has
//! already identified the payload as raw caption byte pairs. Any byte
//! pattern can be a valid caption pair, so detection at the file level
//! is impossible.
//!
//! This Rust port mirrors that behavior: any non-empty buffer is
//! accepted and Format=EIA-608 is declared on General and Text streams.
//! Because there is no magic to validate against, callers must place it
//! at the very end of the dispatch chain so a real format isn't
//! overridden.

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_eia608(fa: &mut FileAnalyze) -> bool {
    if fa.remain() == 0 {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "EIA-608");
    fa.set_field(StreamKind::General, 0, "TextCount", "1");

    fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, 0, "Format", "EIA-608");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_buffer() {
        let mut fa = FileAnalyze::new(b"");
        assert!(!parse_eia608(&mut fa));
    }

    #[test]
    fn accepts_any_non_empty_buffer() {
        let mut fa = FileAnalyze::new(&[0x94, 0x20, 0x94, 0xAE]);
        assert!(parse_eia608(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("EIA-608"));
        assert_eq!(t("Format").as_deref(), Some("EIA-608"));
        assert_eq!(fa.stream_count(StreamKind::Text), 1);
        assert_eq!(fa.stream_count(StreamKind::General), 1);
    }
}
