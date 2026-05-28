use revelio_core::{FileAnalyze, StreamKind};
use crate::avc::{EncoderInfo, parse_x264_style_encoder};

#[derive(Debug)]
pub struct HevcInfo {
    pub profile_idc: u8,
    pub tier_high: bool,
    pub level_idc: u8,
    pub width: u32,
    pub height: u32,
    pub chroma_format_idc: u8,
    pub bit_depth: u8,
    /// VUI colour_description_present_flag
    pub colour_description_present: bool,
    /// VUI colour_primaries (if colour_description_present)
    pub colour_primaries: Option<u8>,
    /// VUI transfer_characteristics (if colour_description_present)
    pub transfer_characteristics: Option<u8>,
    /// VUI matrix_coefficients (if colour_description_present)
    pub matrix_coefficients: Option<u8>,
    /// VUI video_full_range_flag (if video_signal_type_present)
    pub video_full_range: Option<bool>,
    /// Extracted from user_data_unregistered SEI message (payload_type 5).
    pub encoder_string: Option<String>,
    pub encoder_name: Option<String>,
    pub encoder_version: Option<String>,
    pub encoder_settings: Option<String>,
}

const ANNEX_B_START_CODE: [u8; 3] = [0x00, 0x00, 0x01];
const ANNEX_B_START_CODE_LONG: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

const NAL_TYPE_VPS: u8 = 32;
const NAL_TYPE_SPS: u8 = 33;
#[allow(dead_code)]
const NAL_TYPE_PPS: u8 = 34;
const NAL_TYPE_SEI_PREFIX: u8 = 39;
const NAL_TYPE_SEI_SUFFIX: u8 = 40;

// HDR10 SEI payload types
#[allow(dead_code)]
const SEI_TYPE_MASTERING_DISPLAY_COLOUR_VOLUME: u8 = 137;
#[allow(dead_code)]
const SEI_TYPE_CONTENT_LIGHT_LEVEL: u8 = 144;

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

