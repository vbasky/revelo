use mediainfo_core::{FileAnalyze, StreamKind};

const ANNEX_B_START_CODE: [u8; 3] = [0x00, 0x00, 0x01];
const ANNEX_B_START_CODE_LONG: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

const NAL_TYPE_VPS: u8 = 32;
const NAL_TYPE_SPS: u8 = 33;
const NAL_TYPE_PPS: u8 = 34;

fn find_start_code(data: &[u8], offset: usize) -> Option<usize> {
    for i in offset..data.len().saturating_sub(2) {
        if i + 4 <= data.len() && data[i..].starts_with(&ANNEX_B_START_CODE_LONG) {
            return Some(i);
        }
        if i + 3 <= data.len() && data[i..].starts_with(&ANNEX_B_START_CODE) {
            return Some(i);
        }
    }
    None
}

fn remove_epb(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len());
    let mut i = 0;
    while i < rbsp.len() {
        if i + 2 < rbsp.len() && rbsp[i] == 0 && rbsp[i + 1] == 0 && rbsp[i + 2] == 3 {
            out.push(0);
            out.push(0);
            i += 3;
        } else {
            out.push(rbsp[i]);
            i += 1;
        }
    }
    out
}

fn read_ue(buffer: &[u8], offset: &mut usize) -> Option<u32> {
    let mut leading_zeros = 0;
    while *offset < buffer.len() * 8 {
        let byte = *offset / 8;
        let bit = 7 - (*offset % 8);
        if byte >= buffer.len() { return None; }
        if (buffer[byte] >> bit) & 1 == 1 { break; }
        leading_zeros += 1;
        *offset += 1;
    }
    if leading_zeros >= 31 { return None; }
    *offset += 1;
    let mut value = 0u32;
    for _ in 0..leading_zeros {
        if *offset >= buffer.len() * 8 { return None; }
        let byte = *offset / 8;
        let bit = 7 - (*offset % 8);
        value = (value << 1) | ((buffer[byte] >> bit) & 1) as u32;
        *offset += 1;
    }
    Some(value + (1u32 << leading_zeros) - 1)
}

fn read_bits(buffer: &[u8], offset: &mut usize, n: usize) -> Option<u32> {
    if n > 32 || *offset + n > buffer.len() * 8 { return None; }
    let mut value = 0u32;
    for _ in 0..n {
        let byte = *offset / 8;
        let bit = 7 - (*offset % 8);
        value = (value << 1) | ((buffer[byte] >> bit) & 1) as u32;
        *offset += 1;
    }
    Some(value)
}

fn skip_bits(offset: &mut usize, n: usize) {
    *offset += n;
}

fn read_profile_tier_level(clean: &[u8], offset: &mut usize) -> Option<(u8, bool, u8)> {
    // general_profile_space (2), general_tier_flag (1), general_profile_idc (5)
    let _space = read_bits(clean, offset, 2)?;
    let tier_flag = read_bits(clean, offset, 1)?;
    let profile_idc = read_bits(clean, offset, 5)? as u8;

    // general_profile_compatibility_flag[32]
    for _ in 0..32 {
        read_bits(clean, offset, 1)?;
    }

    // general_progressive_source_flag through general_frame_only_constraint_flag
    read_bits(clean, offset, 1)?;
    read_bits(clean, offset, 1)?;
    read_bits(clean, offset, 1)?;
    read_bits(clean, offset, 1)?;
    // general_reserved_zero_44bits
    read_bits(clean, offset, 32)?;
    read_bits(clean, offset, 12)?;

    let level_idc = read_bits(clean, offset, 8)? as u8;

    Some((profile_idc, tier_flag != 0, level_idc))
}

