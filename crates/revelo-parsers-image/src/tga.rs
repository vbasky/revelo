//! TGA (Truevision Targa) parser.
//!
//! Fixed 18-byte header (little-endian):
//!   0  ID Length
//!   1  Color Map Type (0 = none, 1 = present)
//!   2  Image Type (1/2/3 raw, 9/10/11 RLE, 32/33 Huffman)
//!   3..5  First Entry Index (u16)
//!   5..7  Color Map Length (u16)
//!   7  Color Map Entry Size (bits)
//!   8..10  X-origin (u16)
//!   10..12 Y-origin (u16)
//!   12..14 Image Width (u16)
//!   14..16 Image Height (u16)
//!   16  Pixel Depth (bits per pixel; 8/16/24/32)
//!   17  Image Descriptor
//!   18..18+ID_Length  Image ID
//!
//! Version 2 has a 26-byte footer at end-of-file with the signature
//! "TRUEVISION-XFILE.\0" (18 bytes) at offset file_size-18.

use revelo_core::{FileAnalyze, StreamKind};

const V2_SIGNATURE: &[u8; 18] = b"TRUEVISION-XFILE.\0";

pub fn parse_tga(fa: &mut FileAnalyze) -> bool {
    let file_size = fa.remain();
    if file_size < 18 {
        return false;
    }
    let head = match fa.peek_raw(file_size) {
        Some(b) => b,
        None => return false,
    };
    let id_length = head[0];
    let color_map_type = head[1];
    let image_type = head[2];
    let pixel_depth = head[16];

    // Reject early on obviously-invalid headers (mirrors the C++).
    if image_type == 0 || pixel_depth > 32 {
        return false;
    }
    // Image Type must be one of the supported codes.
    if !matches!(image_type, 1 | 2 | 3 | 9 | 10 | 11 | 32 | 33) {
        return false;
    }
    // Color map consistency: image types 1/9 require map, others reject if present.
    match image_type {
        1 | 9 => {
            if color_map_type != 1 {
                return false;
            }
        }
        _ => {
            if color_map_type != 0 {
                return false;
            }
        }
    }
    // Pixel depth must be 8, 16, 24, or 32.
    if !matches!(pixel_depth, 8 | 16 | 24 | 32) {
        return false;
    }

    let _first_entry_index = u16::from_le_bytes([head[3], head[4]]);
    let _color_map_length = u16::from_le_bytes([head[5], head[6]]);
    let _color_map_entry_size = head[7];
    let width = u16::from_le_bytes([head[12], head[13]]);
    let height = u16::from_le_bytes([head[14], head[15]]);

    if width == 0 || height == 0 {
        return false;
    }

    // Read the Image ID string (variable length, immediately after the header).
    let image_id: String = if id_length > 0 && (18 + id_length as usize) <= head.len() {
        let raw = &head[18..18 + id_length as usize];
        String::from_utf8_lossy(raw).trim_end_matches('\0').to_string()
    } else {
        String::new()
    };

    // Detect Version 2 by footer signature.
    let version =
        if file_size >= 26 && &head[file_size - 18..file_size] == V2_SIGNATURE { 2u8 } else { 1 };

    fa.stream_prepare(StreamKind::Image);
    let _pos = 0usize;
    let format = tga_image_type_compression(image_type);
    let color_space = tga_image_type_color_space(image_type);

    // General is filled after Image so Format_Version (via Streams_Finish in C++) lands at the end.
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "TGA");
    if !image_id.is_empty() {
        fa.set_field(StreamKind::General, 0, "Title", image_id);
    }
    fa.set_field(StreamKind::General, 0, "ImageCount", "1");
    // Oracle only emits Format_Version when the v2 footer signature is
    // present (mirrors C++: `if (Version) Fill(...)` where Version stays
    // at 0 for files without the trailing TRUEVISION-XFILE marker).
    if version == 2 {
        fa.set_field(StreamKind::General, 0, "Format_Version", "Version 2");
    }

    fa.set_field(StreamKind::Image, 0, "Format", format);
    fa.set_field(StreamKind::Image, 0, "CodecID", image_type.to_string());
    if !color_space.is_empty() {
        fa.set_field(StreamKind::Image, 0, "ColorSpace", color_space);
    }
    fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    fa.set_field(StreamKind::Image, 0, "BitDepth", pixel_depth.to_string());
    true
}

fn tga_image_type_compression(t: u8) -> &'static str {
    match t {
        1 => "Color-mapped",
        2 | 3 => "Raw",
        9 => "Color-mapped + RLE",
        10 | 11 => "RLE",
        32 | 33 => "Huffman",
        _ => "",
    }
}
fn tga_image_type_color_space(t: u8) -> &'static str {
    match t {
        1 | 2 | 9 | 10 | 32 | 33 => "RGB",
        3 | 11 => "Y",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_tga(image_type: u8, color_map_type: u8, w: u16, h: u16, depth: u8) -> Vec<u8> {
        let mut buf = vec![0u8; 18];
        buf[1] = color_map_type;
        buf[2] = image_type;
        buf[12..14].copy_from_slice(&w.to_le_bytes());
        buf[14..16].copy_from_slice(&h.to_le_bytes());
        buf[16] = depth;
        // Add a small body so the synthetic file is well-formed.
        buf.resize(buf.len() + 64, 0);
        buf
    }

    #[test]
    fn rejects_non_tga() {
        let mut fa = FileAnalyze::new(b"NOPE NOT TGA AT ALL");
        assert!(!parse_tga(&mut fa));
    }

    #[test]
    fn rejects_zero_image_type() {
        let buf = build_tga(0, 0, 100, 100, 24);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_tga(&mut fa));
    }

    #[test]
    fn parses_uncompressed_truecolor_tga() {
        let buf = build_tga(2, 0, 320, 240, 24);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_tga(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("TGA"));
        // V1 TGA has no footer signature → no Format_Version emitted.
        assert!(g("Format_Version").is_none());
        assert_eq!(i("Format").as_deref(), Some("Raw"));
        assert_eq!(i("ColorSpace").as_deref(), Some("RGB"));
        assert_eq!(i("Width").as_deref(), Some("320"));
        assert_eq!(i("Height").as_deref(), Some("240"));
        assert_eq!(i("BitDepth").as_deref(), Some("24"));
        assert_eq!(i("CodecID").as_deref(), Some("2"));
    }

    #[test]
    fn detects_version_2_footer() {
        let mut buf = build_tga(2, 0, 16, 16, 32);
        // Append v2 footer: 8 bytes (offsets) + 16 (signature start) + 1 (.) + 1 (\0)
        // = 26 bytes total. The last 18 bytes must be the signature.
        buf.extend_from_slice(&[0u8; 8]); // ext + dev offsets
        buf.extend_from_slice(V2_SIGNATURE);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_tga(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format_Version")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("Version 2")
        );
    }
}
