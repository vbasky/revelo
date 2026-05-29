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

use revelo_core::{FileAnalyze, Reader, StreamKind};

pub fn parse_theora(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    // Need at least signature(1) + "theora"(6) + version(3) = 10 bytes
    if r.remain() < 10 {
        return None;
    }

    r.element_begin("Theora");

    if r.be_u8("Signature")? != 0x80 {
        r.element_end();
        return None;
    }
    if r.peek_raw(6)? != b"theora" {
        r.element_end();
        return None;
    }
    r.skip(6)?; // "theora"

    let version = r.be_u24("Version")?;

    if (version & 0x030200) == 0x030200 {
        // Version 3.2.x. Body reads default to 0 on truncation (detection
        // still succeeds), matching the original behaviour.
        r.be_u16("FMBW");
        r.be_u16("FMBH");
        let picw = r.be_u24("PICW").unwrap_or(0);
        let pich = r.be_u24("PICH").unwrap_or(0);
        r.be_u8("PICX");
        r.be_u8("PICY");
        let frn = r.be_u32("FRN").unwrap_or(0);
        let frd = r.be_u32("FRD").unwrap_or(0);
        let parn = r.be_u24("PARN").unwrap_or(0);
        let pard = r.be_u24("PARD").unwrap_or(0);
        r.be_u8("CS"); // color space: 0=4:2:0, 2=4:2:2, 3=4:4:4
        let nombr = r.be_u24("NOMBR").unwrap_or(0); // nominal bitrate

        r.bits(|b| {
            b.skip(6); // QUAL
            b.skip(5); // KFGSHIFT
            b.skip(2); // PF (pixel format)
            b.skip(3); // Reserved
            Some(())
        });

        r.element_end();

        r.stream_prepare(StreamKind::Video);
        r.set_field(StreamKind::Video, 0, "Format", "Theora");
        r.set_field(StreamKind::Video, 0, "Codec", "Theora");

        if frn > 0 && frd > 0 {
            let frame_rate = frn as f64 / frd as f64;
            r.set_field(StreamKind::Video, 0, "FrameRate", format!("{:.3}", frame_rate));
        }

        let pixel_ratio = if parn > 0 && pard > 0 { parn as f64 / pard as f64 } else { 1.0 };

        r.set_field(StreamKind::Video, 0, "Width", picw.to_string());
        r.set_field(StreamKind::Video, 0, "Height", pich.to_string());

        if picw > 0 && pich > 0 {
            let dar = picw as f64 / pich as f64 * pixel_ratio;
            r.set_field(StreamKind::Video, 0, "DisplayAspectRatio", format!("{:.3}", dar));
        }

        if nombr > 0 {
            r.set_field(StreamKind::Video, 0, "BitRate_Nominal", nombr.to_string());
        }

        r.stream_prepare(StreamKind::General);
        r.set_field(StreamKind::General, 0, "Format", "Theora");

        return Some(());
    }

    // Version not 3.2.x — still accept minimal
    r.element_end();
    r.stream_prepare(StreamKind::Video);
    r.set_field(StreamKind::Video, 0, "Format", "Theora");
    r.set_field(StreamKind::Video, 0, "Codec", "Theora");
    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "Theora");

    Some(())
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
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()), Some("Theora"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()), Some("640"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()), Some("480"));
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