fn parse_sps(rbsp: &[u8]) -> Option<(u8, bool, u8, u32, u32, u32, u8)> {
    let clean = remove_epb(rbsp);
    if clean.len() < 6 { return None; }
    let mut offset = 0usize;

    // NAL header (2 bytes)
    read_bits(&clean, &mut offset, 16)?;

    // sps_video_parameter_set_id (4 bits)
    read_bits(&clean, &mut offset, 4)?;
    // sps_max_sub_layers_minus1 (3 bits)
    let max_sub_layers = read_bits(&clean, &mut offset, 3)?;
    // sps_temporal_id_nesting_flag (1 bit)
    read_bits(&clean, &mut offset, 1)?;

    // profile_tier_level: 2+1+5+32+4+44+8 = 96 bits
    let _space = read_bits(&clean, &mut offset, 2)?;
    let tier_flag = read_bits(&clean, &mut offset, 1)?;
    let profile_idc = read_bits(&clean, &mut offset, 5)? as u8;
    for _ in 0..32 { read_bits(&clean, &mut offset, 1)?; }
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 32)?; // reserved zero bits (first 32)
    read_bits(&clean, &mut offset, 12)?; // reserved zero bits (remaining 12)
    let level_idc = read_bits(&clean, &mut offset, 8)? as u8;

    // Sub-layer profile/level info
    for _i in 0..max_sub_layers {
        let sub_layer_profile_present = read_bits(&clean, &mut offset, 1)?;
        let sub_layer_level_present = read_bits(&clean, &mut offset, 1)?;
        if sub_layer_profile_present != 0 {
            read_profile_tier_level(&clean, &mut offset)?;
        }
        if sub_layer_level_present != 0 {
            read_bits(&clean, &mut offset, 8)?;
        }
    }

    // sps_seq_parameter_set_id (ue)
    read_ue(&clean, &mut offset)?;

    // chroma_format_idc (ue)
    let chroma_format_idc = read_ue(&clean, &mut offset)?;

    if chroma_format_idc == 3 {
        read_bits(&clean, &mut offset, 1)?;
    }

    // pic_width_in_luma_samples (ue)
    let pic_width = read_ue(&clean, &mut offset)?;
    // pic_height_in_luma_samples (ue)
    let pic_height = read_ue(&clean, &mut offset)?;

    // conformance_window_flag
    let conf_window = read_bits(&clean, &mut offset, 1)?;
    let mut conf_win_left = 0u32;
    let mut conf_win_right = 0u32;
    let mut conf_win_top = 0u32;
    let mut conf_win_bottom = 0u32;
    if conf_window != 0 {
        conf_win_left = read_ue(&clean, &mut offset)?;
        conf_win_right = read_ue(&clean, &mut offset)?;
        conf_win_top = read_ue(&clean, &mut offset)?;
        conf_win_bottom = read_ue(&clean, &mut offset)?;
    }

    // bit_depth_luma_minus8 (ue)
    let bit_depth = read_ue(&clean, &mut offset)? + 8;
    // bit_depth_chroma_minus8 (ue)
    read_ue(&clean, &mut offset)?;

    // Compute dimensions with conformance window
    let sub_width_c = match chroma_format_idc {
        0 => 1,
        1 => 2,
        2 => 2,
        3 => 1,
        _ => 2,
    };
    let sub_height_c = match chroma_format_idc {
        0 => 1,
        1 => 2,
        2 => 1,
        3 => 1,
        _ => 2,
    };

    let width = pic_width - sub_width_c * (conf_win_left + conf_win_right);
    let height = pic_height - sub_height_c * (conf_win_top + conf_win_bottom);

    Some((profile_idc, tier_flag != 0, level_idc, width, height, chroma_format_idc, bit_depth as u8))
}

fn profile_name(profile_idc: u8) -> &'static str {
    match profile_idc {
        0 => "Main",
        1 => "Main 10",
        2 => "Main Still Picture",
        3 => "Main 12",
        4 => "Main 4:2:2 10",
        5 => "Main 4:2:2 12",
        6 => "Main 4:4:4 10",
        7 => "Main 4:4:4 12",
        8 => "Main Intra",
        9 => "Main 10 Intra",
        10 => "Main 12 Intra",
        11 => "Main 4:2:2 10 Intra",
        12 => "Main 4:2:2 12 Intra",
        13 => "Main 4:4:4 10 Intra",
        14 => "Main 4:4:4 12 Intra",
        16 => "Monochrome 12",
        17 => "Monochrome 16",
        18 => "Monochrome 12 Intra",
        19 => "Monochrome 16 Intra",
        _ => "Unknown",
    }
}

fn level_name(level: u8) -> String {
    let major = level / 30;
    let minor = (level % 30) / 3;
    if level == 0 { return "0".to_owned(); }
    let s = format!("{major}.{minor}");
    if s.ends_with(".0") { s[..s.len() - 2].to_owned() } else { s }
}

