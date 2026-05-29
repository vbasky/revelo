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

    /// Build a Dolby Vision RPU NAL unit with the given extension data.
    /// NAL header: first byte = (0x3F << 1) | (nal_type >> 0... actually 0x7C = NAL type 62
    fn make_dv_rpu(extensions: &[(u8, &[u8])]) -> Vec<u8> {
        // NAL header: 2 bytes for HEVC
        // NAL type 62: first byte = (62 << 1) = 0x7C, second byte = 0x01 (temporal_id+1)
        let mut nal = vec![0x7Cu8, 0x01];

        // RPU prefix: 6 bytes
        nal.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

        // num_extensions_minus1
        nal.push((extensions.len().saturating_sub(1)) as u8);

        for &(ext_type, ext_data) in extensions {
            // extension_size in LEB128 (assuming < 128)
            nal.push(ext_data.len() as u8);
            nal.push(ext_type);
            nal.extend_from_slice(ext_data);
        }

        // Pad to meet the minimum length requirement (10 bytes)
        nal.resize(nal.len().max(10), 0);
        nal
    }

    /// Build L1 metadata payload (8 bytes).
    fn make_l1_data(color_primaries: u8, max_lum: u32, min_lum: u32) -> Vec<u8> {
        let l1_byte = color_primaries << 5; // top 3 bits = color_primaries
        let mut data = vec![l1_byte];
        data.extend_from_slice(&max_lum.to_be_bytes());
        data.extend_from_slice(&min_lum.to_be_bytes());
        data
    }

    #[test]
    fn rejects_non_dv_nal() {
        let nal = vec![0x00; 10];
        assert!(parse_dv_rpu(&nal).is_none());
    }

    #[test]
    fn rejects_short_buffer() {
        let nal = vec![0x7C; 5];
        assert!(parse_dv_rpu(&nal).is_none());
    }

    #[test]
    fn accepts_valid_rpu_no_extensions() {
        // "no L1-L8 extensions" = a single dummy extension block with type=255 (reserved), size=0
        let nal = make_dv_rpu(&[(255, &[])]);
        let info = parse_dv_rpu(&nal);
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(info.has_rpu);
        assert!(!info.l1_metadata);
        assert!(!info.reshaping_present);
    }

    #[test]
    fn parses_l1_extension() {
        let l1 = make_l1_data(0, 10000000, 50); // 1000 cd/m² max, 0.005 cd/m² min
        let nal = make_dv_rpu(&[(0, &l1)]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert!(info.l1_metadata);
        assert!(!info.l2_metadata);
        assert!(!info.l3_metadata);
        assert!(!info.l4_metadata);
    }

    #[test]
    fn parses_l1_luminance_values() {
        let l1 = make_l1_data(0, 10_000_000u32, 100); // max=1000.0, min=0.01
        let nal = make_dv_rpu(&[(0, &l1)]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert!(info.max_luminance.is_some());
        assert!(info.min_luminance.is_some());
        assert!((info.max_luminance.unwrap() - 1000.0).abs() < 0.1);
        assert!((info.min_luminance.unwrap() - 0.01).abs() < 0.001);
    }

    #[test]
    fn parses_l1_color_space_bt2020() {
        let l1 = make_l1_data(0, 10000000, 50);
        let nal = make_dv_rpu(&[(0, &l1)]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert_eq!(info.color_space.as_deref(), Some("BT.2020"));
    }

    #[test]
    fn parses_l1_color_space_p3_d65() {
        let l1 = make_l1_data(1, 10000000, 50);
        let nal = make_dv_rpu(&[(0, &l1)]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert_eq!(info.color_space.as_deref(), Some("P3 D65"));
    }

    #[test]
    fn parses_l1_color_space_bt709() {
        let l1 = make_l1_data(2, 10000000, 50);
        let nal = make_dv_rpu(&[(0, &l1)]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert_eq!(info.color_space.as_deref(), Some("BT.709"));
    }

    #[test]
    fn parses_l5_reshaping_extension() {
        // L5 extension (type 4) with non-empty data → reshaping_present
        let l5_data = [0x01, 0x02, 0x03];
        let nal = make_dv_rpu(&[(4, &l5_data)]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert!(info.l5_metadata);
        assert!(info.reshaping_present);
    }

    #[test]
    fn parses_l1_and_l5_together() {
        let l1 = make_l1_data(1, 10000000, 50);
        let l5 = [0x01, 0x02, 0x03];
        let nal = make_dv_rpu(&[(0, &l1), (4, &l5)]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert!(info.l1_metadata);
        assert!(info.l5_metadata);
        assert!(info.reshaping_present);
        assert_eq!(info.color_space.as_deref(), Some("P3 D65"));
    }

    #[test]
    fn parses_l2_l3_l4_extensions() {
        let nal = make_dv_rpu(&[(1, &[0u8; 4]), (2, &[0u8; 4]), (3, &[0u8; 4])]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert!(info.l2_metadata);
        assert!(info.l3_metadata);
        assert!(info.l4_metadata);
        assert!(!info.l1_metadata);
    }

    #[test]
    fn parses_vdr_dm_data_ext_type_8() {
        let nal = make_dv_rpu(&[(8, &[0u8; 4])]);
        let info = parse_dv_rpu(&nal).unwrap();
        assert!(info.l5_metadata); // VDR DM data also sets l5_metadata
    }

    #[test]
    fn fill_dv_rpu_fields_sets_correctly() {
        let l1 = make_l1_data(0, 10000000, 50);
        let nal = make_dv_rpu(&[(0, &l1)]);
        let info = parse_dv_rpu(&nal).unwrap();

        let mut fa = FileAnalyze::new(&[0u8; 10]);
        fa.stream_prepare(StreamKind::Video);
        fill_dv_rpu_fields(&mut fa, 0, &info);

        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(v("HDR_Format_Version").as_deref(), Some("RPU L1"));
        assert_eq!(v("colour_primaries").as_deref(), Some("BT.2020"));
        assert!(v("MasteringDisplay_Luminance").unwrap_or_default().contains("cd/m²"));
    }
}
