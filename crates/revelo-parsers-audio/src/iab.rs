//! IAB (Immersive Audio Bitstream) parser — frame-wrapped sync.
//!
//! Mirrors the IA-Frame preamble validation from `File_Iab.cpp::Header_Parse`.
//! IAB has no file-level magic; detection is the per-frame header:
//!   1 byte  PreambleTag (must be 0x01)
//!   4 bytes PreambleLength (big-endian)
//!   N bytes PreambleValue
//!   1 byte  IAFrameTag (must be 0x02)
//!   4 bytes IAFrameLength (big-endian)
//!   IAFrame payload (Version, then 8 packed bits of SampleRate/BitDepth/FrameRate).
//!
//! Only Version==1 carries SampleRate/BitDepth/FrameRate; other versions
//! still get Format=IAB but leave those audio fields blank, matching the
//! C++ which simply skips the unknown payload.

use revelo_core::{FileAnalyze, Reader, StreamKind};

const IAB_SAMPLE_RATE: [u32; 4] = [48000, 96000, 0, 0];
const IAB_BIT_DEPTH: [u8; 4] = [16, 24, 0, 0];

fn iab_frame_rate(code: u8) -> Option<f64> {
    match code {
        0 => Some(24.0),
        1 => Some(25.0),
        2 => Some(30.0),
        3 => Some(48.0),
        4 => Some(50.0),
        5 => Some(60.0),
        6 => Some(96.0),
        7 => Some(100.0),
        8 => Some(120.0),
        9 => Some(24000.0 / 1001.0),
        _ => None,
    }
}

/// Parse Immersive Audio Bitstream (ATSC 3.0).
///
/// Detection: iab_frame sync.
/// Fills: Format, frame config.
pub fn parse_iab(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    // Validate the preamble + IAFrame headers by peeking, then consume.
    let head = r.peek_raw(5)?;
    if head.len() < 5 || head[0] != 0x01 {
        return None;
    }
    let preamble_length = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;

    let full_needed = 5usize.saturating_add(preamble_length).saturating_add(5).saturating_add(1);
    let full = r.peek_raw(full_needed)?;
    if full.len() < full_needed {
        return None;
    }
    let iaframe_tag_off = 5 + preamble_length;
    if full[iaframe_tag_off] != 0x02 {
        return None;
    }
    let iaframe_length = u32::from_be_bytes([
        full[iaframe_tag_off + 1],
        full[iaframe_tag_off + 2],
        full[iaframe_tag_off + 3],
        full[iaframe_tag_off + 4],
    ]);
    // Sanity: IAFrameLength must cover at least the Version byte.
    if iaframe_length < 1 {
        return None;
    }

    let version = full[iaframe_tag_off + 5];

    r.element_begin("IAB");
    r.be_u8("PreambleTag")?;
    r.be_u32("PreambleLength")?;
    r.skip(preamble_length)?; // PreambleValue
    r.be_u8("IAFrameTag")?;
    r.be_u32("IAFrameLength")?;

    let mut sample_rate: Option<u32> = None;
    let mut bit_depth: Option<u8> = None;
    let mut frame_rate: Option<f64> = None;

    if version == 1 {
        r.be_u8("Version")?;
        let (sr_idx, bd_idx, fr_idx) = r.bits(|b| {
            let sr_idx = b.read::<u8>(2, "SampleRate")?;
            let bd_idx = b.read::<u8>(2, "BitDepth")?;
            let fr_idx = b.read::<u8>(4, "FrameRate")?;
            Some((sr_idx, bd_idx, fr_idx))
        })?;

        let sr = IAB_SAMPLE_RATE[(sr_idx & 0x3) as usize];
        if sr != 0 {
            sample_rate = Some(sr);
        }
        let bd = IAB_BIT_DEPTH[(bd_idx & 0x3) as usize];
        if bd != 0 {
            bit_depth = Some(bd);
        }
        if let Some(fr) = iab_frame_rate(fr_idx & 0x0F) {
            frame_rate = Some(fr);
        }
    }
    r.element_end();

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "IAB");
    r.set_field(StreamKind::General, 0, "AudioCount", "1");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "IAB");
    if let Some(sr) = sample_rate {
        r.set_field(StreamKind::Audio, 0, "SamplingRate", sr.to_string());
    }
    if let Some(bd) = bit_depth {
        r.set_field(StreamKind::Audio, 0, "BitDepth", bd.to_string());
    }
    if let Some(fr) = frame_rate {
        // Integer frame rates render without decimals; 23.976 keeps 3 digits
        // like MediaInfoLib's Ztring float formatting for FrameRate.
        let s = if (fr.fract()).abs() < 1e-6 {
            format!("{}", fr.round() as i64)
        } else {
            format!("{:.3}", fr)
        };
        r.set_field(StreamKind::Audio, 0, "FrameRate", s);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_iab_frame(
        preamble_value: &[u8],
        version: u8,
        sr_idx: u8,
        bd_idx: u8,
        fr_idx: u8,
        payload_extra: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0x01);
        buf.extend_from_slice(&(preamble_value.len() as u32).to_be_bytes());
        buf.extend_from_slice(preamble_value);
        buf.push(0x02);
        // IAFrameLength: Version(1) + packed(1) + extra
        let ia_len = (1 + 1 + payload_extra) as u32;
        buf.extend_from_slice(&ia_len.to_be_bytes());
        buf.push(version);
        if version == 1 {
            let packed: u8 = ((sr_idx & 0x3) << 6) | ((bd_idx & 0x3) << 4) | (fr_idx & 0x0F);
            buf.push(packed);
        } else {
            buf.push(0);
        }
        buf.resize(buf.len() + payload_extra, 0);
        buf
    }

    #[test]
    fn parses_version1_48k_24bit_24fps() {
        let buf = build_iab_frame(&[0xAA, 0xBB, 0xCC], 1, 0, 1, 0, 32);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iab(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("IAB"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("IAB"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("FrameRate").as_deref(), Some("24"));
    }

    #[test]
    fn parses_version1_96k_23_976fps() {
        // sr_idx=1 → 96000, bd_idx=0 → 16, fr_idx=9 → 24000/1001.
        let buf = build_iab_frame(&[], 1, 1, 0, 9, 8);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iab(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("96000"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("FrameRate").as_deref(), Some("23.976"));
    }

    #[test]
    fn rejects_bad_preamble_tag() {
        // Wrong preamble tag (0x00 instead of 0x01).
        let mut buf = build_iab_frame(&[0x11], 1, 0, 0, 0, 4);
        buf[0] = 0x00;
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_iab(&mut fa));

        // Short buffer.
        let mut fa2 = FileAnalyze::new(&[0x01, 0x00, 0x00]);
        assert!(!parse_iab(&mut fa2));

        // Wrong IAFrameTag (corrupt byte at offset 5+preamble_length).
        let mut buf3 = build_iab_frame(&[0x22, 0x33], 1, 0, 0, 0, 4);
        // preamble_length=2 → iaframe_tag at offset 5+2=7.
        buf3[7] = 0x99;
        let mut fa3 = FileAnalyze::new(&buf3);
        assert!(!parse_iab(&mut fa3));
    }
}
