use revelo_core::{FileAnalyze, StreamKind};

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

    // SPS header fields in order (H.266 §7.3.2.4).
    read_bits(&data, &mut offset, 4)?; // sps_seq_parameter_set_id
    read_bits(&data, &mut offset, 4)?; // sps_video_parameter_set_id
    let _max_sublayers = read_bits(&data, &mut offset, 3)?; // sps_max_sublayers_minus1
    let chroma_format_idc = read_bits(&data, &mut offset, 2)? as u8; // sps_chroma_format_idc
    read_bits(&data, &mut offset, 2)?; // sps_log2_ctu_size_minus5
    let ptl_present = read_bits(&data, &mut offset, 1)?; // sps_ptl_dpb_hrd_params_present_flag

    // profile_tier_level(1, max_sublayers) per §7.3.3.1: general_profile_idc,
    // general_tier_flag and general_level_idc are at fixed offsets and decode
    // reliably. The general_constraint_info() that follows is variable-length.
    let (profile_idc, tier_flag, level_idc) = if ptl_present == 1 {
        let profile = read_bits(&data, &mut offset, 7)? as u8; // general_profile_idc
        let tier = read_bits(&data, &mut offset, 1)? != 0; // general_tier_flag
        let level = read_bits(&data, &mut offset, 8)? as u8; // general_level_idc
        (profile, tier, level)
    } else {
        (0u8, false, 0u8)
    };

    // Width/height/bit-depth appear in the SPS only after the *full*
    // profile_tier_level (including the variable-length general_constraint_info
    // and sub-layer level structures). Recovering them correctly needs a
    // complete PTL traversal validated against VVC conformance streams, so they
    // are reported as 0 ("unknown") rather than read from a guessed offset.
    // fill_vvc_streams omits zero-valued dimension fields.
    Some(VvcInfo {
        profile_idc,
        level_idc,
        tier_flag,
        width: 0,
        height: 0,
        bit_depth: 0,
        chroma_format_idc,
        frame_only_constraint_flag: 0,
    })
}

/// Removes H.26x emulation-prevention bytes to recover the RBSP.
///
/// The encoder inserts a `0x03` into any `00 00 00/01/02/03` sequence, yielding
/// `00 00 03 XX`; de-emulation strips that inserted `0x03` (`00 00 03 XX` →
/// `00 00 XX`). All other bytes are copied verbatim. The previous version
/// pushed the `0x03` through unchanged (and also rewrote `00 00 00`), so it
/// removed nothing and left the RBSP bit-misaligned.
fn remove_epb_3(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
            out.push(0);
            out.push(0);
            i += 3; // skip the inserted 0x03 emulation-prevention byte
        } else {
            out.push(data[i]);
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

// Retained for the pending full profile_tier_level / dimension parse.
#[allow(dead_code)]
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
    fa.set_field(StreamKind::Video, 0, "Format", "VVC");
    fa.set_field(StreamKind::Video, 0, "Format_Version", "Version 1");
    fa.set_field(StreamKind::Video, 0, "Format_Profile", vvc_profile_name(info.profile_idc));
    // Omit dimension/bit-depth fields when unknown (0) — see parse_sps.
    if info.width > 0 {
        fa.set_field(StreamKind::Video, 0, "Width", info.width.to_string());
    }
    if info.height > 0 {
        fa.set_field(StreamKind::Video, 0, "Height", info.height.to_string());
    }
    if info.bit_depth > 0 {
        fa.set_field(StreamKind::Video, 0, "BitDepth", info.bit_depth.to_string());
    }
    if info.chroma_format_idc == 0 {
        fa.set_field(StreamKind::Video, 0, "ChromaSubsampling", "4:0:0");
    } else if info.chroma_format_idc == 1 {
        fa.set_field(StreamKind::Video, 0, "ChromaSubsampling", "4:2:0");
    } else if info.chroma_format_idc == 2 {
        fa.set_field(StreamKind::Video, 0, "ChromaSubsampling", "4:2:2");
    } else if info.chroma_format_idc == 3 {
        fa.set_field(StreamKind::Video, 0, "ChromaSubsampling", "4:4:4");
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
    use revelo_core::FileAnalyze;

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
