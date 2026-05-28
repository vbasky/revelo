//! AV1 (AOMedia Video 1) parser.
//!
//! Parses AV1 OBU (Open Bitstream Unit) sequence headers to extract
//! profile, level, bit depth, chroma subsampling, and colour info.

use revelio_core::{FileAnalyze, StreamKind};

const OBU_SEQUENCE_HEADER: u8 = 1;
const OBU_TEMPORAL_DELIMITER: u8 = 2;
const OBU_FRAME_HEADER: u8 = 3;
const OBU_TILE_GROUP: u8 = 4;
const OBU_METADATA: u8 = 5;
const OBU_FRAME: u8 = 6;
const OBU_REDUNDANT_FRAME_HEADER: u8 = 7;
const OBU_TILE_LIST: u8 = 8;
const OBU_PADDING: u8 = 15;

/// AV1 sequence header info extracted from OBU.
#[derive(Debug)]
pub struct Av1Info {
    pub profile: u8,
    pub level: u8,
    pub tier: u8,  // 0 = Main, 1 = High
    pub bit_depth: u8,
    pub chroma_subsampling: &'static str,
    pub monochrome: bool,
    pub colour_description_present: bool,
    pub colour_primaries: Option<u8>,
    pub transfer_characteristics: Option<u8>,
    pub matrix_coefficients: Option<u8>,
    pub video_full_range: Option<bool>,
    pub width: u32,
    pub height: u32,
}

/// Parse AV1 OBU header and return (obu_type, obu_extension_flag, obu_has_size_field, obu_size)
fn parse_obu_header(data: &[u8]) -> Option<(u8, bool, bool, usize)> {
    if data.is_empty() {
        return None;
    }
    
    let byte = data[0];
    let obu_forbidden_bit = (byte >> 7) & 1;
    if obu_forbidden_bit != 0 {
        return None; // Invalid OBU
    }
    
    let obu_type = (byte >> 3) & 0xF;
    let obu_extension_flag = ((byte >> 2) & 1) != 0;
    let obu_has_size_field = ((byte >> 1) & 1) != 0;
    // obu_reserved_1bit "shall be equal to 0" (AV1 §5.3.2). Enforcing it
    // rejects byte patterns that merely happen to look like an OBU header,
    // e.g. AC-3's 0x0B77 sync word (0x0B decodes as a seq-header OBU but
    // with the reserved bit set).
    if byte & 1 != 0 {
        return None;
    }
    
    let mut pos = 1usize;
    
    if obu_extension_flag {
        // Skip temporal_id and spatial_id
        if pos >= data.len() {
            return None;
        }
        pos += 1;
    }
    
    let obu_size = if obu_has_size_field {
        // LEB128 encoded size
        let mut size: usize = 0;
        let mut shift = 0;
        loop {
            if pos >= data.len() || shift > 56 {
                return None;
            }
            let b = data[pos];
            pos += 1;
            size |= ((b & 0x7F) as usize) << shift;
            if (b & 0x80) == 0 {
                break;
            }
            shift += 7;
        }
        size
    } else {
        // Size is rest of data (until next OBU or EOF)
        data.len() - pos
    };
    
    Some((obu_type, obu_extension_flag, obu_has_size_field, obu_size))
}

fn read_bits_leb128(data: &[u8], offset: &mut usize, n: usize) -> Option<u64> {
    if n > 64 || *offset + n > data.len() * 8 {
        return None;
    }
    let mut value = 0u64;
    for i in 0..n {
        let byte_idx = (*offset + i) / 8;
        let bit_idx = (*offset + i) % 8;
        if (data[byte_idx] >> bit_idx) & 1 != 0 {
            value |= 1 << i;
        }
    }
    *offset += n;
    Some(value)
}

fn read_ue(data: &[u8], offset: &mut usize) -> Option<u64> {
    let mut leading_zeros = 0usize;
    // Count leading zeros
    loop {
        if *offset >= data.len() * 8 {
            return None;
        }
        let byte_idx = *offset / 8;
        let bit_idx = *offset % 8;
        if (data[byte_idx] >> bit_idx) & 1 != 0 {
            break;
        }
        leading_zeros += 1;
        *offset += 1;
        if leading_zeros >= 32 {
            return None;
        }
    }
    // Skip the leading 1
    *offset += 1;
    
    // Read 'leading_zeros' bits
    let mut value = 0u64;
    for _ in 0..leading_zeros {
        if *offset >= data.len() * 8 {
            return None;
        }
        let byte_idx = *offset / 8;
        let bit_idx = *offset % 8;
        value = (value << 1) | (((data[byte_idx] >> bit_idx) & 1) as u64);
        *offset += 1;
    }
    
    Some(value + (1u64 << leading_zeros) - 1)
}

