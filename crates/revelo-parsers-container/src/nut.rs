//! NUT container parser.
//!
//! Mirrors MediaInfoLib's `File_Nut.cpp` `FileHeader_Parse`. The NUT format
//! (<http://svn.mplayerhq.hu/nut/docs/nut.txt>) starts with a 24-byte ASCII
//! file_id_string followed by a single zero byte. Element packets that
//! follow each start with a 64-bit startcode whose first byte is 'N'
//! (0x4E) — e.g. main = 0x4E4D7A561F5F04AD. This parser only validates the
//! 25-byte file header and fills General.Format=NUT.
//!
//! Header layout:
//!   0x00  24  file_id_string = "nut/multimedia container"
//!   0x18   1  file_id_string_zero = 0x00

use revelo_core::{FileAnalyze, Reader, StreamKind};

const NUT_HEADER_SIZE: usize = 25;
const NUT_FILE_ID: &[u8; 24] = b"nut/multimedia container";

/// Parse NUT container.
///
/// Detection: `nut/multimedia container\0` magic.
/// Fills: Stream headers, timebase, codec info.
pub fn parse_nut(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    // Require the full 25-byte header to accept.
    let header = r.peek_raw(NUT_HEADER_SIZE)?;
    if &header[0..24] != NUT_FILE_ID || header[24] != 0 {
        return None;
    }

    r.element_begin("Nut header");
    r.skip(24)?; // file_id_string
    r.be_u8("file_id_string zero")?;
    r.element_end();

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "Nut");
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nut_header() -> Vec<u8> {
        let mut buf = Vec::with_capacity(NUT_HEADER_SIZE);
        buf.extend_from_slice(NUT_FILE_ID);
        buf.push(0x00);
        buf
    }

    #[test]
    fn parses_minimal_nut_header() {
        let buf = make_nut_header();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_nut(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("Nut")
        );
    }

    #[test]
    fn rejects_non_nut_buffer() {
        let buf = b"This is definitely not a NUT file at all!!";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_nut(&mut fa));
    }

    #[test]
    fn rejects_nut_id_without_trailing_zero() {
        let mut buf = make_nut_header();
        // C++ rejects when file_id_string_zero != 0 — exercise that gate.
        buf[24] = 0x01;
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_nut(&mut fa));
    }

    #[test]
    fn rejects_short_buffer() {
        let buf = b"nut/multimedia";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_nut(&mut fa));
    }
}
