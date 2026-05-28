//! Theora video parser — Ogg-mapped codec.
//!
//! Mirrors MediaInfoLib's `File_Theora.cpp`. Theora uses the Ogg
//! encapsulation; the parser receives the entire initial header
//! packet (identification header) in one buffer.
//!
//! Layout:
//!   B1: Signature (0x80)
//!   6 bytes: "theora"
//!   B3: Version (major.minor.revision)
//!   B2: FMBW  (frame width in macroblocks)
//!   B2: FMBH  (frame height in macroblocks)
//!   B3: PICW  (picture width)
//!   B3: PICH  (picture height)
//!   B1: PICX  (picture x offset)
//!   B1: PICY  (picture y offset)
//!   B4: FRN   (frame rate numerator)
//!   B4: FRD   (frame rate denominator)
//!   B3: PARN  (pixel aspect ratio numerator)
//!   B3: PARD  (pixel aspect ratio denominator)
//!   B1: CS    (color space)
//!   B3: NOMBR (nominal bitrate)
//!   bit-packed: QUAL(6) KFGSHIFT(5) PF(2) reserved(3)

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::Int32u;

pub fn parse_theora(fa: &mut FileAnalyze) -> bool {
    // Need at least signature(1) + "theora"(6) + version(3) = 10 bytes
    if fa.remain() < 10 {
        return false;
    }

    fa.element_begin("Theora");

    let mut signature: u8 = 0;
    fa.get_b1(&mut signature, "Signature");
    if signature != 0x80 {
        fa.element_end();
        return false;
    }

    // Check for "theora" signature bytes
    let sig_bytes = match fa.peek_raw(6) {
        Some(b) => b,
        None => {
            fa.element_end();
            return false;
        }
    };
    if sig_bytes != b"theora" {
        fa.element_end();
        return false;
    }
    fa.skip_hexa(6, "Signature");

    let mut version: Int32u = 0;
    fa.get_b3(&mut version, "Version");

    if (version & 0x030200) == 0x030200 {
        // Version 3.2.x
        fa.skip_b2("FMBW");
        fa.skip_b2("FMBH");

        let mut picw: Int32u = 0;
        let mut pich: Int32u = 0;
        fa.get_b3(&mut picw, "PICW");
        fa.get_b3(&mut pich, "PICH");

        fa.skip_b1("PICX");
        fa.skip_b1("PICY");

        let mut frn: Int32u = 0;
        let mut frd: Int32u = 0;
        fa.get_b4(&mut frn, "FRN");
        fa.get_b4(&mut frd, "FRD");

        let mut parn: Int32u = 0;
        let mut pard: Int32u = 0;
        fa.get_b3(&mut parn, "PARN");
        fa.get_b3(&mut pard, "PARD");

        fa.skip_b1("CS"); // color space: 0=4:2:0, 2=4:2:2, 3=4:4:4

        let mut nombr: Int32u = 0;
        fa.get_b3(&mut nombr, "NOMBR"); // nominal bitrate

        fa.bs_begin();
        fa.skip_s1(6, "QUAL");
        fa.skip_s1(5, "KFGSHIFT");
        fa.skip_s1(2, "PF"); // pixel format
        fa.skip_s1(3, "Reserved");
        fa.bs_end();

        fa.element_end();

        // Fill streams
        fa.stream_prepare(StreamKind::Video);
        fa.fill(StreamKind::Video, 0, "Format", "Theora", false);
        fa.fill(StreamKind::Video, 0, "Codec", "Theora", false);

        if frn > 0 && frd > 0 {
            let frame_rate = frn as f64 / frd as f64;
            fa.fill(StreamKind::Video, 0, "FrameRate", format!("{:.3}", frame_rate), false);
        }

        let pixel_ratio = if parn > 0 && pard > 0 {
            parn as f64 / pard as f64
        } else {
            1.0
        };

        fa.fill(StreamKind::Video, 0, "Width", picw.to_string(), false);
        fa.fill(StreamKind::Video, 0, "Height", pich.to_string(), false);

        if picw > 0 && pich > 0 {
            let dar = picw as f64 / pich as f64 * pixel_ratio;
            fa.fill(StreamKind::Video, 0, "DisplayAspectRatio", format!("{:.3}", dar), false);
        }

        if nombr > 0 {
            fa.fill(StreamKind::Video, 0, "BitRate_Nominal", nombr.to_string(), false);
        }

        fa.stream_prepare(StreamKind::General);
        fa.fill(StreamKind::General, 0, "Format", "Theora", false);

        return true;
    }

    // Version not 3.2.x — still accept minimal
    fa.element_end();
    fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, 0, "Format", "Theora", false);
    fa.fill(StreamKind::Video, 0, "Codec", "Theora", false);
    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "Theora", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_theora_identification() -> Vec<u8> {
        let mut buf = Vec::new();
        // Signature byte 0x80
        buf.push(0x80);
        // "theora"
        buf.extend_from_slice(b"theora");
        // Version = 0x030200 (3.2.0)
        buf.extend_from_slice(&0x030200u32.to_be_bytes()[1..]); // 3 bytes: 03 02 00
        // FMBW = 40, FMBH = 30
        buf.extend_from_slice(&40u16.to_be_bytes());
        buf.extend_from_slice(&30u16.to_be_bytes());
        // PICW = 640
        buf.extend_from_slice(&640u32.to_be_bytes()[1..]); // 3 bytes
        // PICH = 480
        buf.extend_from_slice(&480u32.to_be_bytes()[1..]); // 3 bytes
        // PICX = 0, PICY = 0
        buf.push(0);
        buf.push(0);
        // FRN = 30000, FRD = 1001 (29.97 fps)
        buf.extend_from_slice(&30000u32.to_be_bytes());
        buf.extend_from_slice(&1001u32.to_be_bytes());
        // PARN = 1, PARD = 1
        buf.extend_from_slice(&1u32.to_be_bytes()[1..]); // 3 bytes
        buf.extend_from_slice(&1u32.to_be_bytes()[1..]); // 3 bytes
        // CS = 0 (4:2:0)
        buf.push(0);
        // NOMBR = 500000
        buf.extend_from_slice(&500000u32.to_be_bytes()[1..]); // 3 bytes
        // bit-packed: QUAL(6)=0, KFGSHIFT(5)=0, PF(2)=0, reserved(3)=0
        buf.push(0);
        buf
    }

    #[test]
    fn rejects_short_buffer() {
        let buf = vec![0u8; 10];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_theora(&mut fa));
    }

    #[test]
    fn rejects_bad_signature() {
        let mut buf = make_theora_identification();
        buf[0] = 0x00;
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_theora(&mut fa));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = make_theora_identification();
        buf[1] = b'x'; // corrupt "theora"
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_theora(&mut fa));
    }

    #[test]
    fn parses_theora_identification() {
        let buf = make_theora_identification();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_theora(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()),
            Some("Theora")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()),
            Some("640")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()),
            Some("480")
        );
    }

    #[test]
    fn calculates_frame_rate() {
        let buf = make_theora_identification();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_theora(&mut fa));
        let fr = fa.retrieve(StreamKind::Video, 0, "FrameRate").map(|z| z.as_str().to_owned());
        // 30000/1001 ≈ 29.970
        assert!(fr.is_some() && fr.unwrap().starts_with("29.970"));
    }
}
