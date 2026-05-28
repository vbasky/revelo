//! IVF parser — the VPx test-bench container used by libvpx/AOM tools.
//!
//! Mirrors MediaInfoLib's `File_Ivf.cpp` (FileHeader_Begin / FileHeader_Parse).
//! The reference parser also peels off per-frame headers and forwards the
//! payload to a nested VP8/VP9/AV1 decoder, but here we only need the
//! container-level identification + Video stream metadata.
//!
//! Layout (all multi-byte fields little-endian):
//!   0x00  C4  "DKIF" signature
//!   0x04  L2  Version (only 0 is defined)
//!   0x06  L2  Header size (>= 32 for v0 with full payload)
//!   0x08  C4  Codec FourCC (e.g. "VP80", "VP90", "AV01")
//!   0x0C  L2  Width
//!   0x0E  L2  Height
//!   0x10  L4  Frame rate numerator
//!   0x14  L4  Frame rate denominator
//!   0x18  L4  Frame count
//!   0x1C  L4  Unused
//!   0x20  ... Per-frame records (frame_size L4 + PTS L8 + payload)

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{Int16u, Int32u};

const FOURCC_DKIF: Int32u = u32::from_be_bytes(*b"DKIF");

/// Parse IVF container (VP8/VP9/AV1 elementary streams).
///
/// Detection: `DKIF` magic.
/// Fills: Codec fourcc, frame dimensions, frame rate from timebase.
pub fn parse_ivf(fa: &mut FileAnalyze) -> bool {
    // Peek the signature so non-IVF buffers leave the cursor untouched
    // for sibling parsers to try.
    let head = match fa.peek_raw(fa.remain().min(4)) {
        Some(b) if b.len() == 4 => b,
        _ => return false,
    };
    let magic = Int32u::from_be_bytes([head[0], head[1], head[2], head[3]]);
    if magic != FOURCC_DKIF {
        return false;
    }

    // Need at least the 32-byte v0 header to extract anything useful.
    if fa.remain() < 32 {
        return false;
    }

    fa.element_begin("IVF");
    fa.skip_c4("Signature");
    let mut version: Int16u = 0;
    fa.get_l2(&mut version, "Version");

    let mut header_size: Int16u = 0;
    let mut fourcc: Int32u = 0;
    let mut width: Int16u = 0;
    let mut height: Int16u = 0;
    let mut frame_rate_num: Int32u = 0;
    let mut frame_rate_den: Int32u = 0;
    let mut frame_count: Int32u = 0;

    if version == 0 {
        fa.get_l2(&mut header_size, "Header Size");
        if header_size >= 32 {
            fa.get_c4(&mut fourcc, "Fourcc");
            fa.get_l2(&mut width, "Width");
            fa.get_l2(&mut height, "Height");
            fa.get_l4(&mut frame_rate_num, "FrameRate Numerator");
            fa.get_l4(&mut frame_rate_den, "FrameRate Denominator");
            fa.get_l4(&mut frame_count, "Frame Count");
            let mut _unused: Int32u = 0;
            fa.get_l4(&mut _unused, "Unused");
            let extra = header_size as usize - 32;
            if extra > 0 && fa.remain() >= extra {
                fa.skip_hexa(extra, "Unknown");
            }
        }
    }

    fa.element_end();

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "IVF", false);

    if version == 0 && header_size >= 32 {
        fa.stream_prepare(StreamKind::Video);
        let format = video_format_from_fourcc(fourcc);
        if !format.is_empty() {
            fa.fill(StreamKind::Video, 0, "Format", format, false);
        }
        let cc = fourcc.to_be_bytes();
        let codec_id = String::from_utf8_lossy(&cc).into_owned();
        fa.fill(StreamKind::Video, 0, "CodecID", codec_id, false);

        if width > 0 {
            fa.fill(StreamKind::Video, 0, "Width", width.to_string(), false);
        }
        if height > 0 {
            fa.fill(StreamKind::Video, 0, "Height", height.to_string(), false);
        }
        if frame_rate_den != 0 {
            let fr = frame_rate_num as f64 / frame_rate_den as f64;
            fa.fill(StreamKind::Video, 0, "FrameRate", format!("{:.3}", fr), false);
            fa.fill(
                StreamKind::Video,
                0,
                "FrameRate_Num",
                frame_rate_num.to_string(),
                false,
            );
            fa.fill(
                StreamKind::Video,
                0,
                "FrameRate_Den",
                frame_rate_den.to_string(),
                false,
            );
        }
        if frame_count > 0 {
            fa.fill(StreamKind::Video, 0, "FrameCount", frame_count.to_string(), false);
        }

        fa.fill(StreamKind::General, 0, "VideoCount", "1", false);
    }

    true
}

fn video_format_from_fourcc(fcc: Int32u) -> &'static str {
    // FourCCs IVF files use in the wild — kept aligned with the codecs
    // MediaInfoLib's File_Ivf.cpp dispatches to (AV1/AV2/VP8/VP9).
    match &fcc.to_be_bytes() {
        b"VP80" | b"vp80" => "VP8",
        b"VP90" | b"vp90" => "VP9",
        b"AV01" | b"av01" => "AV1",
        b"AV02" | b"av02" => "AV2",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ivf(fourcc: &[u8; 4], width: u16, height: u16, fr_num: u32, fr_den: u32, frame_count: u32) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32);
        buf.extend_from_slice(b"DKIF");
        buf.extend_from_slice(&0u16.to_le_bytes()); // version
        buf.extend_from_slice(&32u16.to_le_bytes()); // header size
        buf.extend_from_slice(fourcc);
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&height.to_le_bytes());
        buf.extend_from_slice(&fr_num.to_le_bytes());
        buf.extend_from_slice(&fr_den.to_le_bytes());
        buf.extend_from_slice(&frame_count.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // unused
        buf
    }

    #[test]
    fn parses_vp9_header() {
        let buf = make_ivf(b"VP90", 1920, 1080, 30, 1, 300);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ivf(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("IVF"));
        assert_eq!(g("VideoCount").as_deref(), Some("1"));
        assert_eq!(v("Format").as_deref(), Some("VP9"));
        assert_eq!(v("CodecID").as_deref(), Some("VP90"));
        assert_eq!(v("Width").as_deref(), Some("1920"));
        assert_eq!(v("Height").as_deref(), Some("1080"));
        assert_eq!(v("FrameRate").as_deref(), Some("30.000"));
        assert_eq!(v("FrameCount").as_deref(), Some("300"));
    }

    #[test]
    fn parses_av1_header_with_fractional_rate() {
        // 30000/1001 = 29.97 — the NTSC-style framerate IVF tools emit.
        let buf = make_ivf(b"AV01", 640, 480, 30000, 1001, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ivf(&mut fa));
        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(v("Format").as_deref(), Some("AV1"));
        assert_eq!(v("FrameRate").as_deref(), Some("29.970"));
        assert_eq!(v("FrameRate_Num").as_deref(), Some("30000"));
        assert_eq!(v("FrameRate_Den").as_deref(), Some("1001"));
    }

    #[test]
    fn non_dkif_buffer_returns_false() {
        let buf = b"RIFF\x00\x00\x00\x00WAVE";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_ivf(&mut fa));
    }
}
