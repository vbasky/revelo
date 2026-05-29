//! VBI (Vertical Blanking Interval) raw-line sample parser.
//!
//! Carries Line 21 closed captions, VITC timecode, and Teletext per
//! SMPTE ST 436M / ETSI EN 300 706. Raw VBI lines have no in-band magic
//! — File_Vbi.cpp's `Read_Buffer_Continue` calls `Accept()` unconditionally
//! ("No way to detect non-VBI content") because the parser is only invoked
//! by an outer container (MXF VBI/ANC, GXF, etc.) that has already
//! identified the payload type. We mirror that contract here: any
//! non-empty buffer is accepted, and detection is the caller's job.

use revelo_core::{FileAnalyze, StreamKind};

const PEEK_BYTES: usize = 1;

pub fn parse_vbi(fa: &mut FileAnalyze) -> bool {
    let n = fa.remain().min(PEEK_BYTES);
    let buf = match fa.peek_raw(n) {
        Some(b) => b,
        None => return false,
    };
    if buf.is_empty() {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.force_field(StreamKind::General, 0, "Format", "VBI");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_core::FileAnalyze;

    #[test]
    fn accepts_any_non_empty_buffer() {
        // Matches File_Vbi.cpp Accept()-without-detection semantics:
        // raw VBI samples are unidentifiable so any non-empty payload is taken.
        let data = vec![0x80u8; 720];
        let mut fa = FileAnalyze::new(&data);
        assert!(parse_vbi(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("VBI".to_owned())
        );
    }

    #[test]
    fn rejects_empty_buffer() {
        let mut fa = FileAnalyze::new(&[]);
        assert!(!parse_vbi(&mut fa));
    }

    #[test]
    fn fills_general_format_only() {
        let data = [0xAAu8; 8];
        let mut fa = FileAnalyze::new(&data);
        assert!(parse_vbi(&mut fa));
        assert_eq!(fa.stream_count(StreamKind::General), 1);
        assert_eq!(fa.stream_count(StreamKind::Video), 0);
        assert_eq!(fa.stream_count(StreamKind::Audio), 0);
    }
}
