//! GIF parser. Header layout:
//!   "GIF87a" or "GIF89a" (6 bytes)
//!   Logical Screen Width  (2 bytes LE)
//!   Logical Screen Height (2 bytes LE)

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_gif(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(10);
    let Some(h) = head else { return false };
    if &h[0..4] != b"GIF8" || h[5] != b'a' {
        return false;
    }
    let profile = match h[4] {
        b'7' => "87a",
        b'9' => "89a",
        _ => return false,
    };
    let width = u16::from_le_bytes([h[6], h[7]]);
    let height = u16::from_le_bytes([h[8], h[9]]);

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "GIF", false);
    fa.fill(StreamKind::General, 0, "ImageCount", "1", false);

    fa.stream_prepare(StreamKind::Image);
    fa.fill(StreamKind::Image, 0, "Format", "GIF", false);
    fa.fill(StreamKind::Image, 0, "Format_Profile", profile, false);
    fa.fill(StreamKind::Image, 0, "Width", width.to_string(), false);
    fa.fill(StreamKind::Image, 0, "Height", height.to_string(), false);
    fa.fill(StreamKind::Image, 0, "PixelAspectRatio", "1.000", false);
    if width > 0 && height > 0 {
        let dar = (width as f64) / (height as f64);
        fa.fill(
            StreamKind::Image,
            0,
            "DisplayAspectRatio",
            format!("{:.3}", dar),
            false,
        );
    }
    fa.fill(StreamKind::Image, 0, "Compression_Mode", "Lossless", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_gif() {
        let mut fa = FileAnalyze::new(b"NOT a GIF");
        assert!(!parse_gif(&mut fa));
    }
}
