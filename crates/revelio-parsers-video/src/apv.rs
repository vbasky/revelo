//! APV (Advanced Professional Video) elementary-stream parser.
//!
//! APV is a professional, intra-only video codec. A raw access unit begins
//! with `au_size` (u32 BE) followed by the signature `aPv1`, then one or more
//! PBUs (Packed Bitstream Units). We walk to the first frame (or
//! access-unit-information) PBU and read its `frame_info` to recover
//! profile/level, dimensions, chroma format, bit depth, and colour.
//!
//! Mirrors MediaInfoLib's `File_Apv.cpp` (in-band `frame_info`: 24-bit
//! `frame_width`/`frame_height`).
//!
//! Layout walked (big-endian):
//!   0x00  B4  au_size
//!   0x04  C4  signature ("aPv1")
//!   PBU:  B4  pbu_size  +  B1 pbu_type, B2 group_id, B1 reserved
//!   frame PBU → frame_header → frame_info:
//!     B1 profile_idc, B1 level_idc, B1 [band_idc:3 | reserved:5],
//!     B3 frame_width, B3 frame_height,
//!     B1 [chroma_format_idc:4 | bit_depth_minus8:4],
//!     B1 capture_time_distance, B1 reserved
//!   then B1 reserved, then 1-bit color_description_present_flag (+ payload)

use revelio_core::{FileAnalyze, Reader, StreamKind};

#[derive(Default)]
struct FrameInfo {
    profile_idc: u8,
    level_idc: u8,
    band_idc: u8,
    frame_width: u32,
    frame_height: u32,
    chroma_format_idc: u8,
    bit_depth_minus8: u8,
    color_description_present_flag: bool,
    color_primaries: u8,
    transfer_characteristics: u8,
    matrix_coefficients: u8,
    full_range_flag: bool,
}

/// Parse APV (Advanced Professional Video) elementary stream.
///
/// Detection: `aPv1` signature at byte offset 4 (after the `au_size` field).
/// Fills: profile\@level, dimensions, chroma, bit depth, colour description.
pub fn parse_apv(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let fi = {
        let r = &mut Reader::wrap(fa);
        let head = r.peek_raw(8)?;
        // Signature "aPv1" sits at offset 4; offset 0..4 is au_size.
        if head[4..8] != *b"aPv1" {
            return None;
        }
        r.be_u32("au_size")?;
        r.fourcc("signature")?;

        // Walk PBUs until the first frame or access-unit-information unit.
        let mut found: Option<FrameInfo> = None;
        for _ in 0..8 {
            if r.remain() < 8 {
                break;
            }
            let pbu_size = r.be_u32("pbu_size")?;
            let pbu_type = r.be_u8("pbu_type")?;
            r.be_u16("group_id")?;
            r.be_u8("reserved")?;
            match pbu_type {
                1 | 2 | 25 | 26 | 27 => {
                    found = Some(parse_frame_header(r)?);
                    break;
                }
                65 => {
                    // access_unit_information: num_frames, then per-frame
                    // pbu_type(1) + group_id(2) + reserved(1) + frame_info.
                    let num_frames = r.be_u16("num_frames")?;
                    if num_frames > 0 {
                        r.be_u8("pbu_type")?;
                        r.be_u16("group_id")?;
                        r.be_u8("reserved")?;
                        found = Some(parse_frame_info(r)?);
                    }
                    break;
                }
                _ => {
                    // pbu_size counts the 4-byte pbu_header we already read.
                    r.skip((pbu_size as usize).saturating_sub(4))?;
                }
            }
        }
        found?
    };
    fill_streams(fa, &fi);
    Some(())
}

fn parse_frame_info(r: &mut Reader<'_, '_>) -> Option<FrameInfo> {
    let profile_idc = r.be_u8("profile_idc")?;
    let level_idc = r.be_u8("level_idc")?;
    let band_byte = r.be_u8("band_idc")?;
    let frame_width = r.be_u24("frame_width")?;
    let frame_height = r.be_u24("frame_height")?;
    let cbyte = r.be_u8("chroma_format_idc / bit_depth_minus8")?;
    r.be_u8("capture_time_distance")?;
    r.be_u8("reserved_zero_8bits")?;
    Some(FrameInfo {
        profile_idc,
        level_idc,
        band_idc: (band_byte >> 5) & 0x07,
        frame_width,
        frame_height,
        chroma_format_idc: cbyte >> 4,
        bit_depth_minus8: cbyte & 0x0F,
        ..Default::default()
    })
}

