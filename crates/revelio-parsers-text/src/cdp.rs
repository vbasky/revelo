//! CDP (Closed-caption Data Packet, SMPTE 334-2) parser.
//!
//! On-disk shape per CDP packet starts with `cdp_identifier` = 0x9669
//! (big-endian) followed by `cdp_length`, `cdp_frame_rate` /
//! `time_code_present` flags, then section payloads carrying CEA-608 /
//! CEA-708 captions. MediaInfoLib's `File_Cdp` parses each section to
//! expose the embedded CEA-608/CEA-708 streams; this Rust parser
//! short-circuits at detection and only fills the General and Text
//! Format fields, since the broader engine does not yet model the
//! embedded caption sub-streams.

use revelio_core::{FileAnalyze, StreamKind};

const CDP_IDENTIFIER: [u8; 2] = [0x96, 0x69];

pub fn parse_cdp(fa: &mut FileAnalyze) -> bool {
    let want = 2usize.min(fa.remain());
    let Some(head) = fa.peek_raw(want) else {
        return false;
    };
    if head.len() < 2 || head[..2] != CDP_IDENTIFIER {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.force_field(StreamKind::General, 0, "Format", "CDP");

    fa.stream_prepare(StreamKind::Text);
    fa.force_field(StreamKind::Text, 0, "Format", "CDP");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cdp_magic() {
        // Minimal CDP-shaped buffer: identifier + length + framerate/flags +
        // some payload. Detection only needs the 2-byte magic.
        let buf: &[u8] = &[0x96, 0x69, 0x0A, 0x5F, 0x43, 0x00, 0x00, 0x72, 0xF4, 0xFF];
        let mut fa = FileAnalyze::new(buf);
        assert!(parse_cdp(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("CDP")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("CDP")
        );
        assert_eq!(fa.stream_count(StreamKind::Text), 1);
    }

    #[test]
    fn rejects_non_cdp() {
        let mut fa = FileAnalyze::new(b"NOT CDP DATA");
        assert!(!parse_cdp(&mut fa));
    }

    #[test]
    fn rejects_swapped_magic() {
        // 0x6996 must not be accepted — the identifier is big-endian
        // 0x9669, so a byte-swapped variant is not a CDP packet.
        let buf: &[u8] = &[0x69, 0x96, 0x0A, 0x5F];
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_cdp(&mut fa));
    }
}
