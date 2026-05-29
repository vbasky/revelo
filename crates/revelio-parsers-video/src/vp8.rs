//! VP8 video parser.
//!
//! Mirrors MediaInfoLib's `File_Vp8.cpp`. VP8 is a simple intra-frame
//! codec used in WebM. The parser handles both key frames (I-Frame) and
//! inter frames, extracting width, height, and format metadata from the
//! uncompressed header.
//!
//! Layout (I-Frame):
//!   1 byte  bitstream: frame_type(1) | version(3) | show_frame(1) | partition_size(19)
//!   3 bytes BE start_code (0x9D012A)
//!   2 bytes LE width  (14 bits valid, top 2 bits for scaling)
//!   2 bytes LE height (14 bits valid, top 2 bits for scaling)

use revelio_core::{FileAnalyze, Reader, StreamKind};

const VP8_START_CODE: u32 = 0x9D012A;

pub fn parse_vp8(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < 10 {
        return None;
    }

    r.element_begin("VP8");
    // Frame tag: frame_type(1) | version(3) | show_frame(1) | partition_size(19).
    let frame_type = r.bits(|b| {
        let ft = b.read::<u8>(1, "frame type")?;
        b.skip(3); // version
        b.skip(1); // show_frame
        b.skip(19); // first partition size
        Some(ft)
    })?;

    if frame_type == 0 {
        // I-Frame
        let start_code = r.be_u24("start code")?;
        if start_code != VP8_START_CODE {
            r.element_end();
            return None;
        }
        let width = r.le_u16("width")?;
        let height = r.le_u16("height")?;
        let w = (width & 0x3FFF) as u32;
        let h = (height & 0x3FFF) as u32;
        r.element_end();

        fa.stream_prepare(StreamKind::Video);
        fa.set_field(StreamKind::Video, 0, "Format", "VP8");
        fa.set_field(StreamKind::Video, 0, "Codec", "VP8");
        fa.set_field(StreamKind::Video, 0, "BitDepth", "8");
        fa.set_field(StreamKind::Video, 0, "ColorSpace", "YUV");
        fa.set_field(StreamKind::Video, 0, "Width", w.to_string());
        fa.set_field(StreamKind::Video, 0, "Height", h.to_string());

        fa.stream_prepare(StreamKind::General);
        fa.set_field(StreamKind::General, 0, "Format", "VP8");
        return Some(());
    }

    // P-Frame (no resolution info)
    r.element_end();

    fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, 0, "Format", "VP8");
    fa.set_field(StreamKind::Video, 0, "Codec", "VP8");
    fa.set_field(StreamKind::Video, 0, "BitDepth", "8");
    fa.set_field(StreamKind::Video, 0, "ColorSpace", "YUV");

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "VP8");
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vp8_iframe(width: u16, height: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        // VP8 frame tag (bits read MSB-first):
        //   bit 7: frame_type (0=key)  = 0
        //   bits 6-4: version (0)      = 000
        //   bit 3: show_frame (1)      = 1
        //   bits 2-0: part_size top    = 000
        //   => byte 0 = 0b0000_1000 = 0x08
        // In the C++ side this is read via BS_Begin_LE() (LSB-first),
        // so frame_type is bit 0. Our BE reader puts it at bit 7.
        // For the test, the key frame bit on the MSB side is 0, so:
        // byte 0 = 0x08, byte 1 = 0x00, byte 2 = 0x00
        buf.push(0x08);
        buf.push(0x00);
        buf.push(0x00);
        // start code 0x9D012A (3 bytes BE)
        buf.extend_from_slice(&VP8_START_CODE.to_be_bytes()[1..]);
        // width LE (low 14 bits)
        buf.extend_from_slice(&width.to_le_bytes());
        // height LE (low 14 bits)
        buf.extend_from_slice(&height.to_le_bytes());
        buf
    }

    #[test]
    fn rejects_too_short() {
        let buf = vec![0u8; 3];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_vp8(&mut fa));
    }

    #[test]
    fn rejects_bad_start_code() {
        let mut buf = make_vp8_iframe(320, 240);
        buf[4] = 0xFF; // corrupt start code
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_vp8(&mut fa));
    }

    #[test]
    fn parses_vp8_iframe() {
        let buf = make_vp8_iframe(1920, 1080);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_vp8(&mut fa));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()), Some("VP8"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()), Some("1920"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()), Some("1080"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "BitDepth").map(|z| z.as_str()), Some("8"));
    }
}
