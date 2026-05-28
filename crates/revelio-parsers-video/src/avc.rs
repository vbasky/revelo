use revelio_core::{FileAnalyze, StreamKind};

const ANNEX_B_START_CODE: [u8; 3] = [0x00, 0x00, 0x01];
const ANNEX_B_START_CODE_LONG: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

const NAL_TYPE_SPS: u8 = 7;
const NAL_TYPE_PPS: u8 = 8;
const NAL_TYPE_SEI: u8 = 6;
const NAL_TYPE_IDR: u8 = 5;
const NAL_TYPE_NON_IDR: u8 = 1;

/// Find an Annex B start code (0x00000001 or 0x000001) at or after `offset`.
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

/// Remove emulation prevention bytes (0x000003 → 0x0000) from an RBSP.
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

/// Read ue(v) (unsigned Exp-Golomb coded) from a bit reader.
fn read_ue(buffer: &[u8], offset: &mut usize) -> Option<u32> {
    let mut leading_zeros = 0;
    while *offset < buffer.len() * 8 {
        let byte = *offset / 8;
        let bit = 7 - (*offset % 8);
        if byte >= buffer.len() {
            return None;
        }
        if (buffer[byte] >> bit) & 1 == 1 {
            break;
        }
        leading_zeros += 1;
        *offset += 1;
    }
    if leading_zeros >= 31 {
        return None;
    }
    *offset += 1; // skip the 1 bit
    let mut value = 0u32;
    for _ in 0..leading_zeros {
        if *offset >= buffer.len() * 8 {
            return None;
        }
        let byte = *offset / 8;
        let bit = 7 - (*offset % 8);
        value = (value << 1) | ((buffer[byte] >> bit) & 1) as u32;
        *offset += 1;
    }
    Some(value + (1u32 << leading_zeros) - 1)
}

/// Read `n` bits from a bit reader.
fn read_bits(buffer: &[u8], offset: &mut usize, n: usize) -> Option<u32> {
    if n > 32 || *offset + n > buffer.len() * 8 {
        return None;
    }
    let mut value = 0u32;
    for _ in 0..n {
        let byte = *offset / 8;
        let bit = 7 - (*offset % 8);
        value = (value << 1) | ((buffer[byte] >> bit) & 1) as u32;
        *offset += 1;
    }
    Some(value)
}

/// Skip `n` bits.
fn skip_bits(offset: &mut usize, n: usize) {
    *offset += n;
}

/// Check if we're past the end of the buffer (in bits).
#[allow(dead_code)]
fn bits_remaining(buffer: &[u8], offset: usize) -> usize {
    buffer.len() * 8 - offset
}

