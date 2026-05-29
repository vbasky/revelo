//! DPX / Cineon (SMPTE 268M digital cinema film) parser.
//!
//! Detection by 4-byte magic at offset 0:
//!   0x53445058 "SDPX" → DPX big-endian
//!   0x58504453 "XPDS" → DPX little-endian
//!   0x802A5FD7        → Cineon big-endian
//!   0xD75F2A80        → Cineon little-endian
//!
//! DPX File Information (768 bytes), endian per magic:
//!   0..4    Magic
//!   4..8    Image data offset
//!   8..16   Version (8 ASCII bytes, e.g. "V2.0    ")
//!   16..20  File size
//!   ...etc
//!
//! DPX Image Information starts at offset 768:
//!   768..770 ImageOrientation
//!   770..772 ImageElements
//!   772..776 Width
//!   776..780 Height
//!   780..    Image element 0 (72 bytes), then up to 7 more elements
//!
//! Image element 0 layout (72 bytes from offset 780):
//!   +0   DataSign (4)
//!   +4   RefLowDataCodeValue (4)
//!   +8   RefLowQuantity (4)
//!   +12  RefHighDataCodeValue (4)
//!   +16  RefHighQuantity (4)
//!   +20  Descriptor (1)
//!   +21  TransferCharacteristic (1)
//!   +22  ColorimetricSpecification (1)
//!   +23  BitDepth (1)
//!   +24  ComponentDataPackingMethod (2)
//!   +26  ComponentDataEncodingMethod (2)
//!   ...

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_dpx(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(2048);
    let Some(h) = head else { return false };
    if h.len() < 32 {
        return false;
    }
    let m0 = h[0];
    let m1 = h[1];
    let m2 = h[2];
    let m3 = h[3];
    let (is_dpx, le) = match (m0, m1, m2, m3) {
        (b'S', b'D', b'P', b'X') => (true, false),
        (b'X', b'P', b'D', b'S') => (true, true),
        (0x80, 0x2A, 0x5F, 0xD7) => (false, false),
        (0xD7, 0x5F, 0x2A, 0x80) => (false, true),
        _ => return false,
    };

    if !is_dpx {
        // Cineon path: provide a minimal stream entry; the file
        // information layout differs from DPX and we don't decode it.
        fa.stream_prepare(StreamKind::General);
        fa.set_field(StreamKind::General, 0, "Format", "Cineon");
        fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, 0, "Format", "Cineon");
        let endian_str = if le { "Little" } else { "Big" };
        fa.set_field(StreamKind::Image, 0, "Format_Settings_Endianness", endian_str);
        return true;
    }

    // DPX path — read fields at their SMPTE 268M offsets.
    if h.len() < 808 {
        return false;
    }
    let version_raw = String::from_utf8_lossy(&h[8..16]).trim_end_matches('\0').trim().to_string();
    let width = read_u32(h, 772, le);
    let height = read_u32(h, 776, le);
    let descriptor = h[800];
    let _transfer = h[801];
    let _colorimetric = h[802];
    let bit_depth = h[803];
    let packing_method = read_u16(h, 804, le);
    let encoding_method = read_u16(h, 806, le);

    let creator = if h.len() >= 260 {
        let s = String::from_utf8_lossy(&h[160..260]).trim_end_matches('\0').to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    };

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "DPX");
    fa.set_field(StreamKind::General, 0, "ImageCount", "1");
    let version_fmt = format_version(&version_raw);
    if !version_fmt.is_empty() {
        fa.set_field(StreamKind::General, 0, "Format_Version", version_fmt.clone());
    }
    if let Some(ref c) = creator {
        fa.set_field(StreamKind::General, 0, "Encoded_Library", c.clone());
    }

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", "DPX");
    if !version_fmt.is_empty() {
        fa.set_field(StreamKind::Image, 0, "Format_Version", version_fmt);
    }
    let endian_str = if le { "Little" } else { "Big" };
    fa.set_field(StreamKind::Image, 0, "Format_Settings_Endianness", endian_str);
    if let Some(pack) = dpx_packing(packing_method) {
        fa.set_field(StreamKind::Image, 0, "Format_Settings_Packing", pack);
    }
    if let Some(enc) = dpx_encoding(encoding_method) {
        fa.set_field(StreamKind::Image, 0, "Format_Compression", enc);
    }
    if width > 0 {
        fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    }
    if height > 0 {
        fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    }
    if width > 0 && height > 0 {
        fa.set_field(StreamKind::Image, 0, "PixelAspectRatio", "1.000");
        let dar = width as f64 / height as f64;
        fa.set_field(StreamKind::Image, 0, "DisplayAspectRatio", format!("{:.3}", dar));
    }
    let cs = dpx_descriptor_color_space(descriptor);
    if !cs.is_empty() {
        fa.set_field(StreamKind::Image, 0, "ColorSpace", cs);
    }
    let cm = dpx_descriptor_chroma(descriptor);
    if !cm.is_empty() {
        fa.set_field(StreamKind::Image, 0, "ChromaSubsampling", cm);
    }
    fa.set_field(StreamKind::Image, 0, "BitDepth", bit_depth.to_string());
    // DPX is uncompressed → lossless.
    fa.set_field(StreamKind::Image, 0, "Compression_Mode", "Lossless");
    // Image.StreamSize = file size (whole file is image data + header,
    // but oracle reports the file size as the image's StreamSize). General
    // StreamSize = 0 (no separately-tracked overhead in this model).
    let file_size = fa.remain();
    fa.set_field(StreamKind::Image, 0, "StreamSize", file_size.to_string());
    fa.force_field(StreamKind::General, 0, "StreamSize", "0");
    // Oracle marks colour_description_present whenever colorimetric/
    // transfer fields are present in the image element header (always
    // present in DPX, so always emit "Yes").
    fa.set_field(StreamKind::Image, 0, "colour_description_present", "Yes");
    if let Some(c) = creator {
        fa.set_field(StreamKind::Image, 0, "Encoded_Library", c);
    }
    true
}

