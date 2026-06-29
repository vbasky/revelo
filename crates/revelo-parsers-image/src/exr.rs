//! OpenEXR (HDR image) parser.
//!
//! Header layout (little-endian):
//!   0..4    Magic: 0x76 0x2F 0x31 0x01
//!   4       Version (low byte of 32-bit version field)
//!   5..8    Flags (high 3 bytes; bit 0x02 = Tile, 0x10 = MultiPart)
//!   8..    Attribute list, terminated by an empty name (single 0 byte):
//!     name (null-terminated ASCII)
//!     type (null-terminated ASCII)
//!     size (u32 LE)
//!     value (size bytes)
//!
//! Common attributes:
//!   "compression"   type "compression" (1 byte): codec id
//!   "displayWindow" type "box2i" (16 bytes): xMin/yMin/xMax/yMax → Width/Height
//!   "pixelAspectRatio" type "float" (4 bytes LE f32)
//!   "framesPerSecond"  type "rational" (4+4 bytes: n, d) → FrameRate

use revelo_core::{FileAnalyze, StreamKind};

const EXR_MAGIC: [u8; 4] = [0x76, 0x2F, 0x31, 0x01];
const EXR_HEADER_SCAN_LIMIT: usize = 256 * 1024;
const EXR_ATTRIBUTE_TOKEN_LIMIT: usize = 1024;
const EXR_COMMENT_LIMIT: usize = 4096;

pub fn parse_exr(fa: &mut FileAnalyze) -> bool {
    let total = fa.remain();
    let head = match fa.peek_raw(8) {
        Some(b) => b,
        None => return false,
    };
    if head[0..4] != EXR_MAGIC {
        return false;
    }
    let version = head[4];
    let flags = u32::from_le_bytes([0, head[5], head[6], head[7]]);
    let is_tile = (flags & 0x200) != 0;

    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut compression: Option<&'static str> = None;
    let mut frame_rate: Option<f64> = None;
    let mut pixel_aspect_ratio: Option<f32> = None;
    let mut comments: Option<String> = None;

    let header_limit = total.min(EXR_HEADER_SCAN_LIMIT);
    let mut i = 8usize;
    while i < header_limit {
        // End-of-header sentinel: empty name (single null byte).
        if fa.peek_raw_at(i, 1) == Some(&[0][..]) {
            break;
        }
        let Some((name, after_name)) = read_exr_cstring(fa, i, header_limit) else {
            break;
        };
        let Some((type_str, after_type)) = read_exr_cstring(fa, after_name, header_limit) else {
            break;
        };
        let Some(size_bytes) = fa.peek_raw_at(after_type, 4) else { break };
        let size = u32::from_le_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]])
            as usize;
        let value_start = after_type + 4;
        let Some(value_end) = value_start.checked_add(size) else {
            break;
        };
        if value_end > header_limit {
            break;
        }

        match (name.as_str(), type_str.as_str()) {
            ("compression", "compression") if size == 1 => {
                if let Some(value) = fa.peek_raw_at(value_start, 1) {
                    compression = exr_compression(value[0]);
                }
            }
            ("displayWindow", "box2i") if size == 16 => {
                let Some(value) = fa.peek_raw_at(value_start, 16) else {
                    break;
                };
                let x_min = u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                let y_min = u32::from_le_bytes([value[4], value[5], value[6], value[7]]);
                let x_max = u32::from_le_bytes([value[8], value[9], value[10], value[11]]);
                let y_max = u32::from_le_bytes([value[12], value[13], value[14], value[15]]);
                width = x_max.wrapping_sub(x_min).wrapping_add(1);
                height = y_max.wrapping_sub(y_min).wrapping_add(1);
            }
            ("pixelAspectRatio", "float") if size == 4 => {
                let Some(value) = fa.peek_raw_at(value_start, 4) else {
                    break;
                };
                pixel_aspect_ratio =
                    Some(f32::from_le_bytes([value[0], value[1], value[2], value[3]]));
            }
            ("framesPerSecond", "rational") if size == 8 => {
                let Some(value) = fa.peek_raw_at(value_start, 8) else {
                    break;
                };
                let n = u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                let d = u32::from_le_bytes([value[4], value[5], value[6], value[7]]);
                if d > 0 {
                    frame_rate = Some(n as f64 / d as f64);
                }
            }
            ("comments", "string") => {
                if let Some(value) = fa.peek_raw_at(value_start, size.min(EXR_COMMENT_LIMIT)) {
                    comments = Some(String::from_utf8_lossy(value).to_string());
                }
            }
            _ => {}
        }
        i = value_end;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "EXR");
    fa.set_field(StreamKind::General, 0, "Format_Version", version.to_string());
    fa.set_field(StreamKind::General, 0, "ImageCount", "1");
    if let Some(c) = comments {
        fa.set_field(StreamKind::General, 0, "Comment", c);
    }

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", "EXR");
    fa.set_field(StreamKind::Image, 0, "Format_Version", version.to_string());
    fa.set_field(StreamKind::Image, 0, "Format_Profile", if is_tile { "Tile" } else { "Line" });
    if let Some(c) = compression {
        fa.set_field(StreamKind::Image, 0, "Format_Compression", c);
    }
    if width > 0 {
        fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    }
    if height > 0 {
        fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    }
    if width > 0 && height > 0 {
        let dar = width as f64 / height as f64;
        fa.set_field(StreamKind::Image, 0, "DisplayAspectRatio", format!("{:.3}", dar));
    }
    if let Some(par) = pixel_aspect_ratio {
        fa.set_field(StreamKind::Image, 0, "PixelAspectRatio", format!("{:.3}", par));
    }
    if let Some(fr) = frame_rate {
        fa.set_field(StreamKind::Image, 0, "FrameRate", format!("{:.3}", fr));
    }
    // EXR compression codes 0-4 are lossless; 5+ are lossy.
    if compression.is_some() {
        let lossless = matches!(
            compression,
            Some("raw") | Some("RLZ") | Some("ZIPS") | Some("ZIP") | Some("PIZ")
        );
        fa.set_field(
            StreamKind::Image,
            0,
            "Compression_Mode",
            if lossless { "Lossless" } else { "Lossy" },
        );
    }
    let file_size = fa.remain();
    fa.set_field(StreamKind::Image, 0, "StreamSize", file_size.to_string());
    fa.force_field(StreamKind::General, 0, "StreamSize", "0");
    true
}

