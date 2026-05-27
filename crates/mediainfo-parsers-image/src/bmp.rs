//! BMP (Windows Bitmap) parser.
//!
//! Layout:
//!   "BM" (2 bytes)
//!   bfSize, bfReserved, bfOffBits (12 bytes, all LE)
//!   DIB header: biSize (4 LE), biWidth (4 LE signed),
//!     biHeight (4 LE signed — negative means top-down),
//!     biPlanes (2 LE), biBitCount (2 LE), biCompression (4 LE), ...

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_bmp(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(30);
    let Some(h) = head else { return false };
    if &h[0..2] != b"BM" {
        return false;
    }
    let width = i32::from_le_bytes([h[18], h[19], h[20], h[21]]).unsigned_abs();
    let height = i32::from_le_bytes([h[22], h[23], h[24], h[25]]).unsigned_abs();
    let bit_count = u16::from_le_bytes([h[28], h[29]]);

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "Bitmap", false);
    fa.Fill(StreamKind::General, 0, "ImageCount", "1", false);

    fa.Stream_Prepare(StreamKind::Image);
    fa.Fill(StreamKind::Image, 0, "Format", "Raw", false);
    fa.Fill(StreamKind::Image, 0, "Width", width.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Height", height.to_string(), false);
    let color_space = if bit_count <= 8 { "Palette" } else { "RGB" };
    fa.Fill(StreamKind::Image, 0, "ColorSpace", color_space, false);
    fa.Fill(StreamKind::Image, 0, "BitDepth", bit_count.to_string(), false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_bmp() {
        let mut fa = FileAnalyze::new(b"NOT a BMP");
        assert!(!parse_bmp(&mut fa));
    }
}
