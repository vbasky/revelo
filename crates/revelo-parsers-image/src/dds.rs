//! DDS (DirectDraw Surface) parser.
//!
//! Fixed-layout header (124+ bytes, little-endian):
//!   0..4    Magic "DDS "
//!   4..8    Size (must be ≥ 124)
//!   8..12   Flags
//!   12..16  Height
//!   16..20  Width
//!   20..24  PitchOrLinearSize
//!   24..28  Depth
//!   28..32  MipMapCount
//!   32..76  Reserved1 (11 × 4 bytes)
//!   76..108 Pixel format:
//!     76..80  pf_Size (typically 32)
//!     80..84  pfFlags
//!     84..88  FourCC
//!     88..92  RGBBitCount
//!     92..108 R/G/B/A bit masks
//!   108..128 Caps1..4 + Reserved2
//!
//! Notable flag bits:
//!   Flags & 0x000002 = DDSD_HEIGHT
//!   Flags & 0x000004 = DDSD_WIDTH
//!   Flags & 0x800000 = DDSD_DEPTH
//!   pfFlags & 0x4 = DDPF_FOURCC (compressed; FourCC names the codec)

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_dds(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(128);
    let Some(h) = head else { return false };
    if &h[0..4] != b"DDS " {
        return false;
    }
    let size = u32::from_le_bytes([h[4], h[5], h[6], h[7]]);
    if size < 124 {
        return false;
    }
    let flags = u32::from_le_bytes([h[8], h[9], h[10], h[11]]);
    let height = u32::from_le_bytes([h[12], h[13], h[14], h[15]]);
    let width = u32::from_le_bytes([h[16], h[17], h[18], h[19]]);
    let depth = u32::from_le_bytes([h[24], h[25], h[26], h[27]]);
    let pf_size = u32::from_le_bytes([h[76], h[77], h[78], h[79]]);
    let (pf_flags, fourcc) = if pf_size >= 32 {
        let pf = u32::from_le_bytes([h[80], h[81], h[82], h[83]]);
        let fcc = [h[84], h[85], h[86], h[87]];
        (pf, fcc)
    } else {
        (0u32, [0u8; 4])
    };

    let file_size = fa.remain();

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "DDS");
    fa.set_field(StreamKind::General, 0, "ImageCount", "1");

    fa.stream_prepare(StreamKind::Image);
    if (pf_flags & 0x4) != 0 {
        // FourCC-compressed: oracle uses the RIFF codec map which
        // resolves DXT1/DXT3/DXT5/DX10/etc to "DirectX TC".
        fa.set_field(StreamKind::Image, 0, "Format", "DirectX TC");
        let fcc_str: String = fourcc.iter().map(|&b| b as char).collect();
        fa.set_field(StreamKind::Image, 0, "CodecID", fcc_str);
    } else {
        fa.set_field(StreamKind::Image, 0, "Format", "DDS");
    }
    if (flags & 0x4) != 0 {
        fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    }
    if (flags & 0x2) != 0 {
        fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    }
    if (flags & 0x800000) != 0 {
        fa.set_field(StreamKind::Image, 0, "BitDepth", depth.to_string());
    }
    fa.set_field(StreamKind::Image, 0, "StreamSize", file_size.to_string());
    fa.force_field(StreamKind::General, 0, "StreamSize", "0");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_dds(width: u32, height: u32, fourcc: &[u8; 4], with_fourcc: bool) -> Vec<u8> {
        let mut buf = vec![0u8; 128];
        buf[..4].copy_from_slice(b"DDS ");
        buf[4..8].copy_from_slice(&124u32.to_le_bytes()); // Size
        let flags: u32 = 0x1 | 0x2 | 0x4 | 0x1000; // CAPS|HEIGHT|WIDTH|PIXELFORMAT
        buf[8..12].copy_from_slice(&flags.to_le_bytes());
        buf[12..16].copy_from_slice(&height.to_le_bytes());
        buf[16..20].copy_from_slice(&width.to_le_bytes());
        // Pixel format @ 76
        buf[76..80].copy_from_slice(&32u32.to_le_bytes());
        let pf_flags: u32 = if with_fourcc { 0x4 } else { 0 };
        buf[80..84].copy_from_slice(&pf_flags.to_le_bytes());
        buf[84..88].copy_from_slice(fourcc);
        buf
    }

    #[test]
    fn rejects_non_dds() {
        let mut fa = FileAnalyze::new(b"NOT DDS .................");
        assert!(!parse_dds(&mut fa));
    }

    #[test]
    fn parses_dxt1_dds() {
        let buf = build_dds(256, 256, b"DXT1", true);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dds(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("DirectX TC"));
        assert_eq!(i("Width").as_deref(), Some("256"));
        assert_eq!(i("Height").as_deref(), Some("256"));
        assert_eq!(i("CodecID").as_deref(), Some("DXT1"));
    }

    #[test]
    fn uncompressed_dds_omits_codec_id() {
        let buf = build_dds(64, 64, b"\0\0\0\0", false);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dds(&mut fa));
        assert!(fa.retrieve(StreamKind::Image, 0, "CodecID").is_none());
    }
}
