use revelio_core::{StreamCollection, StreamKind};

const FIELD_COLUMN_WIDTH: usize = 42;

pub fn to_text(streams: &StreamCollection) -> String {
    let mut out = String::new();
    let kinds = [
        StreamKind::General,
        StreamKind::Video,
        StreamKind::Audio,
        StreamKind::Text,
        StreamKind::Other,
        StreamKind::Image,
        StreamKind::Menu,
    ];

    let mut first_kind = true;
    for kind in kinds {
        let count = streams.Count_Get(kind);
        if count == 0 {
            continue;
        }
        if !first_kind {
            out.push('\n');
        }
        first_kind = false;

        for pos in 0..count {
            if count > 1 {
                out.push_str(&format!("{} #{}\n", kind.name(), pos + 1));
            } else {
                out.push_str(kind.name());
                out.push('\n');
            }
            if let Some(stream) = streams.stream(kind, pos) {
                emit_text_stream_fields(&mut out, kind, stream);
            }
        }
    }
    out.push('\n');
    out
}

fn emit_text_stream_fields(
    out: &mut String,
    kind: StreamKind,
    stream: &revelio_core::Stream,
) {
    let canonical = canonical_field_order(kind);
    let extras = extra_field_order(kind);
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for field in canonical {
        if let Some(z) = stream.get(field) {
            emit_text_field(out, field, z.as_str());
            emitted.insert(*field);
        }
    }

    let extras_set: std::collections::HashSet<&'static str> = extras.iter().copied().collect();
    for (k, v) in stream.iter() {
        if !emitted.contains(k) && !extras_set.contains(k) {
            emit_text_field(out, k, v.as_str());
        }
    }

    for field in extras {
        if let Some(z) = stream.get(field) {
            emit_text_field(out, field, z.as_str());
        }
    }
    for (k, v) in stream.extras_iter() {
        emit_text_field(out, k, v.as_str());
    }
}

fn emit_text_field(out: &mut String, name: &str, raw_value: &str) {
    let value = render_field_value(name, raw_value);
    let padding = FIELD_COLUMN_WIDTH.saturating_sub(name.len());
    out.push_str(name);
    for _ in 0..padding {
        out.push(' ');
    }
    out.push_str(" : ");
    out.push_str(&value);
    out.push('\n');
}

fn render_field_value(name: &str, raw: &str) -> String {
    if is_duration_field(name) {
        if let Ok(ms) = raw.parse::<i64>() {
            return format_milliseconds_as_duration(ms);
        }
    }
    raw.to_owned()
}

fn is_duration_field(name: &str) -> bool {
    name == "Duration"
        || name.ends_with("_Duration")
        || name.ends_with("/Duration")
        || name == "Source_Duration_LastFrame"
}

fn format_milliseconds_as_duration(ms: i64) -> String {
    let negative = ms < 0;
    let abs = ms.unsigned_abs();
    let whole = abs / 1000;
    let frac = abs % 1000;
    let body = format!("{whole} s {frac} ms");
    if negative {
        format!("-{body}")
    } else {
        body
    }
}

