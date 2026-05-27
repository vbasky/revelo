//! PCX (Personal Computer eXchange) image parser.
//!
//! Fixed 128-byte header (little-endian):
//!   0   Manufacturer (0x0A)
//!   1   Version (0/2/3/4/5)
//!   2   Encoding (1 = RLE)
//!   3   Bits per pixel (1/4/8/24)
//!   4..6   XMin (u16)
//!   6..8   YMin (u16)
//!   8..10  XMax (u16)
//!   10..12 YMax (u16)
//!   12..14 Horizontal DPI (u16)
//!   14..16 Vertical DPI (u16)
//!   16..64 Palette (48 bytes)
//!   64  Reserved
//!   65  ColorPlanes
//!   66..68 BytesPerLine (u16)
//!   ...
//!
//! Width  = XMax − XMin
//! Height = YMax − YMin

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_pcx(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(130);
    let Some(h) = head else { return false };
    // Validate the magic / version / encoding / bit-depth fields.
    if h[0] != 0x0A
        || h[1] > 0x05
        || h[2] != 0x01
        || !matches!(h[3], 1 | 4 | 8 | 24)
    {
        return false;
    }
    let version = h[1];
    let bits_per_pixel = h[3];
    let x_min = u16::from_le_bytes([h[4], h[5]]);
    let y_min = u16::from_le_bytes([h[6], h[7]]);
    let x_max = u16::from_le_bytes([h[8], h[9]]);
    let y_max = u16::from_le_bytes([h[10], h[11]]);
    let hor_dpi = u16::from_le_bytes([h[12], h[13]]);
    let vert_dpi = u16::from_le_bytes([h[14], h[15]]);
    let bytes_per_line = u16::from_le_bytes([h[66], h[67]]);

    // Integrity tests mirroring C++.
    if x_max <= x_min || y_max <= y_min || bytes_per_line < x_max - x_min {
        return false;
    }

    let width = (x_max - x_min) as u32;
    let height = (y_max - y_min) as u32;

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "PCX", false);
    fa.Fill(StreamKind::General, 0, "ImageCount", "1", false);

    fa.Stream_Prepare(StreamKind::Image);
    fa.Fill(StreamKind::Image, 0, "Format", "PCX", false);
    let v = pcx_version_info(version);
    if !v.is_empty() {
        fa.Fill(StreamKind::Image, 0, "Format_Version", v, false);
    }
    fa.Fill(StreamKind::Image, 0, "Width", width.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Height", height.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "BitDepth", bits_per_pixel.to_string(), false);
    // PCX uses RLE (encoding byte = 1) — always lossless.
    fa.Fill(StreamKind::Image, 0, "Compression_Mode", "Lossless", false);
    fa.Fill(
        StreamKind::Image,
        0,
        "DPI",
        format!("{} x {}", vert_dpi, hor_dpi),
        false,
    );
    true
}

fn pcx_version_info(v: u8) -> &'static str {
    match v {
        0 => "Paintbrush v2.5",
        2 => "Paintbrush v2.8 with palette information",
        3 => "Paintbrush v2.8 without palette information",
        4 => "Paintbrush/Windows",
        5 => "Paintbrush v3.0+",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_pcx(version: u8, bits: u8, width: u16, height: u16, dpi_h: u16, dpi_v: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 130];
        buf[0] = 0x0A;
        buf[1] = version;
        buf[2] = 0x01;
        buf[3] = bits;
        buf[4..6].copy_from_slice(&0u16.to_le_bytes());
        buf[6..8].copy_from_slice(&0u16.to_le_bytes());
        buf[8..10].copy_from_slice(&width.to_le_bytes());
        buf[10..12].copy_from_slice(&height.to_le_bytes());
        buf[12..14].copy_from_slice(&dpi_h.to_le_bytes());
        buf[14..16].copy_from_slice(&dpi_v.to_le_bytes());
        buf[66..68].copy_from_slice(&width.to_le_bytes());
        buf
    }

    #[test]
    fn rejects_non_pcx() {
        let mut fa = FileAnalyze::new(b"NOT PCX FILE.....................................................................................................................");
        assert!(!parse_pcx(&mut fa));
    }

    #[test]
    fn parses_minimal_pcx() {
        let buf = build_pcx(5, 24, 320, 240, 96, 96);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pcx(&mut fa));
        let i = |k: &str| fa.Retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("PCX"));
        assert_eq!(i("Format_Version").as_deref(), Some("Paintbrush v3.0+"));
        assert_eq!(i("Width").as_deref(), Some("320"));
        assert_eq!(i("Height").as_deref(), Some("240"));
        assert_eq!(i("BitDepth").as_deref(), Some("24"));
        assert_eq!(i("DPI").as_deref(), Some("96 x 96"));
    }
}