fn format_version(raw: &str) -> String {
    // Oracle emits just the numeric version (strips the leading "V"
    // marker), e.g. "V1.0    " → "1.0".
    raw.trim().trim_start_matches('V').to_string()
}

fn read_u16(buf: &[u8], off: usize, le: bool) -> u16 {
    if off + 2 > buf.len() {
        return 0;
    }
    if le {
        u16::from_le_bytes([buf[off], buf[off + 1]])
    } else {
        u16::from_be_bytes([buf[off], buf[off + 1]])
    }
}
fn read_u32(buf: &[u8], off: usize, le: bool) -> u32 {
    if off + 4 > buf.len() {
        return 0;
    }
    if le {
        u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
    } else {
        u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
    }
}

fn dpx_descriptor_color_space(d: u8) -> &'static str {
    // SMPTE 268M-2003 Annex C
    match d {
        0 => "User-defined",
        1 => "Y",   // Red
        2 => "Y",   // Green
        3 => "Y",   // Blue
        4 => "A",   // Alpha
        6 => "Y",   // Luma
        7 => "YUV", // Cb
        8 => "YUV", // Cr
        9 => "YUV", // Depth
        50 => "RGB",
        51 => "RGBA",
        52 => "ABGR",
        100 => "YUV", // CbYCrY
        101 => "YUV", // CbYACrYA
        102 => "YUV",
        103 => "YUV",
        _ => "",
    }
}
fn dpx_descriptor_chroma(d: u8) -> &'static str {
    // RGB/RGBA/ABGR (50-52) have no chroma subsampling — oracle omits
    // the field. Only emit for YUV color formats.
    match d {
        100 => "4:2:2",
        101 => "4:2:2",
        102 => "4:4:4",
        103 => "4:4:4",
        _ => "",
    }
}
fn dpx_packing(p: u16) -> Option<&'static str> {
    match p {
        0 => Some("Packed"),
        1 => Some("Filled, method A"),
        2 => Some("Filled, method B"),
        _ => None,
    }
}
fn dpx_encoding(e: u16) -> Option<&'static str> {
    match e {
        0 => Some("Raw"),
        1 => Some("RLE"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_dpx(width: u32, height: u32, bit_depth: u8, descriptor: u8) -> Vec<u8> {
        let mut buf = vec![0u8; 2048];
        buf[..4].copy_from_slice(b"SDPX");
        // image offset = 2048
        buf[4..8].copy_from_slice(&2048u32.to_be_bytes());
        // version
        let ver = b"V2.0    ";
        buf[8..16].copy_from_slice(ver);
        // file size = 2048
        buf[16..20].copy_from_slice(&2048u32.to_be_bytes());
        // image info @ 768
        buf[772..776].copy_from_slice(&width.to_be_bytes());
        buf[776..780].copy_from_slice(&height.to_be_bytes());
        // image element 0 @ 780
        buf[800] = descriptor;
        buf[803] = bit_depth;
        buf
    }

    #[test]
    fn rejects_non_dpx() {
        let mut fa = FileAnalyze::new(b"NOT a DPX file at all..");
        assert!(!parse_dpx(&mut fa));
    }

    #[test]
    fn parses_minimal_big_endian_dpx() {
        let buf = build_minimal_dpx(2048, 1556, 10, 50); // 2K, 10-bit RGB
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dpx(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("DPX"));
        assert_eq!(i("Format_Version").as_deref(), Some("2.0"));
        assert_eq!(i("Width").as_deref(), Some("2048"));
        assert_eq!(i("Height").as_deref(), Some("1556"));
        assert_eq!(i("BitDepth").as_deref(), Some("10"));
        assert_eq!(i("ColorSpace").as_deref(), Some("RGB"));
        assert_eq!(i("Format_Settings_Endianness").as_deref(), Some("Big"));
    }
}