fn read_exr_cstring(
    fa: &FileAnalyze,
    offset: usize,
    header_limit: usize,
) -> Option<(String, usize)> {
    let max_len = header_limit.checked_sub(offset)?.min(EXR_ATTRIBUTE_TOKEN_LIMIT);
    let bytes = fa.peek_raw_at(offset, max_len)?;
    let end = bytes.iter().position(|&b| b == 0)?;
    let value = std::str::from_utf8(&bytes[..end]).unwrap_or("").to_owned();
    Some((value, offset + end + 1))
}

fn exr_compression(v: u8) -> Option<&'static str> {
    match v {
        0 => Some("raw"),
        1 => Some("RLZ"),
        2 => Some("ZIPS"),
        3 => Some("ZIP"),
        4 => Some("PIZ"),
        5 => Some("PXR24"),
        6 => Some("B44"),
        7 => Some("B44A"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_exr(width: u32, height: u32, compression: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&EXR_MAGIC);
        buf.push(2); // version
        buf.extend_from_slice(&[0, 0, 0]); // flags (no tile)
        // displayWindow attribute
        buf.extend_from_slice(b"displayWindow\0box2i\0");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // x_min
        buf.extend_from_slice(&0u32.to_le_bytes()); // y_min
        buf.extend_from_slice(&(width - 1).to_le_bytes());
        buf.extend_from_slice(&(height - 1).to_le_bytes());
        // compression attribute
        buf.extend_from_slice(b"compression\0compression\0");
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.push(compression);
        // End sentinel
        buf.push(0);
        buf
    }

    #[test]
    fn rejects_non_exr() {
        let mut fa = FileAnalyze::new(b"NOT AN EXR FILE.........");
        assert!(!parse_exr(&mut fa));
    }

    #[test]
    fn parses_minimal_exr() {
        let buf = build_minimal_exr(320, 240, 3); // ZIP
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_exr(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("EXR"));
        assert_eq!(i("Format_Version").as_deref(), Some("2"));
        assert_eq!(i("Width").as_deref(), Some("320"));
        assert_eq!(i("Height").as_deref(), Some("240"));
        assert_eq!(i("Format_Compression").as_deref(), Some("ZIP"));
        assert_eq!(i("Format_Profile").as_deref(), Some("Line"));
    }

    #[test]
    fn exr_probe_stops_at_header_sentinel() {
        let mut buf = build_minimal_exr(320, 240, 3);
        buf.resize(1024 * 1024, 0);
        let mut fa = FileAnalyze::new(&buf);

        assert!(parse_exr(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, EXR_ATTRIBUTE_TOKEN_LIMIT);
    }
}
