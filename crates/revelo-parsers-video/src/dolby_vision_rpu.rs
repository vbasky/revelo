//! Dolby Vision RPU (Reference Processing Unit) parser.
//!
//! RPU NAL unit type 62 in HEVC carries the Dolby Vision enhancement
//! metadata: L1..L8 extension metadata, reshaping curves, and the
//! colour mapping from HDR/SDR base layers.
//!
//! RPU header structure (after NAL header):
//!   rpu_nal_prefix (6 bytes)
//!   num_extensions_minus1 (8 bits)
//!   for each extension:
//!     extension_size (8 bits, variable)
//!     extension_type (8 bits)
//!     extension_data (extension_size bytes)
//!   rpu_data_bytes (variable)

use revelo_core::{FileAnalyze, StreamKind};

/// Dolby Vision RPU metadata fields extracted from NAL type 62.
#[derive(Debug, Default)]
pub struct DolbyVisionRpuInfo {
    pub profile: Option<String>,
    pub level: Option<String>,
    pub bl_compatibility_id: Option<u8>,
    pub has_rpu: bool,
    pub has_el: bool,
    pub l1_metadata: bool,
    pub l2_metadata: bool,
    pub l3_metadata: bool,
    pub l4_metadata: bool,
    pub l5_metadata: bool,
    pub l6_metadata: bool,
    pub l7_metadata: bool,
    pub l8_metadata: bool,
    pub reshaping_present: bool,
    pub color_space: Option<String>,
    pub max_luminance: Option<f64>,
    pub min_luminance: Option<f64>,
}

const DOLBY_VISION_NAL_TYPE: u8 = 62;

/// Read `n` bits big-endian from `data` starting at `bit_off`.
fn get_bits(data: &[u8], bit_off: usize, n: usize) -> Option<u32> {
    if n == 0 {
        return Some(0);
    }
    if bit_off + n > data.len() * 8 {
        return None;
    }
    let mut v = 0u32;
    for i in 0..n {
        let byte = data[(bit_off + i) / 8];
        v = (v << 1) | ((byte >> (7 - ((bit_off + i) % 8))) & 1) as u32;
    }
    Some(v)
}

/// Parse a Dolby Vision RPU NAL unit.
pub fn parse_dv_rpu(nal_unit: &[u8]) -> Option<DolbyVisionRpuInfo> {
    if nal_unit.len() < 10 {
        return None;
    }

    let nal_type = (nal_unit[0] >> 1) & 0x3F;
    if nal_type != DOLBY_VISION_NAL_TYPE {
        return None;
    }

    // Skip NAL header (2 bytes for HEVC)
    let mut off = 16; // bits

    // rpu_nal_prefix (header_data_bytes)
    // 6 bytes of RPU header prefix
    let _rpu_prefix_byte0 = get_bits(nal_unit, off, 8)?;
    off += 8;
    let _rpu_prefix_byte1 = get_bits(nal_unit, off, 8)?;
    off += 8;
    let _rpu_prefix_byte2 = get_bits(nal_unit, off, 8)?;
    off += 8;
    let _rpu_prefix_byte3 = get_bits(nal_unit, off, 8)?;
    off += 8;
    let _rpu_prefix_byte4 = get_bits(nal_unit, off, 8)?;
    off += 8;
    let _rpu_prefix_byte5 = get_bits(nal_unit, off, 8)?;
    off += 8;

    let mut info = DolbyVisionRpuInfo::default();
    info.has_rpu = true;

    // num_extensions_minus1
    let num_ext_minus1 = get_bits(nal_unit, off, 8)?;
    off += 8;

    for _ext in 0..=num_ext_minus1 {
        // extension_size (LEB128-like)
        let mut ext_size = 0u32;
        let mut shift = 0;
        loop {
            let byte = get_bits(nal_unit, off, 8)? as u8;
            off += 8;
            ext_size |= ((byte & 0x7F) as u32) << shift;
            shift += 7;
            if (byte & 0x80) == 0 {
                break;
            }
        }

        let ext_type = get_bits(nal_unit, off, 8)?;
        off += 8;

        match ext_type {
            0..=7 => {
                // L1..L8 extension metadata
                match ext_type {
                    0 => {
                        info.l1_metadata = true;
                    }
                    1 => {
                        info.l2_metadata = true;
                    }
                    2 => {
                        info.l3_metadata = true;
                    }
                    3 => {
                        info.l4_metadata = true;
                    }
                    4 => {
                        info.l5_metadata = true;
                    }
                    5 => {
                        info.l6_metadata = true;
                    }
                    6 => {
                        info.l7_metadata = true;
                    }
                    7 => {
                        info.l8_metadata = true;
                    }
                    _ => {}
                }
                // Parse L1 metadata for luminance
                if ext_type == 0 && ext_size >= 8 {
                    // L1: mastering display metadata
                    let l1_byte = get_bits(nal_unit, off, 8)?;
                    let max_lum_raw = get_bits(nal_unit, off + 8, 32)?;
                    let min_lum_raw = get_bits(nal_unit, off + 40, 32)?;

                    // luminance values are in the format (exponent, mantissa)
                    // Small parsing: store the raw values for now
                    info.max_luminance = Some(max_lum_raw as f64 * 0.0001);
                    info.min_luminance = Some(min_lum_raw as f64 * 0.0001);

                    // Color space from L1 byte bits
                    let color_primaries = (l1_byte >> 5) & 0x07;
                    info.color_space = match color_primaries {
                        0 => Some("BT.2020"),
                        1 => Some("P3 D65"),
                        2 => Some("BT.709"),
                        _ => Some("Unknown"),
                    }
                    .map(|s| s.to_owned());
                }
                // Parse L5 metadata for reshaping
                if ext_type == 4 && ext_size > 0 {
                    info.reshaping_present = true;
                }
            }
            8 => {
                // VDR DM data
                info.l5_metadata = true;
            }
            _ => {}
        }

        off += (ext_size * 8) as usize;
    }

    Some(info)
}

