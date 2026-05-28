//! EIA-708 (CEA-708) digital closed caption parser.
//!
//! EIA-708 has no file-level magic — in the C++ engine (`File_Eia708.cpp`)
//! it is `MustSynchronize=true` and only ever instantiated as a sub-parser
//! from container/MPEG video streams once the caption packet stream is
//! already identified by the upstream demuxer (DTVCC/cc_type). The parser
//! simply declares Format=EIA-708 on General and Text streams.
//!
//! This Rust port mirrors that behavior: any non-empty buffer is accepted
//! and the same fields are filled. Because there is no magic to validate
//! against, callers must place it at the very end of the dispatch chain
//! so a real format isn't overridden.

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_eia708(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() == 0 {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "EIA-708", false);

    fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, 0, "Format", "EIA-708", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_buffer() {
        let mut fa = FileAnalyze::new(b"");
        assert!(!parse_eia708(&mut fa));
    }

    #[test]
    fn accepts_any_non_empty_buffer() {
        let mut fa = FileAnalyze::new(&[0x00, 0x01, 0x02, 0x03]);
        assert!(parse_eia708(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("EIA-708"));
        assert_eq!(t("Format").as_deref(), Some("EIA-708"));
        assert_eq!(fa.Count_Get(StreamKind::Text), 1);
        assert_eq!(fa.Count_Get(StreamKind::General), 1);
    }
}