fn parse_frame_header(r: &mut Reader<'_, '_>) -> Option<FrameInfo> {
    let mut fi = parse_frame_info(r)?;
    r.be_u8("reserved_zero_8bits")?;
    let cd = r.bits(|b| {
        let present = b.read::<u8>(1, "color_description_present_flag")? != 0;
        if present {
            let cp = b.read::<u8>(8, "color_primaries")?;
            let tc = b.read::<u8>(8, "transfer_characteristics")?;
            let mc = b.read::<u8>(8, "matrix_coefficients")?;
            let fr = b.read::<u8>(1, "full_range_flag")? != 0;
            Some(Some((cp, tc, mc, fr)))
        } else {
            Some(None)
        }
    })?;
    if let Some((cp, tc, mc, fr)) = cd {
        fi.color_description_present_flag = true;
        fi.color_primaries = cp;
        fi.transfer_characteristics = tc;
        fi.matrix_coefficients = mc;
        fi.full_range_flag = fr;
    }
    Some(fi)
}

fn fill_streams(fa: &mut FileAnalyze, fi: &FrameInfo) {
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "APV");

    fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, 0, "Format", "APV");
    let level = format!("{:.1}", fi.level_idc as f32 / 30.0);
    fa.set_field(
        StreamKind::Video,
        0,
        "Format_Profile",
        format!("{}@{}", apv_profile(fi.profile_idc), level),
    );
    fa.set_field(StreamKind::Video, 0, "band_idc", fi.band_idc.to_string());
    fa.set_field(StreamKind::Video, 0, "Width", fi.frame_width.to_string());
    fa.set_field(StreamKind::Video, 0, "Height", fi.frame_height.to_string());
    fa.set_field(StreamKind::Video, 0, "ColorSpace", apv_colorspace(fi.chroma_format_idc));
    fa.set_field(StreamKind::Video, 0, "ChromaSubsampling", apv_chroma(fi.chroma_format_idc));
    fa.set_field(StreamKind::Video, 0, "BitDepth", (fi.bit_depth_minus8 as u16 + 8).to_string());

    if fi.color_description_present_flag {
        if let Some(s) = cicp_primaries(fi.color_primaries) {
            fa.set_field(StreamKind::Video, 0, "colour_primaries", s);
        }
        if let Some(s) = cicp_transfer(fi.transfer_characteristics) {
            fa.set_field(StreamKind::Video, 0, "transfer_characteristics", s);
        }
        if let Some(s) = cicp_matrix(fi.matrix_coefficients) {
            fa.set_field(StreamKind::Video, 0, "matrix_coefficients", s);
        }
        fa.set_field(
            StreamKind::Video,
            0,
            "colour_range",
            if fi.full_range_flag { "Full" } else { "Limited" },
        );
    }
}

fn apv_profile(profile_idc: u8) -> String {
    match profile_idc {
        33 => "422-10".to_owned(),
        44 => "422-12".to_owned(),
        55 => "444-10".to_owned(),
        66 => "444-12".to_owned(),
        77 => "4444-10".to_owned(),
        88 => "4444-12".to_owned(),
        99 => "400-10".to_owned(),
        other => other.to_string(),
    }
}

fn apv_colorspace(chroma_format_idc: u8) -> &'static str {
    match chroma_format_idc {
        0 | 2 | 3 => "YUV",
        4 => "YUVA",
        _ => "",
    }
}

fn apv_chroma(chroma_format_idc: u8) -> &'static str {
    match chroma_format_idc {
        0 => "4:0:0",
        2 => "4:2:2",
        3 | 4 => "4:4:4",
        _ => "",
    }
}

fn cicp_primaries(idc: u8) -> Option<&'static str> {
    match idc {
        1 => Some("BT.709"),
        4 => Some("BT.470 System M"),
        5 => Some("BT.601 PAL"),
        6 => Some("BT.601 NTSC"),
        7 => Some("SMPTE 240M"),
        8 => Some("Generic film"),
        9 => Some("BT.2020"),
        10 => Some("XYZ"),
        11 => Some("DCI P3"),
        12 => Some("Display P3"),
        _ => None,
    }
}

