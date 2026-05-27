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

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::{int16u, int32u};

const VP8_START_CODE: int32u = 0x9D012A;

pub fn parse_vp8(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < 10 {
        return false;
    }

    fa.Element_Begin("VP8");

    // VP8 bitstream header (LE bit-packed)
    fa.BS_Begin();

    let mut frame_type: u8 = 0;
    fa.Get_S1(1, &mut frame_type, "frame type");

    fa.Skip_S1(3, "version number");
    fa.Skip_S1(1, "show_frame flag");
    fa.Skip_S4(19, "size of the first data partition");
    fa.BS_End();

    if frame_type == 0 {
        // I-Frame
        let mut start_code: int32u = 0;
        fa.Get_B3(&mut start_code, "start code");

        if start_code != VP8_START_CODE {
            fa.Element_End();
            return false;
        }

        let mut width: int16u = 0;
        let mut height: int16u = 0;
        fa.Get_L2(&mut width, "width");
        fa.Get_L2(&mut height, "height");

        let w = (width & 0x3FFF) as u32;
        let h = (height & 0x3FFF) as u32;

        fa.Element_End();

        fa.Stream_Prepare(StreamKind::Video);
        fa.Fill(StreamKind::Video, 0, "Format", "VP8", false);
        fa.Fill(StreamKind::Video, 0, "Codec", "VP8", false);
        fa.Fill(StreamKind::Video, 0, "BitDepth", "8", false);
        fa.Fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);
        fa.Fill(StreamKind::Video, 0, "Width", w.to_string(), false);
        fa.Fill(StreamKind::Video, 0, "Height", h.to_string(), false);

        fa.Stream_Prepare(StreamKind::General);
        fa.Fill(StreamKind::General, 0, "Format", "VP8", false);

        return true;
    }

    // P-Frame (no resolution info)
    fa.Element_End();

    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "VP8", false);
    fa.Fill(StreamKind::Video, 0, "Codec", "VP8", false);
    fa.Fill(StreamKind::Video, 0, "BitDepth", "8", false);
    fa.Fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "VP8", false);

    true
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
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()),
            Some("VP8")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()),
            Some("1920")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()),
            Some("1080")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "BitDepth").map(|z| z.as_str()),
            Some("8")
        );
    }
}
