//! PGS (Blu-ray Presentation Graphic Stream) subtitle parser.
//!
//! A PGS segment header is 13 bytes:
//!   "PG"               magic (2 bytes)
//!   PTS                u32 BE (4 bytes)
//!   DTS                u32 BE (4 bytes)
//!   segment_type       u8
//!   segment_size       u16 BE (2 bytes)
//!
//! The C++ `File_Pgs` relies on container-level detection (MPEG-PS PID,
//! BDMV PlayList) and only fills `Text.Format = "PGS"` / `Text.Codec = "PGS"`.
//! For standalone .sup files we identify the stream by matching the "PG"
//! magic and validating that the first byte after the PTS/DTS pair is a
//! known PGS segment_type.

use mediainfo_core::{FileAnalyze, StreamKind};

const PGS_MAGIC: [u8; 2] = [b'P', b'G'];

// WHY: segment_type values defined by the BD spec. Anything outside this set
// is almost certainly not PGS, so we reject to avoid false positives on any
// file that happens to start with the bytes "PG".
const SEG_PALETTE: u8 = 0x14;
const SEG_OBJECT: u8 = 0x15;
const SEG_PRESENTATION_COMPOSITION: u8 = 0x16;
const SEG_WINDOW: u8 = 0x17;
const SEG_INTERACTIVE: u8 = 0x18;
const SEG_END_OF_DISPLAY: u8 = 0x80;

fn is_valid_segment_type(t: u8) -> bool {
    matches!(
        t,
        SEG_PALETTE
            | SEG_OBJECT
            | SEG_PRESENTATION_COMPOSITION
            | SEG_WINDOW
            | SEG_INTERACTIVE
            | SEG_END_OF_DISPLAY
    )
}

pub fn parse_pgs(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.Remain().min(13));
    let Some(h) = head else { return false };
    if h.len() < 13 {
        return false;
    }
    if h[0..2] != PGS_MAGIC {
        return false;
    }
    let segment_type = h[10];
    if !is_valid_segment_type(segment_type) {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "PGS", false);

    fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, 0, "Format", "PGS", false);
    fa.Fill(StreamKind::Text, 0, "Codec", "PGS", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_segment(seg_type: u8) -> Vec<u8> {
        let mut v = Vec::with_capacity(13);
        v.extend_from_slice(b"PG");
        v.extend_from_slice(&[0, 0, 0, 0]); // PTS
        v.extend_from_slice(&[0, 0, 0, 0]); // DTS
        v.push(seg_type);
        v.extend_from_slice(&[0, 0]); // segment_size
        v
    }

    #[test]
    fn accepts_presentation_composition() {
        let buf = make_segment(SEG_PRESENTATION_COMPOSITION);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pgs(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("PGS"));
        assert_eq!(t("Format").as_deref(), Some("PGS"));
        assert_eq!(t("Codec").as_deref(), Some("PGS"));
    }

    #[test]
    fn rejects_non_pgs() {
        let mut fa = FileAnalyze::new(b"RIFF\0\0\0\0WAVEfmt padding......");
        assert!(!parse_pgs(&mut fa));
    }

    #[test]
    fn rejects_invalid_segment_type() {
        // "PG" magic but segment_type 0x42 is not a PGS segment.
        let buf = make_segment(0x42);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_pgs(&mut fa));
    }
}