fn cicp_transfer(idc: u8) -> Option<&'static str> {
    match idc {
        1 => Some("BT.709"),
        4 => Some("BT.470 System M"),
        5 => Some("BT.470 System B/G"),
        6 => Some("BT.601"),
        7 => Some("SMPTE 240M"),
        8 => Some("Linear"),
        11 => Some("IEC 61966-2-4"),
        12 => Some("BT.1361"),
        13 => Some("IEC 61966-2-1"),
        14 => Some("BT.2020 (10-bit)"),
        15 => Some("BT.2020 (12-bit)"),
        16 => Some("PQ"),
        17 => Some("SMPTE 428M"),
        18 => Some("HLG"),
        _ => None,
    }
}

fn cicp_matrix(idc: u8) -> Option<&'static str> {
    match idc {
        0 => Some("Identity"),
        1 => Some("BT.709"),
        4 => Some("FCC 73.682"),
        5 => Some("BT.470 System B/G"),
        6 => Some("BT.601"),
        7 => Some("SMPTE 240M"),
        8 => Some("YCgCo"),
        9 => Some("BT.2020 non-constant"),
        10 => Some("BT.2020 constant"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal raw APV access unit with a single primary-frame PBU
    /// carrying the given frame_info (no colour description).
    fn make_apv(
        profile_idc: u8,
        level_idc: u8,
        band_idc: u8,
        width: u32,
        height: u32,
        chroma_format_idc: u8,
        bit_depth_minus8: u8,
    ) -> Vec<u8> {
        let mut frame = Vec::new();
        frame.push(profile_idc);
        frame.push(level_idc);
        frame.push((band_idc & 0x07) << 5);
        frame.extend_from_slice(&width.to_be_bytes()[1..4]); // 24-bit
        frame.extend_from_slice(&height.to_be_bytes()[1..4]); // 24-bit
        frame.push((chroma_format_idc << 4) | (bit_depth_minus8 & 0x0F));
        frame.push(0); // capture_time_distance
        frame.push(0); // reserved_zero_8bits (frame_info)
        frame.push(0); // reserved_zero_8bits (frame_header)
        frame.push(0); // color_description_present_flag = 0 + 7 alignment bits

        let mut pbu = Vec::new();
        pbu.push(1u8); // pbu_type = primary frame
        pbu.extend_from_slice(&0u16.to_be_bytes()); // group_id
        pbu.push(0); // reserved
        pbu.extend_from_slice(&frame);
        let pbu_size = pbu.len() as u32; // pbu_header + payload

        let mut au = Vec::new();
        au.extend_from_slice(b"aPv1");
        au.extend_from_slice(&pbu_size.to_be_bytes());
        au.extend_from_slice(&pbu);

        let mut buf = Vec::new();
        buf.extend_from_slice(&(au.len() as u32).to_be_bytes()); // au_size
        buf.extend_from_slice(&au);
        buf
    }

    #[test]
    fn parses_apv_444_10bit() {
        let buf = make_apv(55, 90, 0, 1920, 1080, 3, 2);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_apv(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("APV"));
        assert_eq!(v("Format").as_deref(), Some("APV"));
        assert_eq!(v("Format_Profile").as_deref(), Some("444-10@3.0"));
        assert_eq!(v("Width").as_deref(), Some("1920"));
        assert_eq!(v("Height").as_deref(), Some("1080"));
        assert_eq!(v("ColorSpace").as_deref(), Some("YUV"));
        assert_eq!(v("ChromaSubsampling").as_deref(), Some("4:4:4"));
        assert_eq!(v("BitDepth").as_deref(), Some("10"));
        assert_eq!(v("band_idc").as_deref(), Some("0"));
    }

    #[test]
    fn rejects_non_apv_buffer() {
        let mut fa = FileAnalyze::new(b"\x00\x00\x00\x10NOTaPVsignature!");
        assert!(!parse_apv(&mut fa));
    }

    #[test]
    fn rejects_short_buffer() {
        let mut fa = FileAnalyze::new(&[0u8, 0, 0, 4]);
        assert!(!parse_apv(&mut fa));
    }
}