/// Fill Dolby Vision fields from RPU metadata onto existing Video stream.
pub fn fill_dv_rpu_fields(fa: &mut FileAnalyze, pos: usize, rpu: &DolbyVisionRpuInfo) {
    if rpu.l1_metadata {
        fa.set_field(StreamKind::Video, pos, "HDR_Format_Version", "RPU L1");
    }

    if let Some(ref cs) = rpu.color_space {
        fa.set_field(StreamKind::Video, pos, "colour_primaries", cs.as_str());
    }

    if let (Some(max_lum), Some(min_lum)) = (rpu.max_luminance, rpu.min_luminance) {
        fa.set_field(
            StreamKind::Video,
            pos,
            "MasteringDisplay_Luminance",
            format!("min: {:.4} cd/m², max: {:.0} cd/m²", min_lum, max_lum),
        );
    }

    if rpu.reshaping_present {
        fa.set_field(StreamKind::Video, pos, "HDR_Format_Profile", "Reshaping");
    }

    // Summary of extension layers present
    let mut ext_parts = Vec::new();
    if rpu.l1_metadata {
        ext_parts.push("L1");
    }
    if rpu.l2_metadata {
        ext_parts.push("L2");
    }
    if rpu.l3_metadata {
        ext_parts.push("L3");
    }
    if rpu.l4_metadata {
        ext_parts.push("L4");
    }
    if rpu.l5_metadata {
        ext_parts.push("L5");
    }
    if rpu.l6_metadata {
        ext_parts.push("L6");
    }
    if rpu.l7_metadata {
        ext_parts.push("L7");
    }
    if rpu.l8_metadata {
        ext_parts.push("L8");
    }
    if !ext_parts.is_empty() {
        let ext_str = format!("RPU extensions: {}", ext_parts.join(", "));
        fa.set_field(StreamKind::Video, pos, "HDR_Format_Settings", ext_str.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_dv_nal() {
        let nal = vec![0x00; 10]; // NAL type 0, not 62
        assert!(parse_dv_rpu(&nal).is_none());
    }

    #[test]
    fn rejects_short_buffer() {
        let nal = vec![0x7C; 5]; // NAL type 62 but too short
        assert!(parse_dv_rpu(&nal).is_none());
    }
}