fn read_bits(data: &[u8], offset: &mut usize, n: usize) -> Option<u64> {
    if n == 0 {
        return Some(0);
    }
    if *offset + n > data.len() * 8 {
        return None;
    }
    
    let mut value = 0u64;
    for i in 0..n {
        let byte_idx = (*offset + i) / 8;
        let bit_idx = 7 - ((*offset + i) % 8); // MSB first
        value = (value << 1) | (((data[byte_idx] >> bit_idx) & 1) as u64);
    }
    *offset += n;
    Some(value)
}

/// Parse AV1 sequence header OBU payload and extract info.
pub fn parse_av1_sequence_header(data: &[u8]) -> Option<Av1Info> {
    if data.len() < 4 {
        return None;
    }
    
    let mut offset = 0usize;
    
    // seq_profile (3 bits)
    let profile = read_bits(data, &mut offset, 3)? as u8;
    // still_picture (1 bit)
    read_bits(data, &mut offset, 1)?;
    // reduced_still_picture_header (1 bit)
    let reduced_header = read_bits(data, &mut offset, 1)?;
    
    if reduced_header != 0 {
        // Reduced header - simplified parsing
        // timing_info_present_flag is 0
        // decoder_model_info_present_flag is 0
        // initial_display_delay_present_flag is 0
        // operating_points_cnt_minus_1 is 0
        // operating_point_idc[0] is 0
        // seq_level_idx[0] (5 bits)
        let level = read_bits(data, &mut offset, 5)? as u8;
        // seq_tier[0] is implied 0 (Main tier)
        let tier = 0u8;
        
        // frame_width_bits_minus_1 (4 bits)
        let width_bits = read_bits(data, &mut offset, 4)? + 1;
        // frame_height_bits_minus_1 (4 bits)
        let height_bits = read_bits(data, &mut offset, 4)? + 1;
        // max_frame_width_minus_1 (n+1 bits)
        let width = read_bits(data, &mut offset, width_bits as usize)? as u32 + 1;
        // max_frame_height_minus_1 (n+1 bits)
        let height = read_bits(data, &mut offset, height_bits as usize)? as u32 + 1;
        
        // frame_id_numbers_present_flag is 0
        // use_128x128_superblock (1 bit)
        read_bits(data, &mut offset, 1)?;
        // enable_filter_intra (1 bit)
        read_bits(data, &mut offset, 1)?;
        // enable_intra_edge_filter (1 bit)
        read_bits(data, &mut offset, 1)?;
        
        return Some(Av1Info {
            profile,
            level,
            tier,
            bit_depth: 8,
            chroma_subsampling: "4:2:0",
            monochrome: false,
            colour_description_present: false,
            colour_primaries: None,
            transfer_characteristics: None,
            matrix_coefficients: None,
            video_full_range: None,
            width,
            height,
        });
    }
    
    // Full sequence header parsing
    // timing_info_present_flag (1 bit)
    let timing_present = read_bits(data, &mut offset, 1)?;
    
    if timing_present != 0 {
        // Skip timing_info (configurable in full parser)
        // For now, we don't need timing info for MediaInfo fields
        // Skip: num_units_in_display_tick, time_scale, equal_picture_interval, num_ticks_per_picture_minus_1
        return None; // Complex path not fully implemented yet
    }
    
    // decoder_model_info_present_flag is implied 0 if timing not present
    // initial_display_delay_present_flag (1 bit)
    let initial_display_delay_present = read_bits(data, &mut offset, 1)?;
    if initial_display_delay_present != 0 {
        // initial_display_delay_minus_1 (4 bits)
        read_bits(data, &mut offset, 4)?;
    }
    
    // operating_points_cnt_minus_1 (5 bits)
    let op_cnt = read_bits(data, &mut offset, 5)?;
    for _ in 0..=op_cnt {
        // operating_point_idc[i] (12 bits)
        read_bits(data, &mut offset, 12)?;
        // seq_level_idx[i] (5 bits)
        let _seq_level = read_bits(data, &mut offset, 5)?;
        // if seq_level_idx[i] > 7: seq_tier[i] (1 bit)
        // Simplified: assume level <= 7
    }
    
    // Skip to frame size and other info
    // This is a simplified implementation
    // frame_width_bits_minus_1 (4 bits)
    let width_bits = read_bits(data, &mut offset, 4)? + 1;
    // frame_height_bits_minus_1 (4 bits)
    let height_bits = read_bits(data, &mut offset, 4)? + 1;
    // max_frame_width_minus_1 (n+1 bits)
    let width = read_bits(data, &mut offset, width_bits as usize)? as u32 + 1;
    // max_frame_height_minus_1 (n+1 bits)
    let height = read_bits(data, &mut offset, height_bits as usize)? as u32 + 1;
    
    // frame_id_numbers_present_flag (1 bit)
    let frame_id_present = read_bits(data, &mut offset, 1)?;
    if frame_id_present != 0 {
        // delta_frame_id_length_minus_2 (4 bits)
        read_bits(data, &mut offset, 4)?;
        // additional_frame_id_length_minus_1 (1 bit)
        read_bits(data, &mut offset, 1)?;
    }
    
    // use_128x128_superblock (1 bit)
    read_bits(data, &mut offset, 1)?;
    // enable_filter_intra (1 bit)
    read_bits(data, &mut offset, 1)?;
    // enable_intra_edge_filter (1 bit)
    read_bits(data, &mut offset, 1)?;
    
    // Additional mode support - simplified
    // enable_interintra_compound, enable_masked_compound, etc.
    
    // Derive bit depth from profile
    let bit_depth = match profile {
        0 => 8,  // Main: 8-bit
        1 => 10, // High: 10-bit
        2 => 12, // Professional: 12-bit
        _ => 8,
    };

    // Derive chroma subsampling from profile
    let chroma = if profile <= 1 { "4:2:0" } else { "4:2:2" };

    // Level from operating point (default 5.1 = level 13)
    let level = 13;

    Some(Av1Info {
        profile,
        level,
        tier: 0,
        bit_depth,
        chroma_subsampling: chroma,
        monochrome: false,
        colour_description_present: false,
        colour_primaries: None,
        transfer_characteristics: None,
        matrix_coefficients: None,
        video_full_range: None,
        width,
        height,
    })
}