fn canonical_field_order(kind: StreamKind) -> &'static [&'static str] {
    match kind {
        StreamKind::General => &[
            "Count", "StreamCount", "StreamKind", "StreamKind_String",
            "StreamKindID", "StreamKindPos", "UniqueID",
            "VideoCount", "AudioCount", "TextCount", "OtherCount",
            "ImageCount", "MenuCount",
            "CompleteName", "FolderName", "FileNameExtension", "FileName", "FileExtension",
            "Format", "Format_String", "Format_Info", "Format_Url",
            "Format_Commercial", "Format_Commercial_IfAny",
            "Format_Version", "Format_Profile", "Format_Level", "Format_Compression",
            "Format_Settings", "InternetMediaType",
            "CodecID", "CodecID_String", "CodecID_Info", "CodecID_Hint",
            "CodecID_Url", "CodecID_Compatible",
            "FileSize", "Duration",
            "OverallBitRate_Mode", "OverallBitRate_Mode_String", "OverallBitRate",
            "StreamSize", "HeaderSize", "DataSize", "FooterSize", "IsStreamable",
            "File_Modified_Date", "File_Modified_Date_Local",
            "Encoded_Date", "Tagged_Date",
            "Encoded_Application", "Encoded_Application_Name",
        ],
        StreamKind::Audio => &[
            "Count", "StreamCount", "StreamKind", "StreamKind_String",
            "StreamKindID", "StreamKindPos", "StreamOrder",
            "ID", "UniqueID",
            "Format", "Format_String", "Format_Info", "Format_Url",
            "Format_Commercial", "Format_Commercial_IfAny",
            "Format_Version", "Format_Profile", "Format_Level", "Format_Compression",
            "Format_Settings", "Format_Settings_Mode", "Format_Settings_ModeExtension",
            "Format_Settings_Endianness", "Format_Settings_Sign", "Format_Settings_SBR",
            "Format_AdditionalFeatures",
            "CodecID", "CodecID_String", "CodecID_Info", "CodecID_Hint", "CodecID_Url",
            "Duration", "Source_Duration", "Source_Duration_LastFrame",
            "BitRate_Mode", "BitRate_Mode_String",
            "BitRate", "BitRate_Minimum", "BitRate_Nominal", "BitRate_Maximum",
            "Channels", "Channels_String", "ChannelPositions", "ChannelLayout",
            "SamplesPerFrame",
            "SamplingRate", "SamplingRate_String", "SamplingCount",
            "FrameRate", "FrameCount", "Source_FrameCount",
            "BitDepth", "BitDepth_String",
            "Compression_Mode",
            "StreamSize", "StreamSize_String", "Source_StreamSize",
            "Delay", "Delay_Source",
            "Encoded_Library", "Encoded_Library_String",
            "Title", "Language",
            "Default", "Forced", "AlternateGroup", "ServiceKind",
        ],
        StreamKind::Image => &[
            "Count", "StreamCount", "StreamKind", "StreamKindID", "StreamKindPos",
            "ID", "Type",
            "Format", "Format_Profile", "Format_Version", "Format_Compression",
            "Format_Settings_Packing", "Format_Settings_Endianness",
            "Width", "Height", "PixelAspectRatio", "DisplayAspectRatio",
            "ColorSpace", "ChromaSubsampling", "BitDepth",
            "Compression_Mode", "StreamSize",
        ],
        StreamKind::Video => &[
            "Count", "StreamCount", "StreamKind", "StreamKindID", "StreamKindPos",
            "StreamOrder", "ID",
            "Format", "Format_Profile", "Format_Settings", "CodecID",
            "Duration",
            "BitRate_Mode", "BitRate",
            "Width", "Height", "DisplayAspectRatio",
            "FrameRate_Mode", "FrameRate", "FrameCount",
            "ColorSpace", "ChromaSubsampling", "BitDepth",
            "ScanType", "StreamSize",
        ],
        _ => &[],
    }
}

fn extra_field_order(kind: StreamKind) -> &'static [&'static str] {
    match kind {
        StreamKind::General => &["ErrorDetectionType"],
        StreamKind::Audio => &[
            "MD5_Unencoded", "bsid", "dialnorm", "dsurmod", "acmod", "lfeon",
            "dialnorm_Average", "dialnorm_Minimum",
            "Source_Delay", "Source_Delay_Source",
        ],
        StreamKind::Image => &[
            "FrameRate", "DPI", "Density_X", "Density_Y", "Density_Unit", "Density_String",
        ],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenlib::Ztring;

    #[test]
    fn text_emits_stream_kind_headers() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", Ztring::from("Wave"), false);
        c.Fill(StreamKind::Audio, 0, "Format", Ztring::from("PCM"), false);
        let text = to_text(&c);
        assert!(text.contains("General\n"));
        assert!(text.contains("Audio\n"));
    }

    #[test]
    fn text_multiple_streams_same_kind_has_numbered_header() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::Audio, 0, "Format", Ztring::from("AAC"), false);
        c.Fill(StreamKind::Audio, 1, "Format", Ztring::from("AC3"), false);
        let text = to_text(&c);
        assert!(text.contains("Audio #1\n"));
        assert!(text.contains("Audio #2\n"));
    }

    #[test]
    fn text_field_padded_to_column_42() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::Audio, 0, "Format", Ztring::from("PCM"), false);
        c.Fill(StreamKind::Audio, 0, "BitDepth", Ztring::from("16"), false);
        let text = to_text(&c);
        let expected = format!("Format{:>1$} : PCM", "", 42 - "Format".len());
        assert!(text.contains(&expected), "expected {:?} in {:?}", expected, text);
    }

    #[test]
    fn text_duration_formatted_as_s_ms() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::Audio, 0, "Duration", Ztring::from("1492"), false);
        let text = to_text(&c);
        assert!(text.contains("1 s 492 ms"));
    }

    #[test]
    fn text_empty_collection_produces_minimal_output() {
        let c = StreamCollection::new();
        let text = to_text(&c);
        assert!(!text.contains("General\n"));
    }

    #[test]
    fn text_blanks_between_kinds() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", Ztring::from("MP4"), false);
        c.Fill(StreamKind::Video, 0, "Format", Ztring::from("AVC"), false);
        let text = to_text(&c);
        let general_pos = text.find("General").unwrap();
        let video_pos = text.find("Video").unwrap();
        let segment = &text[general_pos..video_pos];
        assert!(segment.ends_with("\n\n"));
    }
}