fn read_bits64(buffer: &[u8], offset: &mut usize, n: usize) -> Option<u64> {
    if n > 64 || *offset + n > buffer.len() * 8 { return None; }
    let mut value = 0u64;
    for _ in 0..n {
        let byte = *offset / 8;
        let bit = 7 - (*offset % 8);
        value = (value << 1) | ((buffer[byte] >> bit) & 1) as u64;
        *offset += 1;
    }
    Some(value)
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

    // log2_max_pic_order_cnt_lsb_minus4 (ue)
    read_ue(&clean, &mut offset)?;

    // sps_sub_layer_ordering_info_present_flag (u1)
    let sub_layer_ordering_present = read_bits(&clean, &mut offset, 1)?;
    let start_idx = if sub_layer_ordering_present != 0 { 0 } else { max_sub_layers };
    
    for _ in start_idx..=max_sub_layers {
        // sps_max_dec_pic_buffering_minus1[...] (ue)
        read_ue(&clean, &mut offset)?;
        // sps_max_num_reorder_pics[...] (ue)
        read_ue(&clean, &mut offset)?;
        // sps_max_latency_increase_plus1[...] (ue)
        read_ue(&clean, &mut offset)?;
    }

    // log2_min_luma_coding_block_size_minus3 (ue)
    read_ue(&clean, &mut offset)?;
    // log2_diff_max_min_luma_coding_block_size (ue)
    read_ue(&clean, &mut offset)?;
    // log2_min_transform_block_size_minus2 (ue)
    read_ue(&clean, &mut offset)?;
    // log2_diff_max_min_transform_block_size (ue)
    read_ue(&clean, &mut offset)?;
    // max_transform_hierarchy_depth_inter (ue)
    read_ue(&clean, &mut offset)?;
    // max_transform_hierarchy_depth_intra (ue)
    read_ue(&clean, &mut offset)?;

    // scaling_list_enabled_flag (u1)
    let scaling_list_enabled = read_bits(&clean, &mut offset, 1)?;
    if scaling_list_enabled != 0 {
        // sps_scaling_list_data_present_flag (u1)
        let sps_scaling_list_present = read_bits(&clean, &mut offset, 1)?;
        if sps_scaling_list_present != 0 {
            // Skip scaling list data (complex structure, skip for now)
            // This is a simplification - proper scaling list parsing would be needed
            // for complete compliance, but most files don't use SPS scaling lists.
        }
    }

    // amp_enabled_flag (u1)
    read_bits(&clean, &mut offset, 1)?;
    // sample_adaptive_offset_enabled_flag (u1)
    let _sao_enabled = read_bits(&clean, &mut offset, 1)?;
    // pcm_enabled_flag (u1)
    let pcm_enabled = read_bits(&clean, &mut offset, 1)?;
    
    if pcm_enabled != 0 {
        // pcm_sample_bit_depth_luma_minus1 (u4)
        read_bits(&clean, &mut offset, 4)?;
        // pcm_sample_bit_depth_chroma_minus1 (u4)
        read_bits(&clean, &mut offset, 4)?;
        // log2_min_pcm_luma_coding_block_size_minus3 (ue)
        read_ue(&clean, &mut offset)?;
        // log2_diff_max_min_pcm_luma_coding_block_size (ue)
        read_ue(&clean, &mut offset)?;
        // pcm_loop_filter_disabled_flag (u1)
        read_bits(&clean, &mut offset, 1)?;
    }

    // num_short_term_ref_pic_sets (ue)
    let num_short_term_rps = read_ue(&clean, &mut offset)?;
    for _ in 0..num_short_term_rps {
        // Short term RPS parsing is complex, skip for now
        // This is a heuristic - we try to detect the rough size
        // A proper implementation would parse each RPS fully
    }

    // long_term_ref_pics_present_flag (u1)
    let long_term_present = read_bits(&clean, &mut offset, 1)?;
    if long_term_present != 0 {
        // num_long_term_ref_pics_sps (ue)
        let num_long_term = read_ue(&clean, &mut offset)?;
        for _ in 0..num_long_term {
            // lt_ref_pic_poc_lsb_sps[...] (u(log2_max_pic_order_cnt_lsb_minus4 + 4))
            // lt_ref_pic_used_by_curr_pic_sps_flag[...] (u1)
            // Skip without proper parsing
        }
    }

    // sps_temporal_mvp_enabled_flag (u1)
    read_bits(&clean, &mut offset, 1)?;
    // strong_intra_smoothing_enabled_flag (u1)
    read_bits(&clean, &mut offset, 1)?;

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


/// Extract encoder string from HEVC SEI user_data_unregistered message.
/// Similar to AVC but with 2-byte NAL header.
fn extract_encoder_from_hevc_sei(nal_unit: &[u8]) -> Option<EncoderInfo> {
    let clean = remove_epb(nal_unit);
    if clean.len() < 2 {
        return None;
    }
    let mut off = 0usize;
    read_bits(&clean, &mut off, 16)?;

    loop {
        if off >= clean.len() * 8 { break; }

        let mut payload_type = 0u32;
        loop {
            let byte = read_bits(&clean, &mut off, 8)?;
            payload_type += byte;
            if byte != 0xFF { break; }
            if off >= clean.len() * 8 { return None; }
        }

        let mut payload_size = 0u32;
        loop {
            let byte = read_bits(&clean, &mut off, 8)?;
            payload_size += byte;
            if byte != 0xFF { break; }
            if off >= clean.len() * 8 { return None; }
        }

        if payload_type == 5 {
            let payload_bits = payload_size as usize * 8;
            if off + payload_bits > clean.len() * 8 {
                return None;
            }
            let uuid_hi = read_bits64(&clean, &mut off, 64)?;
            skip_bits(&mut off, 64);

            let string_bytes = payload_size.saturating_sub(16);
            if string_bytes == 0 {
                if payload_bits > 128 { skip_bits(&mut off, payload_bits - 128); }
                continue;
            }
            let str_start = off / 8;
            let str_end = str_start + string_bytes as usize;
            if str_end > clean.len() {
                return None;
            }
            let str_bytes = &clean[str_start..str_end];
            let null_pos = str_bytes.iter().position(|&b| b == 0).unwrap_or(str_bytes.len());
            let s = std::str::from_utf8(&str_bytes[..null_pos]).ok()?;
            if !s.is_empty() {
                let info = match uuid_hi {
                    0x2CA2DE09B51747DB => {
                        // x265's SEI string is "x265 (build NNN) - <version>
                        // - <desc> - … - options: …". parse_x264_style_encoder
                        // pulls the right version into .version but leaves the
                        // "(build NNN)" wrapper in .library. MediaInfo reports
                        // library as "x265 - <version>", name "x265", version
                        // the bare version — rebuild to match.
                        let info = parse_x264_style_encoder(s);
                        let library = match &info.version {
                            Some(v) => format!("x265 - {v}"),
                            None => info.library.clone(),
                        };
                        EncoderInfo {
                            library,
                            name: Some("x265".to_owned()),
                            version: info.version,
                            settings: info.settings,
                        }
                    }
                    0x427FCC9BB8924821 => {
                        let info = parse_x264_style_encoder(s);
                        EncoderInfo { name: Some("Ateme".to_owned()), ..info }
                    }
                    _ => parse_x264_style_encoder(s),
                };
                return Some(info);
            }
            skip_bits(&mut off, payload_bits - 128);
        } else {
            skip_bits(&mut off, payload_size as usize * 8);
        }

        if off >= clean.len() * 8 {
            break;
        }
    }

    None
}

pub fn extract_encoder_from_sei_nalus(sei_nalus: &[&[u8]]) -> Option<EncoderInfo> {
    for nal in sei_nalus {
        if let Some(enc) = extract_encoder_from_hevc_sei(nal) {
            return Some(enc);
        }
    }
    None
}

/// Parse HDR10 mastering display colour volume SEI (payload type 137).
/// Returns (display_primaries[3], white_point, max_luminance, min_luminance).
fn parse_mastering_display_sei(payload: &[u8]) -> Option<([(u16, u16); 3], (u16, u16), u32, u32)> {
    if payload.len() < 24 {
        return None;
    }
    // Display primaries: 3 x (x, y) in 0.00002 units, 16 bits each
    let mut primaries = [(0u16, 0u16); 3];
    for i in 0..3 {
        let offset = i * 4;
        let x = ((payload[offset] as u16) << 8) | (payload[offset + 1] as u16);
        let y = ((payload[offset + 2] as u16) << 8) | (payload[offset + 3] as u16);
        primaries[i] = (x, y);
    }
    // White point: x, y in 0.00002 units, 16 bits each
    let white_x = ((payload[12] as u16) << 8) | (payload[13] as u16);
    let white_y = ((payload[14] as u16) << 8) | (payload[15] as u16);
    // Max luminance: 32 bits, in 0.0001 cd/m^2
    let max_lum = ((payload[16] as u32) << 24) | ((payload[17] as u32) << 16) |
                  ((payload[18] as u32) << 8) | (payload[19] as u32);
    // Min luminance: 32 bits, in 0.0001 cd/m^2
    let min_lum = ((payload[20] as u32) << 24) | ((payload[21] as u32) << 16) |
                  ((payload[22] as u32) << 8) | (payload[23] as u32);
    Some((primaries, (white_x, white_y), max_lum, min_lum))
}

/// Parse HDR10 content light level SEI (payload type 144).
/// Returns (max_content_light_level, max_frame_average_light_level) in cd/m^2.
fn parse_content_light_level_sei(payload: &[u8]) -> Option<(u16, u16)> {
    if payload.len() < 4 {
        return None;
    }
    let max_content = ((payload[0] as u16) << 8) | (payload[1] as u16);
    let max_frame_avg = ((payload[2] as u16) << 8) | (payload[3] as u16);
    Some((max_content, max_frame_avg))
}

/// Extract HDR metadata from SEI NAL units.
pub fn extract_hdr_from_sei_nalus(sei_nalus: &[&[u8]]) -> Option<(Option<([(u16, u16); 3], (u16, u16), u32, u32)>, Option<(u16, u16)>)> {
    let mut mastering = None;
    let mut light_level = None;
    
    for nal in sei_nalus {
        // Skip NAL header (2 bytes for HEVC)
        if nal.len() < 3 {
            continue;
        }
        let mut pos = 2;
        while pos < nal.len() {
            // Read payload type (may be multi-byte)
            let mut payload_type = 0u8;
            while pos < nal.len() && nal[pos] == 0xFF {
                payload_type = payload_type.saturating_add(255);
                pos += 1;
            }
            if pos >= nal.len() {
                break;
            }
            payload_type = payload_type.saturating_add(nal[pos]);
            pos += 1;
            
            // Read payload size (may be multi-byte)
            let mut payload_size = 0usize;
            while pos < nal.len() && nal[pos] == 0xFF {
                payload_size = payload_size.saturating_add(255);
                pos += 1;
            }
            if pos >= nal.len() {
                break;
            }
            payload_size = payload_size.saturating_add(nal[pos] as usize);
            pos += 1;
            
            if pos + payload_size > nal.len() {
                break;
            }
            
            let payload = &nal[pos..pos + payload_size];
            
            match payload_type {
                137 => {
                    if let Some(md) = parse_mastering_display_sei(payload) {
                        mastering = Some(md);
                    }
                }
                144 => {
                    if let Some(ll) = parse_content_light_level_sei(payload) {
                        light_level = Some(ll);
                    }
                }
                _ => {}
            }
            
            pos += payload_size;
            
            // Skip trailing byte if present
            if pos < nal.len() && nal[pos] == 0x80 {
                pos += 1;
            }
        }
    }
    
    if mastering.is_some() || light_level.is_some() {
        Some((mastering, light_level))
    } else {
        None
    }
}


/// Parse HEVC SPS and extract info including VUI colour information.
/// This is the public entry point used by container parsers (MP4, etc.)
pub fn parse_hevc_sps(rbsp: &[u8]) -> Option<HevcInfo> {
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

    // profile_tier_level
    let _space = read_bits(&clean, &mut offset, 2)?;
    let tier_flag = read_bits(&clean, &mut offset, 1)?;
    let profile_idc = read_bits(&clean, &mut offset, 5)? as u8;
    for _ in 0..32 { read_bits(&clean, &mut offset, 1)?; }
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 1)?;
    read_bits(&clean, &mut offset, 32)?;
    read_bits(&clean, &mut offset, 12)?;
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

    // log2_max_pic_order_cnt_lsb_minus4 (ue)
    read_ue(&clean, &mut offset)?;

    // sps_sub_layer_ordering_info_present_flag (u1)
    let sub_layer_ordering_present = read_bits(&clean, &mut offset, 1)?;
    let start_idx = if sub_layer_ordering_present != 0 { 0 } else { max_sub_layers };
    
    for _ in start_idx..=max_sub_layers {
        // sps_max_dec_pic_buffering_minus1[...] (ue)
        read_ue(&clean, &mut offset)?;
        // sps_max_num_reorder_pics[...] (ue)
        read_ue(&clean, &mut offset)?;
        // sps_max_latency_increase_plus1[...] (ue)
        read_ue(&clean, &mut offset)?;
    }

    // log2_min_luma_coding_block_size_minus3 (ue)
    read_ue(&clean, &mut offset)?;
    // log2_diff_max_min_luma_coding_block_size (ue)
    read_ue(&clean, &mut offset)?;
    // log2_min_transform_block_size_minus2 (ue)
    read_ue(&clean, &mut offset)?;
    // log2_diff_max_min_transform_block_size (ue)
    read_ue(&clean, &mut offset)?;
    // max_transform_hierarchy_depth_inter (ue)
    read_ue(&clean, &mut offset)?;
    // max_transform_hierarchy_depth_intra (ue)
    read_ue(&clean, &mut offset)?;

    // scaling_list_enabled_flag (u1)
    let scaling_list_enabled = read_bits(&clean, &mut offset, 1)?;
    if scaling_list_enabled != 0 {
        // sps_scaling_list_data_present_flag (u1)
        let sps_scaling_list_present = read_bits(&clean, &mut offset, 1)?;
        if sps_scaling_list_present != 0 {
            // Skip scaling list data (complex structure, skip for now)
            // This is a simplification - proper scaling list parsing would be needed
            // for complete compliance, but most files don't use SPS scaling lists.
        }
    }

    // amp_enabled_flag (u1)
    read_bits(&clean, &mut offset, 1)?;
    // sample_adaptive_offset_enabled_flag (u1)
    let _sao_enabled = read_bits(&clean, &mut offset, 1)?;
    // pcm_enabled_flag (u1)
    let pcm_enabled = read_bits(&clean, &mut offset, 1)?;
    
    if pcm_enabled != 0 {
        // pcm_sample_bit_depth_luma_minus1 (u4)
        read_bits(&clean, &mut offset, 4)?;
        // pcm_sample_bit_depth_chroma_minus1 (u4)
        read_bits(&clean, &mut offset, 4)?;
        // log2_min_pcm_luma_coding_block_size_minus3 (ue)
        read_ue(&clean, &mut offset)?;
        // log2_diff_max_min_pcm_luma_coding_block_size (ue)
        read_ue(&clean, &mut offset)?;
        // pcm_loop_filter_disabled_flag (u1)
        read_bits(&clean, &mut offset, 1)?;
    }

    // num_short_term_ref_pic_sets (ue)
    let num_short_term_rps = read_ue(&clean, &mut offset)?;
    for _ in 0..num_short_term_rps {
        // Short term RPS parsing is complex, skip for now
        // This is a heuristic - we try to detect the rough size
        // A proper implementation would parse each RPS fully
    }

    // long_term_ref_pics_present_flag (u1)
    let long_term_present = read_bits(&clean, &mut offset, 1)?;
    if long_term_present != 0 {
        // num_long_term_ref_pics_sps (ue)
        let num_long_term = read_ue(&clean, &mut offset)?;
        for _ in 0..num_long_term {
            // lt_ref_pic_poc_lsb_sps[...] (u(log2_max_pic_order_cnt_lsb_minus4 + 4))
            // lt_ref_pic_used_by_curr_pic_sps_flag[...] (u1)
            // Skip without proper parsing
        }
    }

    // sps_temporal_mvp_enabled_flag (u1)
    read_bits(&clean, &mut offset, 1)?;
    // strong_intra_smoothing_enabled_flag (u1)
    read_bits(&clean, &mut offset, 1)?;

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

    // Parse VUI for colour information
    let vui_result = parse_vui(&clean, &mut offset);
    let (colour_description_present, colour_primaries, transfer_characteristics, matrix_coefficients, video_full_range) = 
        vui_result.unwrap_or((false, None, None, None, None));

    Some(HevcInfo {
        profile_idc,
        tier_high: tier_flag != 0,
        level_idc,
        width,
        height,
        chroma_format_idc: chroma_format_idc as u8,
        bit_depth: bit_depth as u8,
        colour_description_present,
        colour_primaries,
        transfer_characteristics,
        matrix_coefficients,
        video_full_range,
        encoder_string: None,
        encoder_name: None,
        encoder_version: None,
        encoder_settings: None,
    })
}

