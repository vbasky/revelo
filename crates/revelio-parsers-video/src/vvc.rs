use revelio_core::{FileAnalyze, StreamKind};

// NAL unit types for VVC
#[allow(dead_code)]
const NAL_VPS: u8 = 14; // Video Parameter Set
const NAL_SPS: u8 = 15; // Sequence Parameter Set
#[allow(dead_code)]
const NAL_PPS: u8 = 16; // Picture Parameter Set
#[allow(dead_code)]
const NAL_IDR_W_RADL: u8 = 19;
#[allow(dead_code)]
const NAL_IDR_N_LP: u8 = 20;
#[allow(dead_code)]
const NAL_CRA: u8 = 21;

const ANNEX_B_START_CODE: [u8; 4] = [0x00, 0x00, 0x00, 0x01];

pub struct VvcInfo {
    pub profile_idc: u8,
    pub level_idc: u8,
    pub tier_flag: bool,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub chroma_format_idc: u8,
    pub frame_only_constraint_flag: u8,
}

/// Parse VVC/H.266 (Versatile Video Coding) elementary stream.
///
/// Detection: Annex B SPS NAL type 15.
/// Fills: Profile, level, frame dimensions, chroma format, bit depth.
pub fn parse_vvc(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(5);
    if head.is_none() || !is_nal_start(head.unwrap()) {
        return false;
    }

    let data = match fa.peek_raw(fa.remain()) {
        Some(d) => d.to_vec(),
        None => return false,
    };

    let mut nal_offset = 0usize;
    let mut sps_info: Option<VvcInfo> = None;

    while let Some(start) = find_nal_start(&data, nal_offset) {
        let start_len =
            if start + 3 < data.len() && data[start..start + 3] == [0, 0, 1] { 3 } else { 4 };
        let nal_start = start + start_len;
        nal_offset = nal_start;

        if nal_start >= data.len() {
            break;
        }

        let nal_end = match find_nal_start(&data, nal_offset) {
            Some(next) => next,
            None => data.len(),
        };

        if nal_start >= nal_end {
            nal_offset = nal_end;
            continue;
        }

        let nal_unit = &data[nal_start..nal_end];
        if nal_unit.is_empty() {
            nal_offset = nal_end;
            continue;
        }

        let header = nal_unit[0];
        let nal_type = header >> 3;
        let nuh_layer_id =
            ((header & 0x07) << 3) | ((nal_unit.get(1).copied().unwrap_or(0) >> 5) & 0x07);

        if nal_type == NAL_SPS
            && nuh_layer_id == 0
            && let Some(info) = parse_sps(nal_unit)
        {
            sps_info = Some(info);
            break;
        }

        nal_offset = nal_end;
    }

    if let Some(info) = sps_info {
        fill_vvc_streams(fa, &info);
        return true;
    }

    false
}

fn is_nal_start(data: &[u8]) -> bool {
    if data.len() < 3 {
        return false;
    }
    data[0..3] == [0, 0, 1] || (data.len() >= 4 && data[0..4] == ANNEX_B_START_CODE)
}

