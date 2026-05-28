//! WebP image parser.
//!
//! WebP is a RIFF container with FORM type "WEBP". Three frame chunks:
//!   "VP8 " — lossy VP8 keyframe
//!   "VP8L" — lossless variant
//!   "VP8X" — extended: canvas dimensions + feature flags, with
//!            ALPH / ANIM / EXIF / ICCP / XMP children
//!
//! We extract Format, Width, Height, ColorSpace, Compression_Mode from
//! whichever payload chunk is present.

use revelio_core::{FileAnalyze, StreamKind};

const FOURCC_RIFF: u32 = u32::from_be_bytes(*b"RIFF");
const FOURCC_WEBP: u32 = u32::from_be_bytes(*b"WEBP");
const FOURCC_VP8: u32 = u32::from_be_bytes(*b"VP8 ");
const FOURCC_VP8L: u32 = u32::from_be_bytes(*b"VP8L");
const FOURCC_VP8X: u32 = u32::from_be_bytes(*b"VP8X");
const FOURCC_ALPH: u32 = u32::from_be_bytes(*b"ALPH");
const FOURCC_ANIM: u32 = u32::from_be_bytes(*b"ANIM");

#[derive(Default)]
struct WebpInfo {
    width: u32,
    height: u32,
    format: &'static str,
    compression: &'static str,
    color_space: &'static str,
    has_alpha: bool,
    is_animated: bool,
    version: Option<u8>,
}

/// Parse WebP image.
///
/// Detection: RIFF + `WEBP` + VP8/VP8L/VP8X chunks.
/// Fills: Dimensions, lossy/lossless, alpha, animation, ICC/XMP/EXIF.
pub fn parse_webp(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(12);
    let Some(h) = head else { return false };
    let magic = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    let form = u32::from_be_bytes([h[8], h[9], h[10], h[11]]);
    if magic != FOURCC_RIFF || form != FOURCC_WEBP {
        return false;
    }
    let total = fa.remain();
    let body = match fa.peek_raw(total) {
        Some(b) => b,
        None => return false,
    };

    let mut info = WebpInfo::default();
    walk_chunks(&body[12..], total - 12, &mut info);

    if info.format.is_empty() {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "WebP", false);
    fa.fill(StreamKind::General, 0, "ImageCount", "1", false);

    fa.stream_prepare(StreamKind::Image);
    fa.fill(StreamKind::Image, 0, "Format", info.format, false);
    if let Some(v) = info.version {
        fa.fill(StreamKind::Image, 0, "Format_Version", format!("Version {}", v), false);
    }
    if info.width > 0 {
        fa.fill(StreamKind::Image, 0, "Width", info.width.to_string(), false);
    }
    if info.height > 0 {
        fa.fill(StreamKind::Image, 0, "Height", info.height.to_string(), false);
    }
    fa.fill(StreamKind::Image, 0, "BitDepth", "8", false);
    if !info.compression.is_empty() {
        fa.fill(StreamKind::Image, 0, "Compression_Mode", info.compression, false);
    }
    let mut color_space = info.color_space.to_string();
    if info.has_alpha && !color_space.ends_with('A') {
        color_space.push('A');
    }
    if !color_space.is_empty() {
        fa.fill(StreamKind::Image, 0, "ColorSpace", color_space, false);
    }
    let _ = info.is_animated;
    true
}

fn walk_chunks(buf: &[u8], len: usize, info: &mut WebpInfo) {
    let mut i = 0;
    while i + 8 <= len {
        let fcc = u32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        let size = u32::from_le_bytes([buf[i + 4], buf[i + 5], buf[i + 6], buf[i + 7]]) as usize;
        let data_start = i + 8;
        let data_end = data_start + size;
        if data_end > len {
            break;
        }
        let payload = &buf[data_start..data_end];
        match fcc {
            FOURCC_VP8 => parse_vp8(payload, info),
            FOURCC_VP8L => parse_vp8l(payload, info),
            FOURCC_VP8X => parse_vp8x(payload, info),
            FOURCC_ALPH => info.has_alpha = true,
            FOURCC_ANIM => info.is_animated = true,
            _ => {}
        }
        i = data_end + (size & 1);
    }
}

/// VP8 keyframe header (RFC 6386):
///   frame_tag (3 bytes LE): bit 0 = key/inter (0=key), bits 1-3 = version,
///     bit 4 = show_frame, bits 5-23 = first_part_size
///   start_code (3 bytes): 0x9D 0x01 0x2A
///   width-1 (2 bytes LE, low 14 bits)
///   height-1 (2 bytes LE, low 14 bits)
fn parse_vp8(p: &[u8], info: &mut WebpInfo) {
    // Oracle uses Format="VP8" for lossy WebP (matches the no-VP8-parser
    // fallback in File_WebP.cpp:WEBP_VP8_); VP8L uses Format="WebP".
    info.format = "VP8";
    info.compression = "Lossy";
    if info.color_space.is_empty() {
        info.color_space = "YUV";
    }
    if p.len() < 10 {
        return;
    }
    let tag = (p[0] as u32) | ((p[1] as u32) << 8) | ((p[2] as u32) << 16);
    let is_keyframe = (tag & 1) == 0;
    if !is_keyframe {
        return;
    }
    if p[3] != 0x9D || p[4] != 0x01 || p[5] != 0x2A {
        return;
    }
    let w_raw = (p[6] as u16) | ((p[7] as u16) << 8);
    let h_raw = (p[8] as u16) | ((p[9] as u16) << 8);
    if info.width == 0 {
        info.width = (w_raw & 0x3FFF) as u32;
    }
    if info.height == 0 {
        info.height = (h_raw & 0x3FFF) as u32;
    }
}

