//! PSD / PSB (Adobe Photoshop Document) parser.
//!
//! Fixed 26-byte file header (big-endian):
//!   4 bytes  signature "8BPS"
//!   2 bytes  version: 1 = PSD, 2 = PSB
//!   6 bytes  reserved (zero)
//!   2 bytes  channel count (1..56)
//!   4 bytes  height
//!   4 bytes  width
//!   2 bytes  bits per channel (1, 8, 16, 32)
//!   2 bytes  color mode

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_psd(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(26);
    let Some(h) = head else { return false };
    if &h[0..4] != b"8BPS" {
        return false;
    }
    let version = u16::from_be_bytes([h[4], h[5]]);
    if version != 1 && version != 2 {
        return false;
    }
    let _channels = u16::from_be_bytes([h[12], h[13]]);
    let height = u32::from_be_bytes([h[14], h[15], h[16], h[17]]);
    let width = u32::from_be_bytes([h[18], h[19], h[20], h[21]]);
    let bits = u16::from_be_bytes([h[22], h[23]]);
    let color_mode = u16::from_be_bytes([h[24], h[25]]);

    let format = if version == 1 { "PSD" } else { "PSB" };
    let color_space = psd_color_mode(color_mode);

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", format);
    fa.set_field(StreamKind::General, 0, "ImageCount", "1");

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", format);
    fa.set_field(StreamKind::Image, 0, "Format_Version", version.to_string());
    if !color_space.is_empty() {
        fa.set_field(StreamKind::Image, 0, "ColorSpace", color_space);
    }
    fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    fa.set_field(StreamKind::Image, 0, "BitDepth", bits.to_string());
    fa.set_field(StreamKind::Image, 0, "Compression_Mode", "Lossless");
    true
}

fn psd_color_mode(c: u16) -> &'static str {
    match c {
        0 => "Bitmap",
        1 => "Grayscale",
        2 => "Indexed",
        3 => "RGB",
        4 => "CMYK",
        7 => "Multichannel",
        8 => "Duotone",
        9 => "Lab",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_psd_header(
        version: u16,
        w: u32,
        h: u32,
        bits: u16,
        color_mode: u16,
        channels: u16,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(26);
        buf.extend_from_slice(b"8BPS");
        buf.extend_from_slice(&version.to_be_bytes());
        buf.extend_from_slice(&[0u8; 6]); // reserved
        buf.extend_from_slice(&channels.to_be_bytes());
        buf.extend_from_slice(&h.to_be_bytes());
        buf.extend_from_slice(&w.to_be_bytes());
        buf.extend_from_slice(&bits.to_be_bytes());
        buf.extend_from_slice(&color_mode.to_be_bytes());
        buf
    }

    #[test]
    fn rejects_non_psd() {
        let mut fa = FileAnalyze::new(b"NOT A PSD FILE AT ALL XXXXXX");
        assert!(!parse_psd(&mut fa));
    }

    #[test]
    fn parses_minimal_psd() {
        let buf = make_psd_header(1, 800, 600, 8, 3, 3); // PSD, 800x600x8 RGB 3ch
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_psd(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("PSD"));
        assert_eq!(i("Format_Version").as_deref(), Some("1"));
        assert_eq!(i("Width").as_deref(), Some("800"));
        assert_eq!(i("Height").as_deref(), Some("600"));
        assert_eq!(i("BitDepth").as_deref(), Some("8"));
        assert_eq!(i("ColorSpace").as_deref(), Some("RGB"));
    }

    #[test]
    fn version_2_is_psb() {
        let buf = make_psd_header(2, 100, 50, 16, 4, 4);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_psd(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("PSB"));
        assert_eq!(i("ColorSpace").as_deref(), Some("CMYK"));
        assert_eq!(i("BitDepth").as_deref(), Some("16"));
    }
}