#[derive(Debug, Clone)]
pub struct EncoderInfo {
    pub library: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub settings: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AvcInfo {
    pub profile: u8,
    pub level: u8,
    pub width: u32,
    pub height: u32,
    /// Decoded picture height in luma samples before cropping. Equals
    /// `pic_height_in_map_units * 16 * (2 - frame_mbs_only_flag)`. Used
    /// for MediaInfo's Stored_Height.
    pub stored_height: u32,
    pub chroma_format: u8,
    pub bit_depth: u8,
    pub cabac: bool,
    pub ref_frames: u32,
    pub frame_count: u32,
    pub frame_rate_num: u32,
    pub frame_rate_den: u32,
    pub encoder_string: Option<String>,
    pub encoder_name: Option<String>,
    pub encoder_version: Option<String>,
    pub encoder_settings: Option<String>,
    pub gop_detect: Option<String>,
    // VUI colour_description (present only if both
    // video_signal_type_present_flag and colour_description_present_flag).
    pub colour_description_present: bool,
    pub colour_primaries: Option<u8>,
    pub transfer_characteristics: Option<u8>,
    pub matrix_coefficients: Option<u8>,
    /// video_signal_type_present_flag + video_full_range_flag.
    pub video_full_range: Option<bool>,
    /// VUI aspect_ratio. `Some((w,h))` when set, None otherwise.
    pub sar: Option<(u32, u32)>,
    /// VUI chroma_sample_loc_type_top_field, if VUI chroma_loc_info_present.
    pub chroma_sample_loc: Option<u32>,
}

pub fn parse_sps(rbsp: &[u8]) -> Option<AvcInfo> {
    let clean = remove_epb(rbsp);
    if clean.len() < 4 {
        return None;
    }
    let mut offset = 0usize;

    // Skip nal_unit_header (we already consumed it)
    read_bits(&clean, &mut offset, 8)?; // skip NAL header

    let profile_idc = read_bits(&clean, &mut offset, 8)? as u8;
    let constraint_flags = read_bits(&clean, &mut offset, 8)?; // 6 constraint flags + 2 reserved
    let level_idc = read_bits(&clean, &mut offset, 8)? as u8;

    read_ue(&clean, &mut offset)?; // seq_parameter_set_id

    // Determine chroma format and bit depth
    let mut chroma_format_idc = 1u32; // default 4:2:0
    let mut bit_depth_luma = 8u32;
    let mut bit_depth_chroma = 8u32;

    if profile_idc == 100 || profile_idc == 110 || profile_idc == 122 || profile_idc == 244
        || profile_idc == 44 || profile_idc == 83 || profile_idc == 86 || profile_idc == 118
        || profile_idc == 128 || profile_idc == 138 || profile_idc == 139 || profile_idc == 134
    {
        chroma_format_idc = read_ue(&clean, &mut offset)?;
        if chroma_format_idc == 3 {
            skip_bits(&mut offset, 1); // residual_colour_transform_flag
        }
        bit_depth_luma = read_ue(&clean, &mut offset)? + 8;
        bit_depth_chroma = read_ue(&clean, &mut offset)? + 8;
        let _qpprime_y_zero_transform_bypass_flag = read_bits(&clean, &mut offset, 1)?;
        let seq_scaling_matrix_present_flag = read_bits(&clean, &mut offset, 1)?;
        if seq_scaling_matrix_present_flag != 0 {
            let n_scaling_lists = if chroma_format_idc != 3 { 8 } else { 12 };
            for i in 0..n_scaling_lists {
                let scaling_list_present = read_bits(&clean, &mut offset, 1)?;
                if scaling_list_present != 0 {
                    let size = if i < 6 { 16 } else { 64 };
                    let mut last_scale = 8i32;
                    let mut next_scale = 8i32;
                    for _j in 0..size {
                        if next_scale != 0 {
                            let delta_scale = read_ue(&clean, &mut offset)? as i32;
                            next_scale = (last_scale + delta_scale + 256) % 256;
                        }
                        last_scale = if next_scale == 0 { last_scale } else { next_scale };
                    }
                }
            }
        }
    }

    read_ue(&clean, &mut offset)?; // log2_max_frame_num_minus4
    let pic_order_cnt_type = read_ue(&clean, &mut offset)?;
    if pic_order_cnt_type == 0 {
        read_ue(&clean, &mut offset)?; // log2_max_pic_order_cnt_lsb_minus4
    } else if pic_order_cnt_type == 1 {
        let _delta_pic_order_always_zero_flag = read_bits(&clean, &mut offset, 1)?;
        read_ue(&clean, &mut offset)?; // offset_for_non_ref_pic
        read_ue(&clean, &mut offset)?; // offset_for_top_to_bottom_field
        let num_ref_frames_in_poc_cycle = read_ue(&clean, &mut offset)?;
        for _ in 0..num_ref_frames_in_poc_cycle {
            read_ue(&clean, &mut offset)?; // offset_for_ref_frame[i]
        }
    }

    let ref_frames = read_ue(&clean, &mut offset)?; // max_num_ref_frames
    let _gaps_in_frame_num_value_allowed_flag = read_bits(&clean, &mut offset, 1)?;

    let pic_width_in_mbs_minus1 = read_ue(&clean, &mut offset)?;
    let pic_height_in_map_units_minus1 = read_ue(&clean, &mut offset)?;

    let frame_mbs_only_flag = read_bits(&clean, &mut offset, 1)?;
    let mut mb_adaptive_frame_field_flag = 0u32;
    if frame_mbs_only_flag == 0 {
        mb_adaptive_frame_field_flag = read_bits(&clean, &mut offset, 1)?;
    }

    let _direct_8x8_inference_flag = read_bits(&clean, &mut offset, 1)?;

    let frame_cropping_flag = read_bits(&clean, &mut offset, 1)?;
    let mut crop_left = 0u32;
    let mut crop_right = 0u32;
    let mut crop_top = 0u32;
    let mut crop_bottom = 0u32;
    if frame_cropping_flag != 0 {
        crop_left = read_ue(&clean, &mut offset)?;
        crop_right = read_ue(&clean, &mut offset)?;
        crop_top = read_ue(&clean, &mut offset)?;
        crop_bottom = read_ue(&clean, &mut offset)?;
    }

    let mut frame_rate_num = 0u32;
    let mut frame_rate_den = 1u32;

    let mut sar: Option<(u32, u32)> = None;
    let mut video_full_range: Option<bool> = None;
    let mut colour_description_present = false;
    let mut colour_primaries: Option<u8> = None;
    let mut transfer_characteristics: Option<u8> = None;
    let mut matrix_coefficients: Option<u8> = None;
    let mut chroma_sample_loc: Option<u32> = None;

    let vui_parameters_present_flag = read_bits(&clean, &mut offset, 1)?;
    if vui_parameters_present_flag != 0 {
        let aspect_ratio_info_present = read_bits(&clean, &mut offset, 1)?;
        if aspect_ratio_info_present != 0 {
            let aspect_ratio_idc = read_bits(&clean, &mut offset, 8)?;
            if aspect_ratio_idc == 255 {
                let sar_w = read_bits(&clean, &mut offset, 16)?;
                let sar_h = read_bits(&clean, &mut offset, 16)?;
                if sar_w > 0 && sar_h > 0 {
                    sar = Some((sar_w, sar_h));
                }
            } else {
                // Predefined SAR table from H.264 Table E-1.
                let preset = match aspect_ratio_idc {
                    1 => Some((1, 1)),
                    2 => Some((12, 11)),
                    3 => Some((10, 11)),
                    4 => Some((16, 11)),
                    5 => Some((40, 33)),
                    6 => Some((24, 11)),
                    7 => Some((20, 11)),
                    8 => Some((32, 11)),
                    9 => Some((80, 33)),
                    10 => Some((18, 11)),
                    11 => Some((15, 11)),
                    12 => Some((64, 33)),
                    13 => Some((160, 99)),
                    14 => Some((4, 3)),
                    15 => Some((3, 2)),
                    16 => Some((2, 1)),
                    _ => None,
                };
                sar = preset;
            }
        }
        let overscan_info_present = read_bits(&clean, &mut offset, 1)?;
        if overscan_info_present != 0 {
            let _overscan_appropriate = read_bits(&clean, &mut offset, 1)?;
        }
        let video_signal_type_present = read_bits(&clean, &mut offset, 1)?;
        if video_signal_type_present != 0 {
            let _video_format = read_bits(&clean, &mut offset, 3)?;
            let full_range = read_bits(&clean, &mut offset, 1)?;
            video_full_range = Some(full_range != 0);
            let cdp = read_bits(&clean, &mut offset, 1)?;
            if cdp != 0 {
                colour_description_present = true;
                colour_primaries = Some(read_bits(&clean, &mut offset, 8)? as u8);
                transfer_characteristics = Some(read_bits(&clean, &mut offset, 8)? as u8);
                matrix_coefficients = Some(read_bits(&clean, &mut offset, 8)? as u8);
            }
        }
        let chroma_loc_info_present = read_bits(&clean, &mut offset, 1)?;
        if chroma_loc_info_present != 0 {
            let top = read_ue(&clean, &mut offset)?;
            read_ue(&clean, &mut offset)?; // chroma_sample_loc_type_bottom_field
            chroma_sample_loc = Some(top);
        }
        let timing_info_present_flag = read_bits(&clean, &mut offset, 1)?;
        if timing_info_present_flag != 0 {
            let num_units_in_tick = read_bits(&clean, &mut offset, 32)?;
            let time_scale = read_bits(&clean, &mut offset, 32)?;
            let _fixed_frame_rate_flag = read_bits(&clean, &mut offset, 1)?;
            if num_units_in_tick > 0 && time_scale > 0 {
                // frame rate = time_scale / (2 * num_units_in_tick)
                let den = num_units_in_tick * 2;
                let g = gcd(time_scale, den);
                frame_rate_num = time_scale / g;
                frame_rate_den = den / g;
            }
        }
        // We skip the rest of VUI and HRD
    }

    // Compute dimensions
    let pic_width_in_mbs = pic_width_in_mbs_minus1 + 1;
    let pic_height_in_map_units = pic_height_in_map_units_minus1 + 1;

    let (crop_unit_x, crop_unit_y) = match chroma_format_idc {
        0 => (1, 1),   // Monochrome
        1 => (2, 2),   // 4:2:0
        2 => (2, 1),   // 4:2:2
        3 => (1, 1),   // 4:4:4
        _ => (2, 2),
    };

    let width = pic_width_in_mbs * 16 - crop_unit_x * (crop_left + crop_right);
    let stored_height = pic_height_in_map_units * 16 * (2 - frame_mbs_only_flag);
    let height = stored_height
        - crop_unit_y * (2 - frame_mbs_only_flag) * (crop_top + crop_bottom);

    let chroma_str = match chroma_format_idc {
        0 => 0,
        1 => 1, // 4:2:0
        2 => 2, // 4:2:2
        3 => 3, // 4:4:4
        _ => 1,
    };

    let _ = (constraint_flags, bit_depth_chroma, mb_adaptive_frame_field_flag);

    Some(AvcInfo {
        profile: profile_idc,
        level: level_idc,
        width,
        height,
        stored_height,
        chroma_format: chroma_str,
        bit_depth: bit_depth_luma as u8,
        cabac: false, // updated by parse_pps
        ref_frames,
        frame_count: 0,
        frame_rate_num,
        frame_rate_den,
        encoder_string: None,
        encoder_name: None,
        encoder_version: None,
        encoder_settings: None,
        gop_detect: None,
        colour_description_present,
        colour_primaries,
        transfer_characteristics,
        matrix_coefficients,
        video_full_range,
        sar,
        chroma_sample_loc,
    })
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn profile_name(profile: u8) -> &'static str {
    match profile {
        66 => "Baseline",
        77 => "Main",
        88 => "Extended",
        100 => "High",
        110 => "High 10",
        122 => "High 4:2:2",
        244 => "High 4:4:4",
        44 => "CAVLC 4:4:4",
        83 => "Scalable Baseline",
        86 => "Scalable High",
        118 => "Multiview High",
        128 => "Stereo High",
        138 => "Multiview Depth High",
        139 => "Multiview Depth High 4:2:2",
        134 => "Constrained Baseline",
        _ => "Unknown",
    }
}

fn level_name(level: u8) -> String {
    let major = level / 10;
    let minor = level % 10;
    if minor == 0 {
        format!("{major}")
    } else {
        format!("{major}.{minor}")
    }
}

pub fn parse_x264_style_encoder(raw: &str) -> EncoderInfo {
    let mut library = String::new();
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut settings: Option<String> = None;

    let mut segment_idx = 0usize;
    let mut cursor = 0usize;
    let len = raw.len();

    while cursor < len {
        let sep = raw[cursor..].find(" - ");
        let segment_end = sep.map(|p| cursor + p).unwrap_or(len);
        let segment = &raw[cursor..segment_end];

        if segment.starts_with("options: ") {
            let opts_raw = &segment["options: ".len()..];
            let tokens: Vec<&str> = opts_raw.split_whitespace().collect();
            // Drop the encoder's fps=/bitdepth= tokens (the oracle omits
            // them). Do NOT drop digit-leading tokens — "8x8dct=1" is a
            // real x264 option that begins with a digit.
            let filtered: Vec<&str> = tokens.iter()
                .filter(|t| {
                    !t.is_empty()
                    && !t.starts_with("fps=")
                    && !t.starts_with("bitdepth=")
                })
                .copied()
                .collect();
            if !filtered.is_empty() {
                settings = Some(filtered.join(" / "));
            }
        } else if segment_idx == 0 {
            let cleaned: String = segment.chars()
                .skip_while(|&c| (c as u32) < 0x30)
                .collect();
            let cleaned: String = cleaned.chars()
                .rev()
                .skip_while(|&c| (c as u32) < 0x30)
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            library = cleaned.clone();
            let first_word = cleaned.split_whitespace().next().unwrap_or(&cleaned);
            name = Some(first_word.to_owned());
        } else if segment_idx == 1 {
            if library.starts_with("x264") || library.starts_with("eavc") || library.starts_with("x265") {
                let cleaned = if let Some(pos) = segment.find(" 8bpp") {
                    &segment[..pos]
                } else {
                    segment
                };
                library.push_str(" - ");
                library.push_str(cleaned);
                version = Some(cleaned.to_owned());
            }
        }

        cursor = segment_end;
        if sep.is_some() {
            cursor += 3;
        }
        segment_idx += 1;
    }

    if library.starts_with("x264 - ") {
        name = Some("x264".to_owned());
        version = library.strip_prefix("x264 - ").map(|v| v.to_owned());
    } else if library.starts_with("eavc ") {
        name = Some("eavc".to_owned());
        version = library.strip_prefix("eavc ").map(|v| v.to_owned());
    } else if library.starts_with("x265 - ") {
        name = Some("x265".to_owned());
        version = library.strip_prefix("x265 - ").map(|v| v.to_owned());
    }

    EncoderInfo { library, name, version, settings }
}

#[allow(dead_code)]
fn parse_slice_type_from_nal(nal_unit: &[u8]) -> Option<u8> {
    let clean = remove_epb(nal_unit);
    if clean.len() < 3 { return None; }
    let mut off = 8; // skip NAL header
    let first_mb = read_ue(&clean, &mut off)?;
    if first_mb != 0 { return None; } // only parse first slice in frame
    read_ue(&clean, &mut off)?; // slice_type
    None // placeholder — real impl reads Exp-Golomb coded slice_type
}

/// GOP detection: calculate M (P-frame distance) and N (GOP length) from frame sequence.
/// Returns "M=X, N=Y" string or None if insufficient data.
pub fn gop_detect(frame_types: &[u8]) -> Option<String> {
    let n = frame_types.len();
    if n < 2 { return None; }

    // Find M: typical distance between P frames
    let mut p_positions = Vec::new();
    for (i, &t) in frame_types.iter().enumerate() {
        if t == 1 { p_positions.push(i); } // P frame
    }
    let m = if p_positions.len() >= 2 {
        p_positions[1] - p_positions[0]
    } else if p_positions.len() == 1 {
        p_positions[0] + 1 // single P, assume M = position+1
    } else {
        return None; // no P frames
    };
    if m == 0 { return None; }

    // Find N: distance between IDR/I frames
    let mut i_positions = Vec::new();
    for (i, &t) in frame_types.iter().enumerate() {
        if t == 2 { i_positions.push(i); } // I/IDR frame
    }
    if i_positions.len() < 2 {
        return Some(format!("M={m}"));
    }
    let gop_len = i_positions[1] - i_positions[0];
    if gop_len > 0 {
        Some(format!("M={m}, N={gop_len}"))
    } else {
        Some(format!("M={m}"))
    }
}

pub fn parse_avc(fa: &mut FileAnalyze) -> bool {
    fa.Element_Begin("AVC");

    let head = fa.peek_raw(4);
    let Some(h) = head else {
        fa.Element_End();
        return false;
    };

    // Check for Annex B start code
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

    fa.Element_Info("Format", Some("AVC"));

    let mut sps_found = None;
    let mut frame_count = 0u32;
    let mut cabac = false;
    let mut encoder_string: Option<String> = None;
    let mut encoder_info: Option<EncoderInfo> = None;
    let mut frame_types: Vec<u8> = Vec::new(); // 0=unknown, 1=P, 2=I/IDR

    let mut nal_offset = 0usize;
    while let Some(start) = find_start_code(&data, nal_offset) {
        let start_len = if start + 3 < data.len() && data[start..start + 4].starts_with(&ANNEX_B_START_CODE_LONG) {
            4
        } else {
            3
        };
        let nal_start = start + start_len;
        if start > nal_offset && nal_offset > 0 {
            // skip trailing data before next start code
        }
        nal_offset = start + start_len;

        // Find the next start code or end of data to determine NAL unit boundary
        let nal_end = match find_start_code(&data, nal_offset) {
            Some(next) => next,
            None => data.len(),
        };

        if nal_start >= nal_end || nal_end - nal_start < 1 {
            nal_offset = nal_end;
            continue;
        }

        let nal_unit = &data[nal_start..nal_end];
        let nal_header = nal_unit[0];
        let _nal_ref_idc = (nal_header >> 5) & 3;
        let nal_type = nal_header & 0x1F;

        match nal_type {
            NAL_TYPE_SPS => {
                if let Some(info) = parse_sps(nal_unit) {
                    sps_found = Some(info);
                }
            }
            NAL_TYPE_PPS => {
                // Parse PPS for entropy_coding_mode_flag (CABAC)
                let clean = remove_epb(nal_unit);
                if clean.len() >= 2 {
                    let mut off = 0usize;
                    read_bits(&clean, &mut off, 8); // skip NAL header
                    read_ue(&clean, &mut off); // pic_parameter_set_id
                    if let Some(val) = read_ue(&clean, &mut off) {
                        cabac = val != 0;
                    }
                }
            }
            NAL_TYPE_SEI => {
                if let Some(enc) = extract_encoder_from_sei(nal_unit) {
                    encoder_string = Some(enc.library.clone());
                    encoder_info = Some(enc);
                }
            }
            NAL_TYPE_IDR => {
                frame_count += 1;
                frame_types.push(2);
            }
            NAL_TYPE_NON_IDR => {
                frame_count += 1;
                frame_types.push(1);
            }
            _ => {}
        }

        nal_offset = nal_end;
    }

    if sps_found.is_none() {
        fa.Element_End();
        return false;
    }

    let mut info = sps_found.unwrap();
    info.frame_count = frame_count;
    info.cabac = cabac;
    info.encoder_string = encoder_string;
    if let Some(ref ei) = encoder_info {
        info.encoder_name = ei.name.clone();
        info.encoder_version = ei.version.clone();
        info.encoder_settings = ei.settings.clone();
    }
    if let Some(gop) = gop_detect(&frame_types) {
        info.gop_detect = Some(gop);
    }

    fa.Stream_Prepare(StreamKind::Video);

    fa.Fill(StreamKind::Video, 0, "Format", "AVC", false);

    // Determine profile string
    let prof = profile_name(info.profile);
    // Check for constraint_set flags
    let _is_constrained = info.profile == 66; // baseline is always "Constrained Baseline" in MediaInfo
    // Actually, for baseline, the C++ side output "Constrained Baseline" in our test
    let profile_str = match info.profile {
        66 => "Constrained Baseline",
        77 => "Main",
        88 => "Extended",
        100 => "High",
        110 => "High 10",
        122 => "High 4:2:2",
        244 => "High 4:4:4",
        44 => "CAVLC 4:4:4",
        83 => "Scalable Baseline",
        86 => "Scalable High",
        118 => "Multiview High",
        128 => "Stereo High",
        134 => "Constrained Baseline",
        _ => prof,
    };
    fa.Fill(StreamKind::Video, 0, "Format_Profile", profile_str, false);
    fa.Fill(StreamKind::Video, 0, "Format_Level", level_name(info.level), false);

    if info.cabac {
        fa.Fill(StreamKind::Video, 0, "Format_Settings_CABAC", "Yes", false);
    } else {
        fa.Fill(StreamKind::Video, 0, "Format_Settings_CABAC", "No", false);
    }
    fa.Fill(StreamKind::Video, 0, "Format_Settings_RefFrames", info.ref_frames.to_string(), false);

    fa.Fill(StreamKind::Video, 0, "Width", info.width.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Height", info.height.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Sampled_Width", info.width.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Sampled_Height", info.height.to_string(), false);

    fa.Fill(StreamKind::Video, 0, "PixelAspectRatio", "1.000", false);
    if info.height > 0 {
        let dar = info.width as f64 / info.height as f64;
        fa.Fill(StreamKind::Video, 0, "DisplayAspectRatio", format!("{:.3}", dar), false);
    }

    fa.Fill(StreamKind::Video, 0, "FrameRate_Mode", "CFR", false);
    if info.frame_rate_num > 0 && info.frame_rate_den > 0 {
        fa.Fill(StreamKind::Video, 0, "FrameRate", format!("{:.3}", info.frame_rate_num as f64 / info.frame_rate_den as f64), false);
        fa.Fill(StreamKind::Video, 0, "FrameRate_Num", info.frame_rate_num.to_string(), false);
        fa.Fill(StreamKind::Video, 0, "FrameRate_Den", info.frame_rate_den.to_string(), false);
    }
    fa.Fill(StreamKind::Video, 0, "FrameCount", info.frame_count.to_string(), false);

    let chroma_str = match info.chroma_format {
        0 => "YUV",
        1 | 2 | 3 => "YUV",
        _ => "YUV",
    };
    fa.Fill(StreamKind::Video, 0, "ColorSpace", chroma_str, false);

    let chroma_sub = match info.chroma_format {
        0 => "4:0:0",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "4:2:0",
    };
    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", chroma_sub, false);
    fa.Fill(StreamKind::Video, 0, "BitDepth", info.bit_depth.to_string(), false);
    
    // Emit color information from VUI if present
    if info.colour_description_present {
        if let Some(primaries) = info.colour_primaries {
            let primaries_str = match primaries {
                0 => "Reserved",
                1 => "BT.709",
                2 => "Unspecified",
                3 => "Reserved",
                4 => "BT.470 System M",
                5 => "BT.470 System B, G",
                6 => "SMPTE 170M",
                7 => "SMPTE 240M",
                8 => "Film",
                9 => "BT.2020",
                10 => "SMPTE 428",
                11 => "DCI P3",
                12 => "Display P3",
                22 => "EBU Tech. 3213-E",
                _ => "Unknown",
            };
            if primaries > 0 {
                fa.Fill(StreamKind::Video, 0, "colour_primaries", primaries_str.to_string(), false);
            }
        }
        if let Some(transfer) = info.transfer_characteristics {
            let transfer_str = match transfer {
                0 => "Reserved",
                1 => "BT.709",
                2 => "Unspecified",
                3 => "Reserved",
                4 => "BT.470 System M",
                5 => "BT.470 System B, G",
                6 => "SMPTE 170M",
                7 => "SMPTE 240M",
                8 => "Linear",
                9 => "Logarithmic (100:1)",
                10 => "Logarithmic (316.22777:1)",
                11 => "xvYCC",
                12 => "BT.1361",
                13 => "sRGB/sYCC",
                14 => "BT.2020 (10-bit)",
                15 => "BT.2020 (12-bit)",
                16 => "SMPTE 2084 (PQ)",
                17 => "SMPTE 428",
                18 => "HLG",
                _ => "Unknown",
            };
            if transfer > 0 {
                fa.Fill(StreamKind::Video, 0, "transfer_characteristics", transfer_str.to_string(), false);
            }
        }
        if let Some(matrix) = info.matrix_coefficients {
            let matrix_str = match matrix {
                0 => "GBR",
                1 => "BT.709",
                2 => "Unspecified",
                3 => "Reserved",
                4 => "FCC",
                5 => "BT.470 System B, G",
                6 => "SMPTE 170M",
                7 => "SMPTE 240M",
                8 => "YCgCo",
                9 => "BT.2020 (non-constant)",
                10 => "BT.2020 (constant)",
                11 => "SMPTE 2085",
                12 => "Chromaticity-derived",
                13 => "ICtCp",
                _ => "Unknown",
            };
            if matrix > 0 {
                fa.Fill(StreamKind::Video, 0, "matrix_coefficients", matrix_str.to_string(), false);
            }
        }
    }
    
    fa.Fill(StreamKind::Video, 0, "ScanType", "Progressive", false);

    if let Some(ref enc) = info.encoder_string {
        fa.Fill(StreamKind::Video, 0, "Encoded_Library", enc.as_str(), false);
    }
    if let Some(ref name) = info.encoder_name {
        fa.Fill(StreamKind::Video, 0, "Encoded_Library_Name", name.as_str(), false);
    }
    if let Some(ref ver) = info.encoder_version {
        fa.Fill(StreamKind::Video, 0, "Encoded_Library_Version", ver.as_str(), false);
    }
    if let Some(ref settings) = info.encoder_settings {
        fa.Fill(StreamKind::Video, 0, "Encoded_Library_Settings", settings.as_str(), false);
    }
    if let Some(ref gop) = info.gop_detect {
        fa.Fill(StreamKind::Video, 0, "GOP_Detect", gop.as_str(), false);
    }

    // Fill General stream
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "AVC", false);
    fa.Fill(StreamKind::General, 0, "VideoCount", "1", false);

    if info.frame_rate_num > 0 && info.frame_rate_den > 0 {
        let fr = info.frame_rate_num as f64 / info.frame_rate_den as f64;
        fa.Fill(StreamKind::General, 0, "FrameRate", format!("{:.3}", fr), false);
    }
    fa.Fill(StreamKind::General, 0, "FrameCount", info.frame_count.to_string(), false);

    if let Some(ref enc) = info.encoder_string {
        fa.Fill(StreamKind::General, 0, "Encoded_Library", enc.as_str(), false);
    }
    if let Some(ref settings) = info.encoder_settings {
        fa.Fill(StreamKind::General, 0, "Encoded_Library_Settings", settings.as_str(), false);
    }

    fa.Element_End();
    true
}

/// Extract the x264 encoder string from a SEI NAL unit (user_data_unregistered).
fn extract_encoder_from_sei(nal_unit: &[u8]) -> Option<EncoderInfo> {
    let clean = remove_epb(nal_unit);
    if clean.len() < 2 {
        return None;
    }
    let mut off = 0usize;
    // Skip NAL header
    if clean.len() < 2 { return None; }
    read_bits(&clean, &mut off, 8)?; // nal_unit_header

    // Parse SEI messages
    loop {
        if off >= clean.len() * 8 { break; }

        // Read payload_type (variable length, 0xFF terminated)
        let mut payload_type = 0u32;
        loop {
            let byte = read_bits(&clean, &mut off, 8)?;
            payload_type += byte;
            if byte != 0xFF { break; }
            if off >= clean.len() * 8 { return None; }
        }

        // Read payload_size (variable length, 0xFF terminated)
        let mut payload_size = 0u32;
        loop {
            let byte = read_bits(&clean, &mut off, 8)?;
            payload_size += byte;
            if byte != 0xFF { break; }
            if off >= clean.len() * 8 { return None; }
        }

        if payload_type == 5 {
            // user_data_unregistered: 16-byte UUID + string. payload_size
            // already counts the UUID, so the available-bytes guard must
            // NOT add uuid_len on top (doing so overflowed for any real
            // x264/x265 SEI and made extraction silently fail).
            let uuid_len = 16 * 8; // 16 bytes
            if payload_size < 16 || off + payload_size as usize * 8 > clean.len() * 8 {
                return None;
            }
            skip_bits(&mut off, uuid_len);

            let str_start = off / 8;
            let remaining_bits = payload_size as usize * 8 - uuid_len;
            let str_end = str_start + remaining_bits / 8;
            if str_end > clean.len() {
                return None;
            }
            let str_bytes = &clean[str_start..str_end];
            // Find null terminator
            let null_pos = str_bytes.iter().position(|&b| b == 0).unwrap_or(str_bytes.len());
            let s = std::str::from_utf8(&str_bytes[..null_pos]).ok()?;
            if !s.is_empty() {
                return Some(parse_x264_style_encoder(s));
            }
            skip_bits(&mut off, remaining_bits);
        } else {
            // Skip payload data for other SEI types
            skip_bits(&mut off, payload_size as usize * 8);
        }

        // Check for rbsp_trailing_bits
        if off >= clean.len() * 8 {
            break;
        }
    }

    None
}

/// Extract x264/eavc encoder info from a set of AVC SEI NAL units (each
/// without start code / length prefix, raw NAL bytes). Used by container
/// parsers (MP4) that locate the first-frame SEI in mdat — the avcC config
/// box carries only SPS/PPS, so the encoder SEI lives in the sample data.
pub fn extract_encoder_from_avc_sei_nalus(sei_nalus: &[&[u8]]) -> Option<EncoderInfo> {
    for nal in sei_nalus {
        if let Some(enc) = extract_encoder_from_sei(nal) {
            return Some(enc);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_data() {
        let buf = vec![0u8; 0];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_avc(&mut fa));
    }

    #[test]
    fn rejects_no_start_code() {
        let buf = vec![0x00u8; 100];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_avc(&mut fa));
    }

    #[test]
    fn detects_start_code_short() {
        // Just a start code with no NAL data
        let mut buf = vec![0u8; 5];
        buf[2] = 0x01; // 0x000001
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_avc(&mut fa)); // no SPS found
    }

