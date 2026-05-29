//! BPG (Better Portable Graphics) parser.
//!
//! From <http://bellard.org/bpg/bpg_spec.txt>
//!
//! Header layout:
//!   4 bytes  magic 0x42 0x50 0x47 0xFB ("BPG\xFB")
//!   1 byte (bit-packed):
//!     3 bits  pixel_format (0=Y, 1/4=420, 2/5=422, 3=444)
//!     1 bit   alpha1_flag
//!     4 bits  bit_depth_minus_8
//!   1 byte (bit-packed):
//!     4 bits  color_space (0=YUV BT.601, 1=RGB, 2=YCgCo,
//!                          3=YUV BT.709, 4=YUV BT.2020)
//!     1 bit   extension_present
//!     1 bit   alpha2_flag
//!     1 bit   limited_range
//!     1 bit   reserved
//!   variable_size_integer  picture_width
//!   variable_size_integer  picture_height
//!
//! variable_size_integer (VSI): leading-1 prefix marks byte count.
//!   0xxxxxxx                              → 7 bits
//!   10xxxxxx 8bits                        → 14 bits
//!   110xxxxx 8bits 8bits                  → 21 bits
//!   1110xxxx 8bits×3                      → 28 bits
//!   11110xxx 8bits×4                      → 35 bits

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_bpg(fa: &mut FileAnalyze) -> bool {
    // Peek the smaller of (16, remaining bytes) — the BPG header is
    // very short and tests build minimal buffers (often <16 bytes).
    let want = fa.remain().min(16);
    if want < 6 {
        return false;
    }
    let h = match fa.peek_raw(want) {
        Some(b) => b,
        None => return false,
    };
    if h[0] != 0x42 || h[1] != 0x50 || h[2] != 0x47 || h[3] != 0xFB {
        return false;
    }
    let pixel_format = (h[4] >> 5) & 0x07;
    let _alpha1 = (h[4] >> 4) & 0x01 != 0;
    let bit_depth_minus_8 = h[4] & 0x0F;
    let color_space = (h[5] >> 4) & 0x0F;
    let _ext_present = (h[5] >> 3) & 0x01 != 0;
    let _alpha2 = (h[5] >> 2) & 0x01 != 0;
    let _limited_range = (h[5] >> 1) & 0x01 != 0;

    let (width, w_used) = read_vsi(&h[6..]);
    let (height, _) = read_vsi(&h[6 + w_used..]);

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "BPG");
    fa.set_field(StreamKind::General, 0, "ImageCount", "1");

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", "BPG");
    fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    let cs = bpg_color_space(color_space);
    if !cs.is_empty() {
        fa.set_field(StreamKind::Image, 0, "ColorSpace", cs);
    }
    let cm = bpg_pixel_format(pixel_format);
    if !cm.is_empty() && pixel_format != 0 {
        fa.set_field(StreamKind::Image, 0, "ChromaSubsampling", cm);
    }
    fa.set_field(StreamKind::Image, 0, "BitDepth", (bit_depth_minus_8 as u32 + 8).to_string());
    let cp = bpg_colour_primaries(color_space);
    if !cp.is_empty() {
        fa.set_field(StreamKind::Image, 0, "colour_primaries", cp);
    }
    true
}

/// Read a BPG variable-size-integer from the buffer. Returns (value,
/// bytes_consumed).
fn read_vsi(buf: &[u8]) -> (u64, usize) {
    if buf.is_empty() {
        return (0, 0);
    }
    let first = buf[0];
    // Count leading 1-bits in the first byte to get encoded length.
    let mut byte_count = 1usize;
    let mut mask = 0x80u8;
    while byte_count < 5 && (first & mask) != 0 {
        byte_count += 1;
        mask >>= 1;
    }
    if buf.len() < byte_count {
        return (0, 0);
    }
    // First byte: low (8 - byte_count) bits are data. (For 1-byte: 7 bits;
    // for 2-byte: 6 bits; etc.)
    let prefix_bits = byte_count as u32;
    let first_data_bits = 8 - prefix_bits;
    let first_mask: u8 = (1u8 << first_data_bits) - 1;
    let mut value: u64 = (first & first_mask) as u64;
    for i in 1..byte_count {
        value = (value << 8) | (buf[i] as u64);
    }
    (value, byte_count)
}

fn bpg_color_space(c: u8) -> &'static str {
    match c {
        0 | 3 | 4 => "YUV",
        1 => "RGB",
        2 => "YCgCo",
        _ => "",
    }
}
fn bpg_colour_primaries(c: u8) -> &'static str {
    match c {
        0 => "BT.601",
        3 => "BT.701",
        4 => "BT.2020",
        _ => "",
    }
}
fn bpg_pixel_format(p: u8) -> &'static str {
    match p {
        0 => "Grayscale",
        1 | 4 => "4:2:0",
        2 | 5 => "4:2:2",
        3 => "4:4:4",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vsi_encode(v: u64) -> Vec<u8> {
        // Encode as the smallest VSI that fits.
        if v < (1 << 7) {
            vec![v as u8]
        } else if v < (1 << 14) {
            vec![0x80 | ((v >> 8) as u8 & 0x3F), v as u8]
        } else if v < (1 << 21) {
            vec![0xC0 | ((v >> 16) as u8 & 0x1F), (v >> 8) as u8, v as u8]
        } else {
            // Larger values not used in tests.
            unimplemented!()
        }
    }

    fn build_bpg(
        pixel_format: u8,
        alpha: bool,
        bit_depth: u8,
        color_space: u8,
        w: u32,
        h: u32,
    ) -> Vec<u8> {
        let mut buf = vec![0x42, 0x50, 0x47, 0xFB];
        let b4 = ((pixel_format & 0x7) << 5) | ((alpha as u8) << 4) | ((bit_depth - 8) & 0x0F);
        let b5 = (color_space & 0x0F) << 4;
        buf.push(b4);
        buf.push(b5);
        buf.extend(vsi_encode(w as u64));
        buf.extend(vsi_encode(h as u64));
        buf
    }

    #[test]
    fn rejects_non_bpg() {
        let mut fa = FileAnalyze::new(b"NOT A BPG FILE.....");
        assert!(!parse_bpg(&mut fa));
    }

    #[test]
    fn parses_minimal_bpg() {
        let buf = build_bpg(1, false, 8, 0, 1920, 1080);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_bpg(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("BPG"));
        assert_eq!(i("Width").as_deref(), Some("1920"));
        assert_eq!(i("Height").as_deref(), Some("1080"));
        assert_eq!(i("BitDepth").as_deref(), Some("8"));
        assert_eq!(i("ColorSpace").as_deref(), Some("YUV"));
        assert_eq!(i("ChromaSubsampling").as_deref(), Some("4:2:0"));
        assert_eq!(i("colour_primaries").as_deref(), Some("BT.601"));
    }

    #[test]
    fn parses_small_dimensions_single_byte_vsi() {
        let buf = build_bpg(0, false, 10, 1, 64, 64); // grayscale 10-bit RGB
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_bpg(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Width").as_deref(), Some("64"));
        assert_eq!(i("Height").as_deref(), Some("64"));
        assert_eq!(i("BitDepth").as_deref(), Some("10"));
        assert_eq!(i("ColorSpace").as_deref(), Some("RGB"));
        // pixel_format=0 (grayscale) → no ChromaSubsampling
        assert!(i("ChromaSubsampling").is_none());
    }
}