/// Parse AV1 from raw OBU stream (Annex B or low-overhead format).
pub fn parse_av1(fa: &mut FileAnalyze) -> bool {
    fa.Element_Begin("AV1");
    
    let data = if let Some(d) = fa.peek_raw(fa.Remain() as usize) {
        d.to_vec()
    } else {
        fa.Element_End();
        return false;
    };
    
    if data.len() < 2 {
        fa.Element_End();
        return false;
    }

    // Anchor recognition at the stream start: a low-overhead AV1 bitstream
    // begins with a temporal delimiter (then a sequence header), and a raw
    // sequence-header stream begins with the sequence header itself. If the
    // very first OBU is neither — or isn't a valid OBU at all — this isn't
    // AV1. Without this anchor the scan below would hunt arbitrary offsets
    // and false-positive on unrelated byte streams.
    match parse_obu_header(&data) {
        Some((t, _, _, _)) if t == OBU_TEMPORAL_DELIMITER || t == OBU_SEQUENCE_HEADER => {}
        _ => {
            fa.Element_End();
            return false;
        }
    }

    // Look for sequence header OBU
    let mut pos = 0usize;
    let mut seq_header_info = None;
    
    while pos < data.len() {
        let header_result = parse_obu_header(&data[pos..]);
        let (obu_type, _ext_flag, _has_size, obu_size) = match header_result {
            Some(h) => h,
            None => break,
        };
        
        // Calculate header size
        let header_size = if _has_size {
            // Need to calculate actual bytes consumed by header
            let mut hpos = 1usize;
            if _ext_flag {
                hpos += 1;
            }
            if _has_size {
                // LEB128 size field
                let mut spos = hpos;
                while spos < data.len() - pos && (data[pos + spos] & 0x80) != 0 {
                    spos += 1;
                }
                hpos = spos + 1;
            }
            hpos
        } else {
            1
        };
        
        if obu_type == OBU_SEQUENCE_HEADER {
            let payload_start = pos + header_size;
            if payload_start + obu_size <= data.len() {
                let payload = &data[payload_start..payload_start + obu_size];
                if let Some(info) = parse_av1_sequence_header(payload) {
                    seq_header_info = Some(info);
                    break; // Found sequence header, done
                }
            }
        }
        
        pos += header_size + obu_size;
    }
    
    let info = match seq_header_info {
        Some(i) => i,
        None => {
            fa.Element_End();
            return false;
        }
    };
    
    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "AV1", false);
    fa.Fill(StreamKind::Video, 0, "Width", info.width.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Height", info.height.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "BitDepth", info.bit_depth.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", info.chroma_subsampling, false);
    
    let profile_name = match info.profile {
        0 => "Main",
        1 => "High",
        2 => "Professional",
        _ => "Unknown",
    };
    fa.Fill(StreamKind::Video, 0, "Format_Profile", profile_name, false);
    
    fa.Fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);
    fa.Fill(StreamKind::Video, 0, "ScanType", "Progressive", false);
    
    // General stream
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "AV1", false);
    fa.Fill(StreamKind::General, 0, "VideoCount", "1", false);
    
    fa.Element_End();
    true
}