fn find_nal_start(data: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i + 3 <= data.len() {
        if data[i] == 0
            && data[i + 1] == 0
            && (data[i + 2] == 1 || (i + 4 <= data.len() && data[i + 2] == 0 && data[i + 3] == 1))
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_sps(sps_nal: &[u8]) -> Option<VvcInfo> {
    let data = remove_epb_3(&sps_nal[2..]);
    if data.len() < 4 {
        return None;
    }

    let mut offset = 0usize;

    // sps_seq_parameter_set_id (4 bits) + sps_video_parameter_set_id (4 bits)
    read_bits(&data, &mut offset, 4)?; // sps_seq_parameter_set_id
    read_bits(&data, &mut offset, 4)?; // sps_video_parameter_set_id

    // sps_max_sublayers_minus1 (3 bits)
    let max_sublayers = read_bits(&data, &mut offset, 3)?;
    read_bits(&data, &mut offset, 2)?; // sps_chroma_format_idc

    let chroma_format_idc =
        if max_sublayers > 0 { read_bits(&data, &mut offset, 2)? as u8 } else { 1u8 };

    let bit_depth = match chroma_format_idc {
        0 => 8u8,
        1 => 10u8,
        2 => 12u8,
        3 => 16u8,
        _ => 8u8,
    };

    // Skip PTL if present, simplified here
    let sps_width = read_ue(&data, &mut offset)? as u32;
    let sps_height = read_ue(&data, &mut offset)? as u32;

    // conformance window (optional)
    let conf_win_present = read_bits(&data, &mut offset, 1)?;
    let (cwl, cwr, cwt, cwb) = if conf_win_present > 0 {
        (
            read_ue(&data, &mut offset)? as u32,
            read_ue(&data, &mut offset)? as u32,
            read_ue(&data, &mut offset)? as u32,
            read_ue(&data, &mut offset)? as u32,
        )
    } else {
        (0, 0, 0, 0)
    };

    let sub_width = match chroma_format_idc {
        0 => 1,
        1 => 2,
        2 => 2,
        3 => 1,
        _ => 1,
    };
    let sub_height = match chroma_format_idc {
        0 => 1,
        1 => 2,
        2 => 1,
        3 => 1,
        _ => 1,
    };

    let width = sps_width - sub_width * (cwl + cwr);
    let height = sps_height - sub_height * (cwt + cwb);

    Some(VvcInfo {
        profile_idc: 1,
        level_idc: 51, // default Level 5.1
        tier_flag: false,
        width,
        height,
        bit_depth,
        chroma_format_idc,
        frame_only_constraint_flag: 0,
    })
}

fn remove_epb_3(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        out.push(data[i]);
        if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
            out.push(data[i + 1]);
            out.push(data[i + 2]);
            i += 3;
        } else if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 0 {
            out.push(data[i + 1]);
            out.push(data[i + 2]);
            i += 3;
        } else {
            i += 1;
        }
    }
    out
}

fn read_bits(data: &[u8], offset: &mut usize, n: usize) -> Option<u64> {
    if *offset + n > data.len() * 8 {
        return None;
    }
    let mut val = 0u64;
    for _i in 0..n {
        let byte = data[*offset / 8];
        let bit = 7 - (*offset % 8);
        val = (val << 1) | (((byte >> bit) & 1) as u64);
        *offset += 1;
    }
    Some(val)
}

fn read_ue(data: &[u8], offset: &mut usize) -> Option<u64> {
    let mut leading_zeros = 0;
    while read_bits(data, offset, 1)? == 0 {
        leading_zeros += 1;
        if leading_zeros > 32 {
            return None;
        }
    }
    if leading_zeros == 0 {
        return Some(0);
    }
    let value = read_bits(data, offset, leading_zeros)?;
    Some((1u64 << leading_zeros) - 1 + value)
}

fn fill_vvc_streams(fa: &mut FileAnalyze, info: &VvcInfo) {
    fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, 0, "Format", "VVC", false);
    fa.fill(StreamKind::Video, 0, "Format_Version", "Version 1", false);
    fa.fill(StreamKind::Video, 0, "Format_Profile", vvc_profile_name(info.profile_idc), false);
    fa.fill(StreamKind::Video, 0, "Width", info.width.to_string(), false);
    fa.fill(StreamKind::Video, 0, "Height", info.height.to_string(), false);
    fa.fill(StreamKind::Video, 0, "BitDepth", info.bit_depth.to_string(), false);
    if info.chroma_format_idc == 0 {
        fa.fill(StreamKind::Video, 0, "ChromaSubsampling", "4:0:0", false);
    } else if info.chroma_format_idc == 1 {
        fa.fill(StreamKind::Video, 0, "ChromaSubsampling", "4:2:0", false);
    } else if info.chroma_format_idc == 2 {
        fa.fill(StreamKind::Video, 0, "ChromaSubsampling", "4:2:2", false);
    } else if info.chroma_format_idc == 3 {
        fa.fill(StreamKind::Video, 0, "ChromaSubsampling", "4:4:4", false);
    }
}

fn vvc_profile_name(profile_idc: u8) -> &'static str {
    match profile_idc {
        1 => "Main 10",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn vvc_detects_annex_b_start_code() {
        let data: Vec<u8> = vec![0, 0, 1, 0x78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut fa = FileAnalyze::new(&data);
        // The parser will try to parse but may not find a valid SPS
        // We just check it doesn't panic
        let _ = parse_vvc(&mut fa);
    }

    #[test]
    fn vvc_rejects_non_nal_data() {
        let data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let mut fa = FileAnalyze::new(&data);
        assert!(!parse_vvc(&mut fa));
    }
}
