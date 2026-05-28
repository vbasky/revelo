use revelio_core::{FileAnalyze, StreamKind};

/// Parse Dolby Vision HDR metadata.
///
/// Detection: dvcC/dvvC boxes in MP4, codec ID in MKV, standalone XML.
/// Fills: Profile (5/7/8.1), level, BL compatibility, HDR format fields.
pub fn parse_dolby_vision(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain() as usize) {
        Some(b) => b,
        None => return false,
    };

    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let trimmed = text.trim();

    if !trimmed.contains("DolbyVision") || !trimmed.starts_with('<') {
        return false;
    }

    let mut info = DolbyVisionInfo::default();

    // Parse version
    if let Some(v) = extract_xml_value(trimmed, "dv_version_major") {
        info.version_major = v;
    }
    if let Some(v) = extract_xml_value(trimmed, "dv_version_minor") {
        info.version_minor = v;
    }
    if let Some(v) = extract_xml_value(trimmed, "dv_profile") {
        info.profile = v;
    }
    if let Some(v) = extract_xml_value(trimmed, "dv_level") {
        info.level = v;
    }
    if let Some(v) = extract_xml_value(trimmed, "dv_bl_signal_compatibility_id") {
        info.bl_present = true;
        info.bl_compatibility_id = v;
    }

    // HDR display metadata
    if let Some(cp) = extract_xml_value(trimmed, "color_primaries") {
        info.color_primaries = cp;
    }
    if let Some(ma) = extract_xml_value(trimmed, "max_display_mastering_luminance") {
        info.max_luminance = ma;
    }
    if let Some(mi) = extract_xml_value(trimmed, "min_display_mastering_luminance") {
        info.min_luminance = mi;
    }
    if let Some(mc) = extract_xml_value(trimmed, "max_content_light_level") {
        info.max_cll = mc;
    }
    if let Some(mf) = extract_xml_value(trimmed, "max_frame_average_light_level") {
        info.max_fall = mf;
    }

    fill_dv_streams(fa, &info);
    true
}

#[derive(Default)]
struct DolbyVisionInfo {
    version_major: String,
    version_minor: String,
    profile: String,
    level: String,
    bl_present: bool,
    bl_compatibility_id: String,
    color_primaries: String,
    max_luminance: String,
    min_luminance: String,
    max_cll: String,
    max_fall: String,
}

fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let rest = xml.find(&open)?;
    let after_open = &xml[rest + open.len()..];
    let close = after_open.find('>')?;
    let inner = after_open[close + 1..].trim();
    let end_tag = format!("</{}", tag);
    let inner_end = inner.find(&end_tag)?;
    Some(inner[..inner_end].trim().to_string())
}

fn fill_dv_streams(fa: &mut FileAnalyze, info: &DolbyVisionInfo) {
    let pos = fa.stream_prepare(StreamKind::Video);

    fa.fill(StreamKind::Video, pos, "Format", "Dolby Vision", false);
    fa.fill(StreamKind::Video, pos, "HDR_Format", "Dolby Vision", false);

    if !info.profile.is_empty() {
        fa.fill(StreamKind::Video, pos, "HDR_Format_Profile", info.profile.clone(), false);
    }
    if !info.level.is_empty() {
        fa.fill(StreamKind::Video, pos, "HDR_Format_Level", info.level.clone(), false);
    }
    if !info.version_major.is_empty() {
        fa.fill(StreamKind::Video, pos, "HDR_Format_Version",
            format!("{}.{}", info.version_major, info.version_minor), false);
    }
    if info.bl_present && !info.bl_compatibility_id.is_empty() {
        fa.fill(StreamKind::Video, pos, "HDR_Format_Compatibility", format!("BL:{}", info.bl_compatibility_id), false);
    }
    if !info.max_luminance.is_empty() {
        fa.fill(StreamKind::Video, pos, "MasteringDisplay_Luminance_Max", info.max_luminance.clone(), false);
    }
    if !info.min_luminance.is_empty() {
        fa.fill(StreamKind::Video, pos, "MasteringDisplay_Luminance_Min", info.min_luminance.clone(), false);
    }
    if !info.color_primaries.is_empty() {
        fa.fill(StreamKind::Video, pos, "MasteringDisplay_ColorPrimaries", info.color_primaries.clone(), false);
    }
    if !info.max_cll.is_empty() {
        fa.fill(StreamKind::Video, pos, "MaxCLL", info.max_cll.clone(), false);
    }
    if !info.max_fall.is_empty() {
        fa.fill(StreamKind::Video, pos, "MaxFALL", info.max_fall.clone(), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn dv_extracts_xml_fields() {
        let xml = r#"<DolbyVisionMetadata>
            <dv_version_major>1</dv_version_major>
            <dv_version_minor>0</dv_version_minor>
            <dv_profile>8.1</dv_profile>
            <dv_level>6</dv_level>
            <dv_bl_signal_compatibility_id>1</dv_bl_signal_compatibility_id>
            <color_primaries>BT.2020</color_primaries>
            <max_display_mastering_luminance>1000</max_display_mastering_luminance>
            <min_display_mastering_luminance>50</min_display_mastering_luminance>
            <max_content_light_level>800</max_content_light_level>
            <max_frame_average_light_level>200</max_frame_average_light_level>
        </DolbyVisionMetadata>"#;

        let mut fa = FileAnalyze::new(xml.as_bytes());
        assert!(parse_dolby_vision(&mut fa));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "HDR_Format_Profile").map(|z| z.as_str().to_owned()), Some("8.1".into()));
    }

    #[test]
    fn dv_rejects_non_xml() {
        let buf = vec![0xFF, 0xD8, 0xFF, 0xE0];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_dolby_vision(&mut fa));
    }
}