pub fn parse_hevc(fa: &mut FileAnalyze) -> bool {
    fa.Element_Begin("HEVC");

    let head = fa.peek_raw(4);
    let Some(h) = head else {
        fa.Element_End();
        return false;
    };

    if h != ANNEX_B_START_CODE_LONG && &h[1..] != ANNEX_B_START_CODE {
        fa.Element_End();
        return false;
    }

    let data = if let Some(d) = fa.peek_raw(fa.Remain() as usize) {
        d.to_vec()
    } else {
        fa.Element_End();
        return false;
    };

    fa.Element_Info("Format", Some("HEVC"));

    let mut vps_found = false;
    let mut sps_info = None;

    let mut nal_offset = 0usize;
    while let Some(start) = find_start_code(&data, nal_offset) {
        let start_len = if start + 3 < data.len() && data[start..start + 4].starts_with(&ANNEX_B_START_CODE_LONG) {
            4
        } else {
            3
        };
        let nal_start = start + start_len;
        nal_offset = start + start_len;

        let nal_end = match find_start_code(&data, nal_offset) {
            Some(next) => next,
            None => data.len(),
        };

        if nal_start >= nal_end || nal_end - nal_start < 2 {
            nal_offset = nal_end;
            continue;
        }

        let nal_unit = &data[nal_start..nal_end];
        let nal_type = (nal_unit[0] >> 1) & 0x3F;

        match nal_type {
            NAL_TYPE_VPS => {
                vps_found = true;
            }
            NAL_TYPE_SPS => {
                if let Some(info) = parse_sps(nal_unit) {
                    sps_info = Some(info);
                }
            }
            _ => {}
        }

        nal_offset = nal_end;
    }

    if !vps_found || sps_info.is_none() {
        fa.Element_End();
        return false;
    }

    let (profile_idc, tier_flag, level_idc, width, height, chroma_format_idc, bit_depth) = sps_info.unwrap();

    fa.Stream_Prepare(StreamKind::Video);

    fa.Fill(StreamKind::Video, 0, "Format", "HEVC", false);
    let profile = if bit_depth <= 8 { "Main" } else { profile_name(profile_idc) };
    fa.Fill(StreamKind::Video, 0, "Format_Profile", profile, false);
    fa.Fill(StreamKind::Video, 0, "Format_Level", level_name(level_idc), false);

    if tier_flag {
        fa.Fill(StreamKind::Video, 0, "Format_Tier", "High", false);
    } else {
        fa.Fill(StreamKind::Video, 0, "Format_Tier", "Main", false);
    }

    fa.Fill(StreamKind::Video, 0, "Width", width.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Height", height.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Sampled_Width", width.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Sampled_Height", height.to_string(), false);

    if height > 0 {
        let dar = width as f64 / height as f64;
        fa.Fill(StreamKind::Video, 0, "DisplayAspectRatio", format!("{:.3}", dar), false);
    }
    fa.Fill(StreamKind::Video, 0, "PixelAspectRatio", "1.000", false);

    let chroma_sub = match chroma_format_idc {
        0 => "4:0:0",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "4:2:0",
    };
    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", chroma_sub, false);
    fa.Fill(StreamKind::Video, 0, "BitDepth", bit_depth.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);

    // General stream
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "HEVC", false);
    fa.Fill(StreamKind::General, 0, "VideoCount", "1", false);

    fa.Element_End();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_data() {
        let buf = vec![0u8; 0];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_hevc(&mut fa));
    }

    #[test]
    fn rejects_no_start_code() {
        let buf = vec![0xFFu8; 100];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_hevc(&mut fa));
    }

    #[test]
    fn parses_hevc_main_320x240() {
        let path = "/tmp/test.h265";
        let data = std::fs::read(path).unwrap_or_default();
        if data.is_empty() {
            return;
        }
        let mut fa = FileAnalyze::new(&data);
        assert!(parse_hevc(&mut fa));

        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()), Some("HEVC"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Profile").map(|z| z.as_str()), Some("Main"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Level").map(|z| z.as_str()), Some("2"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Tier").map(|z| z.as_str()), Some("Main"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()), Some("320"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()), Some("240"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "ChromaSubsampling").map(|z| z.as_str()), Some("4:2:0"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "BitDepth").map(|z| z.as_str()), Some("8"));
    }
}