/// VP8L (lossless): signature 0x2F, then little-endian bitstream:
///   14 bits: width-1
///   14 bits: height-1
///    1 bit:  alpha_is_used
///    3 bits: version_number
fn parse_vp8l(p: &[u8], info: &mut WebpInfo) {
    if p.len() < 5 {
        return;
    }
    if p[0] != 0x2F {
        return;
    }
    info.format = "WebP";
    info.compression = "Lossless";
    // LE bitstream packed across bytes 1-4. Read 32 LE bits then unpack.
    let bits = (p[1] as u32)
        | ((p[2] as u32) << 8)
        | ((p[3] as u32) << 16)
        | ((p[4] as u32) << 24);
    let width_m1 = bits & 0x3FFF;
    let height_m1 = (bits >> 14) & 0x3FFF;
    let alpha_used = ((bits >> 28) & 1) != 0;
    let version = ((bits >> 29) & 0x7) as u8;
    info.width = width_m1 + 1;
    info.height = height_m1 + 1;
    info.has_alpha = info.has_alpha || alpha_used;
    info.version = Some(version);
    info.color_space = if alpha_used { "RGBA" } else { "RGB" };
}

/// VP8X: feature flags + canvas dimensions. Used when the file has
/// extended features (alpha, animation, EXIF, ICCP, XMP).
fn parse_vp8x(p: &[u8], info: &mut WebpInfo) {
    if p.len() < 10 {
        return;
    }
    let flags = p[0];
    let has_alpha = (flags & 0x10) != 0;
    let has_anim = (flags & 0x02) != 0;
    info.has_alpha = info.has_alpha || has_alpha;
    info.is_animated = info.is_animated || has_anim;
    // 3-byte LE values, both -1
    let canvas_w = (p[4] as u32) | ((p[5] as u32) << 8) | ((p[6] as u32) << 16);
    let canvas_h = (p[7] as u32) | ((p[8] as u32) << 8) | ((p[9] as u32) << 16);
    if info.width == 0 {
        info.width = canvas_w + 1;
    }
    if info.height == 0 {
        info.height = canvas_h + 1;
    }
    if info.format.is_empty() {
        info.format = "WebP";
    }
    if info.color_space.is_empty() {
        info.color_space = if has_alpha { "RGBA" } else { "RGB" };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_webp() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00AVI ");
        assert!(!parse_webp(&mut fa));
    }

    fn build_riff(form: &[u8; 4], chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body: Vec<u8> = form.to_vec();
        for (fcc, data) in chunks {
            body.extend_from_slice(*fcc);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() & 1 == 1 { body.push(0); }
        }
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
        buf.extend_from_slice(&body);
        buf
    }

    #[test]
    fn parses_lossless_vp8l_320x240() {
        // VP8L: 0x2F + 14 bits width-1 + 14 bits height-1 + 1 bit alpha + 3 bits version
        // width=320 → 319, height=240 → 239, alpha=1, version=0
        let bits: u32 = (320 - 1) | ((240u32 - 1) << 14) | (1 << 28) | (0 << 29);
        let mut payload = vec![0x2F];
        payload.extend_from_slice(&bits.to_le_bytes());
        let buf = build_riff(b"WEBP", &[(b"VP8L", payload)]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_webp(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("WebP"));
        assert_eq!(i("Width").as_deref(), Some("320"));
        assert_eq!(i("Height").as_deref(), Some("240"));
        assert_eq!(i("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(i("ColorSpace").as_deref(), Some("RGBA"));
    }

    #[test]
    fn parses_lossy_vp8_keyframe() {
        let mut payload = vec![0x10, 0x00, 0x00, 0x9D, 0x01, 0x2A];
        let w: u16 = 100; let h: u16 = 200;
        payload.extend_from_slice(&w.to_le_bytes());
        payload.extend_from_slice(&h.to_le_bytes());
        let buf = build_riff(b"WEBP", &[(b"VP8 ", payload)]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_webp(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Compression_Mode").as_deref(), Some("Lossy"));
        assert_eq!(i("Width").as_deref(), Some("100"));
        assert_eq!(i("Height").as_deref(), Some("200"));
        assert_eq!(i("ColorSpace").as_deref(), Some("YUV"));
    }
}