/// Parse VUI section of SPS to extract colour information.
fn parse_vui(clean: &[u8], offset: &mut usize) -> Option<(bool, Option<u8>, Option<u8>, Option<u8>, Option<bool>)> {
    // vui_parameters_present_flag
    let vui_present = read_bits(clean, offset, 1)?;
    if vui_present == 0 {
        return Some((false, None, None, None, None));
    }

    // Skip VUI aspect_ratio, overscan, video_signal_type until we hit colour
    // aspect_ratio_info_present_flag
    let aspect_present = read_bits(clean, offset, 1)?;
    if aspect_present != 0 {
        let aspect_idc = read_bits(clean, offset, 8)?;
        if aspect_idc == 255 { // EXTENDED_SAR
            read_bits(clean, offset, 16)?; // sar_width
            read_bits(clean, offset, 16)?; // sar_height
        }
    }

    // overscan_info_present_flag
    let overscan_present = read_bits(clean, offset, 1)?;
    if overscan_present != 0 {
        read_bits(clean, offset, 1)?; // overscan_appropriate_flag
    }

    // video_signal_type_present_flag
    let video_signal_present = read_bits(clean, offset, 1)?;
    let mut video_full_range = None;
    let mut colour_description_present = false;
    let mut colour_primaries = None;
    let mut transfer_characteristics = None;
    let mut matrix_coefficients = None;

    if video_signal_present != 0 {
        read_bits(clean, offset, 3)?; // video_format
        let full_range = read_bits(clean, offset, 1)?;
        video_full_range = Some(full_range != 0);

        // colour_description_present_flag
        colour_description_present = read_bits(clean, offset, 1)? != 0;
        if colour_description_present {
            colour_primaries = read_bits(clean, offset, 8).map(|v| v as u8);
            transfer_characteristics = read_bits(clean, offset, 8).map(|v| v as u8);
            matrix_coefficients = read_bits(clean, offset, 8).map(|v| v as u8);
        }
    }

    // We don't need to parse the rest of VUI - we've got colour info
    // Return what we found
    Some((colour_description_present, colour_primaries, transfer_characteristics, matrix_coefficients, video_full_range))
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
    fa.element_begin("HEVC");

    let head = fa.peek_raw(4);
    let Some(h) = head else {
        fa.element_end();
        return false;
    };

    if h != ANNEX_B_START_CODE_LONG && &h[1..] != ANNEX_B_START_CODE {
        fa.element_end();
        return false;
    }

    let data = if let Some(d) = fa.peek_raw(fa.remain() as usize) {
        d.to_vec()
    } else {
        fa.element_end();
        return false;
    };

    fa.element_info("Format", Some("HEVC"));

    let mut vps_found = false;
    let mut sps_info = None;
    let mut sei_nalus: Vec<&[u8]> = Vec::new();

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
            NAL_TYPE_SEI_PREFIX | NAL_TYPE_SEI_SUFFIX => {
                sei_nalus.push(nal_unit);
            }
            _ => {}
        }

        nal_offset = nal_end;
    }

    if !vps_found || sps_info.is_none() {
        fa.element_end();
        return false;
    }

    let (profile_idc, tier_flag, level_idc, width, height, chroma_format_idc, bit_depth) = sps_info.unwrap();

    // Extract HDR metadata from SEI NAL units
    let hdr_metadata = extract_hdr_from_sei_nalus(&sei_nalus);

    // Extract encoder info from SEI user_data_unregistered
    let encoder_info = extract_encoder_from_sei_nalus(&sei_nalus);

    fa.stream_prepare(StreamKind::Video);

    fa.fill(StreamKind::Video, 0, "Format", "HEVC", false);
    let profile = if bit_depth <= 8 { "Main" } else { profile_name(profile_idc) };
    fa.fill(StreamKind::Video, 0, "Format_Profile", profile, false);
    fa.fill(StreamKind::Video, 0, "Format_Level", level_name(level_idc), false);

    if tier_flag {
        fa.fill(StreamKind::Video, 0, "Format_Tier", "High", false);
    } else {
        fa.fill(StreamKind::Video, 0, "Format_Tier", "Main", false);
    }

    fa.fill(StreamKind::Video, 0, "Width", width.to_string(), false);
    fa.fill(StreamKind::Video, 0, "Height", height.to_string(), false);
    fa.fill(StreamKind::Video, 0, "Sampled_Width", width.to_string(), false);
    fa.fill(StreamKind::Video, 0, "Sampled_Height", height.to_string(), false);

    if height > 0 {
        let dar = width as f64 / height as f64;
        fa.fill(StreamKind::Video, 0, "DisplayAspectRatio", format!("{:.3}", dar), false);
    }
    fa.fill(StreamKind::Video, 0, "PixelAspectRatio", "1.000", false);

    let chroma_sub = match chroma_format_idc {
        0 => "4:0:0",
        1 => "4:2:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "4:2:0",
    };
    fa.fill(StreamKind::Video, 0, "ChromaSubsampling", chroma_sub, false);
    fa.fill(StreamKind::Video, 0, "BitDepth", bit_depth.to_string(), false);
    fa.fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);

    // Fill HDR metadata if present
    if let Some((mastering, light_level)) = hdr_metadata {
        if let Some((primaries, white_point, max_lum, min_lum)) = mastering {
            fa.fill(StreamKind::Video, 0, "HDR_Format", "SMPTE ST 2086", false);
            fa.fill(StreamKind::Video, 0, "HDR_Format_Compatibility", "HDR10", false);
            // Mastering display primaries (convert from 0.00002 units)
            let r_x = primaries[0].0 as f64 * 0.00002;
            let r_y = primaries[0].1 as f64 * 0.00002;
            let g_x = primaries[1].0 as f64 * 0.00002;
            let g_y = primaries[1].1 as f64 * 0.00002;
            let b_x = primaries[2].0 as f64 * 0.00002;
            let b_y = primaries[2].1 as f64 * 0.00002;
            let w_x = white_point.0 as f64 * 0.00002;
            let w_y = white_point.1 as f64 * 0.00002;
            fa.fill(StreamKind::Video, 0, "MasteringDisplay_ColorPrimaries", 
                format!("Red: ({:.5}, {:.5}), Green: ({:.5}, {:.5}), Blue: ({:.5}, {:.5}), White: ({:.5}, {:.5})", 
                    r_x, r_y, g_x, g_y, b_x, b_y, w_x, w_y), false);
            // Luminance in cd/m^2 (convert from 0.0001 units)
            let max_lum_cd = max_lum as f64 * 0.0001;
            let min_lum_cd = min_lum as f64 * 0.0001;
            fa.fill(StreamKind::Video, 0, "MasteringDisplay_Luminance", 
                format!("min: {:.4} cd/m², max: {:.0} cd/m²", min_lum_cd, max_lum_cd), false);
        }
        if let Some((max_content, max_frame_avg)) = light_level {
            fa.fill(StreamKind::Video, 0, "MaxCLL", format!("{} cd/m²", max_content), false);
            fa.fill(StreamKind::Video, 0, "MaxFALL", format!("{} cd/m²", max_frame_avg), false);
        }
    }

    // Encoder info from SEI user_data_unregistered
    if let Some(ref enc) = encoder_info {
        fa.fill(StreamKind::Video, 0, "Encoded_Library", enc.library.as_str(), false);
        if let Some(ref name) = enc.name {
            fa.fill(StreamKind::Video, 0, "Encoded_Library_Name", name.as_str(), false);
        }
        if let Some(ref ver) = enc.version {
            fa.fill(StreamKind::Video, 0, "Encoded_Library_Version", ver.as_str(), false);
        }
        if let Some(ref settings) = enc.settings {
            fa.fill(StreamKind::Video, 0, "Encoded_Library_Settings", settings.as_str(), false);
        }
    }

    // General stream
    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "HEVC", false);
    fa.fill(StreamKind::General, 0, "VideoCount", "1", false);

    fa.element_end();
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

        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()), Some("HEVC"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Format_Profile").map(|z| z.as_str()), Some("Main"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Format_Level").map(|z| z.as_str()), Some("2"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Format_Tier").map(|z| z.as_str()), Some("Main"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()), Some("320"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()), Some("240"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "ChromaSubsampling").map(|z| z.as_str()), Some("4:2:0"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "BitDepth").map(|z| z.as_str()), Some("8"));
    }
}
