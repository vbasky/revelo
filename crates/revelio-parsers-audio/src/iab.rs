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

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{int32u, int8u};

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

pub fn parse_iab(fa: &mut FileAnalyze) -> bool {
    // Need at least the fixed-size pieces of the preamble + IAFrame headers
    // plus 1 byte of Version. PreambleLength is read up front, so the peek
    // is split into "fixed prefix" (5 bytes) then "rest".
    let head = match fa.peek_raw(fa.Remain().min(5)) {
        Some(h) if h.len() == 5 => h,
        _ => return false,
    };
    if head[0] != 0x01 {
        return false;
    }
    let preamble_length =
        u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;

    // Full header: 5 (preamble header) + preamble_length + 5 (iaframe header)
    // + 1 (version). Use peek_raw with the Remain-clamped len per spec.
    let full_needed = 5usize
        .saturating_add(preamble_length)
        .saturating_add(5)
        .saturating_add(1);
    let peek_len = fa.Remain().min(full_needed);
    if peek_len < full_needed {
        return false;
    }
    let full = match fa.peek_raw(peek_len) {
        Some(f) => f,
        None => return false,
    };
    let iaframe_tag_off = 5 + preamble_length;
    if full[iaframe_tag_off] != 0x02 {
        return false;
    }
    let iaframe_length = u32::from_be_bytes([
        full[iaframe_tag_off + 1],
        full[iaframe_tag_off + 2],
        full[iaframe_tag_off + 3],
        full[iaframe_tag_off + 4],
    ]);
    // Sanity: IAFrameLength must cover at least the Version byte.
    if iaframe_length < 1 {
        return false;
    }

    let version = full[iaframe_tag_off + 5];

    fa.Element_Begin("IAB");
    let mut preamble_tag: int8u = 0;
    fa.Get_B1(&mut preamble_tag, "PreambleTag");
    let mut _preamble_len: int32u = 0;
    fa.Get_B4(&mut _preamble_len, "PreambleLength");
    fa.Skip_Hexa(preamble_length, "PreambleValue");
    let mut iaframe_tag: int8u = 0;
    fa.Get_B1(&mut iaframe_tag, "IAFrameTag");
    let mut _ia_len: int32u = 0;
    fa.Get_B4(&mut _ia_len, "IAFrameLength");

    let mut sample_rate: Option<u32> = None;
    let mut bit_depth: Option<u8> = None;
    let mut frame_rate: Option<f64> = None;

    if version == 1 {
        let mut _v: int8u = 0;
        fa.Get_B1(&mut _v, "Version");
        fa.BS_Begin();
        let mut sr_idx: int8u = 0;
        let mut bd_idx: int8u = 0;
        let mut fr_idx: int8u = 0;
        fa.Get_S1(2, &mut sr_idx, "SampleRate");
        fa.Get_S1(2, &mut bd_idx, "BitDepth");
        fa.Get_S1(4, &mut fr_idx, "FrameRate");
        fa.BS_End();

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
    fa.Element_End();

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "IAB", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "IAB", false);
    if let Some(sr) = sample_rate {
        fa.Fill(StreamKind::Audio, 0, "SamplingRate", sr.to_string(), false);
    }
    if let Some(bd) = bit_depth {
        fa.Fill(StreamKind::Audio, 0, "BitDepth", bd.to_string(), false);
    }
    if let Some(fr) = frame_rate {
        // Integer frame rates render without decimals; 23.976 keeps 3 digits
        // like MediaInfoLib's Ztring float formatting for FrameRate.
        let s = if (fr.fract()).abs() < 1e-6 {
            format!("{}", fr.round() as i64)
        } else {
            format!("{:.3}", fr)
        };
        fa.Fill(StreamKind::Audio, 0, "FrameRate", s, false);
    }

    true
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
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
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
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
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