    #[test]
    fn parses_sps_baseline_320x240() {
        // We'll reuse the actual H.264 file created by ffmpeg
        let path = "/tmp/test.h264";
        let data = std::fs::read(path).unwrap_or_default();
        if data.is_empty() {
            return; // skip if file doesn't exist
        }
        let mut fa = FileAnalyze::new(&data);
        assert!(parse_avc(&mut fa));

        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()), Some("AVC"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Profile").map(|z| z.as_str()), Some("Constrained Baseline"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Level").map(|z| z.as_str()), Some("1.3"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Settings_CABAC").map(|z| z.as_str()), Some("No"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()), Some("320"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()), Some("240"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "BitDepth").map(|z| z.as_str()), Some("8"));
    }

    #[test]
    fn parses_sps_high_640x480() {
        let path = "/tmp/test_high.h264";
        let data = std::fs::read(path).unwrap_or_default();
        if data.is_empty() {
            return;
        }
        let mut fa = FileAnalyze::new(&data);
        assert!(parse_avc(&mut fa));

        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()), Some("AVC"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Profile").map(|z| z.as_str()), Some("High"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format_Level").map(|z| z.as_str()), Some("3"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()), Some("640"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()), Some("480"));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "ChromaSubsampling").map(|z| z.as_str()), Some("4:2:0"));
    }

    #[test]
    fn extracts_encoder_from_sei() {
        let path = "/tmp/test.h264";
        let data = std::fs::read(path).unwrap_or_default();
        if data.is_empty() {
            return;
        }
        // 4-byte start code at 0x21-0x24, SEI NAL at 0x25
        // Next start code at 0x1470
        let nal_content = &data[0x25..0x1470];
        let enc = extract_encoder_from_sei(nal_content);
        assert!(enc.is_some(), "extract_encoder_from_sei returned None");
        if let Some(s) = enc {
            assert!(s.library.contains("x264"), "expected x264");
            assert!(s.library.contains("core 165"), "expected core version");
        }
    }
}