/// Parse AV1 from AV1CodecConfigurationRecord (used in WebM/MKV).
/// The config record contains the sequence header OBU.
pub fn parse_av1_from_codec_config(config: &[u8]) -> Option<Av1Info> {
    if config.len() < 4 {
        return None;
    }
    
    // AV1CodecConfigurationRecord:
    // marker (1 bit) = 1
    // version (7 bits) = 1
    // seq_profile (3 bits)
    // seq_level_idx_0 (5 bits)
    // seq_tier_0 (1 bit) - only if seq_level_idx_0 > 7
    // high_bitdepth (1 bit)
    // twelve_bit (1 bit) - only if high_bitdepth && seq_profile == 2
    // monochrome (1 bit)
    // chroma_subsampling_x (1 bit)
    // chroma_subsampling_y (1 bit)
    // chroma_sample_position (2 bits)
    // reserved (3 bits)
    // initial_presentation_delay_present (1 bit)
    // initial_presentation_delay_minus_one (4 bits) - if present
    // configOBUs (variable)
    
    let mut offset = 0usize;
    
    // First byte: marker (1) + version (7)
    let byte0 = config[0];
    let marker = (byte0 >> 7) & 1;
    let version = byte0 & 0x7F;
    
    if marker != 1 || version != 1 {
        return None;
    }
    
    // seq_profile (3 bits) from bits 7-5 of byte 1
    let profile = (config[1] >> 5) & 0x7;
    // seq_level_idx_0 (5 bits) from bits 4-0 of byte 1
    let level = config[1] & 0x1F;
    
    // seq_tier_0 if level > 7
    let tier = if level > 7 {
        (config[2] >> 7) & 1
    } else {
        0
    };
    
    // high_bitdepth, twelve_bit, monochrome, chroma_subsampling
    let mut pos = 2usize;
    let high_bitdepth = if level > 7 {
        pos += 1;
        (config[2] >> 6) & 1
    } else {
        (config[2] >> 7) & 1
    };
    
    // For full parsing, we need to extract the sequence header from configOBUs
    // Skip to configOBUs and parse the sequence header OBU
    // Simplified: return basic info from config record
    
    let bit_depth = if high_bitdepth != 0 {
        if profile == 2 && config.len() > pos && ((config[pos] >> 5) & 1) != 0 {
            12
        } else {
            10
        }
    } else {
        8
    };
    
    // Try to find and parse the sequence header OBU from configOBUs
    // The configOBUs start after the fixed header
    let header_size = if level > 7 { 4 } else { 3 };
    if config.len() > header_size {
        let obus = &config[header_size..];
        // Look for sequence header OBU (type 1)
        if let Some((obu_type, _, _, obu_size)) = parse_obu_header(obus) {
            if obu_type == OBU_SEQUENCE_HEADER {
                let header_len = if obus.len() > 1 && ((obus[1] >> 1) & 1) != 0 {
                    // Has size field - calculate header length
                    let mut hlen = 1;
                    if ((obus[0] >> 2) & 1) != 0 {
                        hlen += 1; // extension flag
                    }
                    // Skip LEB128 size
                    let mut spos = hlen;
                    while spos < obus.len() && (obus[spos] & 0x80) != 0 {
                        spos += 1;
                    }
                    hlen = spos + 1;
                    hlen
                } else {
                    1
                };
                
                if header_len + obu_size <= obus.len() {
                    let payload = &obus[header_len..header_len + obu_size];
                    if let Some(info) = parse_av1_sequence_header(payload) {
                        return Some(info);
                    }
                }
            }
        }
    }
    
    // Fallback: return partial info from config record
    Some(Av1Info {
        profile,
        level: level as u8,
        tier: tier as u8,
        bit_depth: bit_depth as u8,
        chroma_subsampling: "4:2:0",
        monochrome: false,
        colour_description_present: false,
        colour_primaries: None,
        transfer_characteristics: None,
        matrix_coefficients: None,
        video_full_range: None,
        width: 0, // Unknown without parsing sequence header
        height: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_data() {
        let buf = vec![];
        assert!(parse_av1_sequence_header(&buf).is_none());
    }

    #[test]
    fn rejects_short_data() {
        let buf = vec![0x00, 0x00, 0x00];
        assert!(parse_av1_sequence_header(&buf).is_none());
    }
}
