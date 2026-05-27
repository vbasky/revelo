//! PNG parser — chunk-based image format.
//!
//! Layout:
//!   8-byte signature: 89 50 4E 47 0D 0A 1A 0A
//!   chunks: [length:u32 BE][type:4cc][length bytes data][crc:u32]
//! Only IHDR is needed to fill the standard image metadata fields.
//!
//! IHDR payload (13 bytes):
//!   4 bytes BE: width
//!   4 bytes BE: height
//!   1 byte:     bit_depth (1, 2, 4, 8, 16)
//!   1 byte:     color_type (0=Grayscale, 2=RGB, 3=Palette, 4=GA, 6=RGBA)
//!   1 byte:     compression_method (always 0)
//!   1 byte:     filter_method (always 0)
//!   1 byte:     interlace_method (0=none, 1=Adam7)

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::int32u;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1A\n";

pub fn parse_png(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(8);
    let Some(h) = head else { return false };
    if h != PNG_SIGNATURE {
        return false;
    }

    let file_size = fa.Remain();
    fa.Skip_Hexa(8, "signature");

    // First chunk must be IHDR.
    if fa.Remain() < 8 {
        return false;
    }
    let mut length: int32u = 0;
    fa.Get_B4(&mut length, "IHDR_length");
    let mut chunk_type: int32u = 0;
    fa.Get_C4(&mut chunk_type, "chunk_type");
    if chunk_type != u32::from_be_bytes(*b"IHDR") || length < 13 {
        return false;
    }
    let mut width: int32u = 0;
    fa.Get_B4(&mut width, "Width");
    let mut height: int32u = 0;
    fa.Get_B4(&mut height, "Height");
    let mut bit_depth: zenlib::int8u = 0;
    fa.Get_B1(&mut bit_depth, "BitDepth");
    let mut color_type: zenlib::int8u = 0;
    fa.Get_B1(&mut color_type, "ColorType");

    fill_streams(fa, file_size, width, height, bit_depth, color_type);
    true
}

fn fill_streams(
    fa: &mut FileAnalyze,
    file_size: usize,
    width: int32u,
    height: int32u,
    bit_depth: u8,
    color_type: u8,
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "PNG", false);
    fa.Fill(StreamKind::General, 0, "ImageCount", "1", false);
    fa.Fill(StreamKind::General, 0, "StreamSize", "0", true);

    fa.Stream_Prepare(StreamKind::Image);
    fa.Fill(StreamKind::Image, 0, "Format", "PNG", false);
    fa.Fill(StreamKind::Image, 0, "Format_Compression", "Deflate", false);
    fa.Fill(StreamKind::Image, 0, "Format_Settings_Packing", "Linear", false);
    fa.Fill(StreamKind::Image, 0, "Width", width.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Height", height.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "PixelAspectRatio", "1.000", false);
    if width > 0 && height > 0 {
        let dar = (width as f64) / (height as f64);
        fa.Fill(
            StreamKind::Image,
            0,
            "DisplayAspectRatio",
            format!("{:.3}", dar),
            false,
        );
    }
    let color_space = match color_type {
        0 => "Y",
        2 => "RGB",
        3 => "RGB",
        4 => "YA",
        6 => "RGBA",
        _ => "Unknown",
    };
    fa.Fill(StreamKind::Image, 0, "ColorSpace", color_space, false);
    fa.Fill(StreamKind::Image, 0, "BitDepth", bit_depth.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Compression_Mode", "Lossless", false);
    fa.Fill(StreamKind::Image, 0, "StreamSize", file_size.to_string(), false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_png_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a PNG file at all");
        assert!(!parse_png(&mut fa));
    }
}
