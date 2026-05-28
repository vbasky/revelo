//! XML formatter matching MediaInfoLib's `--Output=XML` schema and
//! formatting exactly.
//!
//! Header layout, indentation, attribute ordering, and namespace
//! declarations are duplicated from the oracle's output. Field ordering
//! per stream follows a canonical list (mirrors MediaInfoLib's
//! `MediaInfo_Config_PerPackage` definitions for the subset of fields
//! we currently fill); unknown fields fall through in insertion order
//! after the canonical ones.
//!
//! Field-specific rendering:
//! - `Duration` (and `*_Duration` variants): stored as integer
//!   milliseconds, emitted as decimal seconds with 3 fraction digits
//!   (e.g. `1492` → `1.492`).
//! - All other values pass through verbatim, XML-escaped.

use revelio_core::{StreamCollection, StreamKind};

/// Render a full MediaInfo XML document. `file_path` becomes the `ref`
/// attribute on `<media>`; `library_version` becomes the
/// `<creatingLibrary>` version attribute.
pub fn to_xml(streams: &StreamCollection, file_path: &str, library_version: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<MediaInfo\n");
    out.push_str("    xmlns=\"https://mediaarea.net/mediainfo\"\n");
    out.push_str("    xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"\n");
    out.push_str("    xsi:schemaLocation=\"https://mediaarea.net/mediainfo https://mediaarea.net/mediainfo/mediainfo_2_0.xsd\"\n");
    out.push_str("    version=\"2.0\">\n");
    out.push_str(&format!(
        "<creatingLibrary version=\"{}\" url=\"https://mediaarea.net/MediaInfo\">MediaInfoLib</creatingLibrary>\n",
        xml_escape_attr(library_version)
    ));
    out.push_str(&format!(
        "<media ref=\"{}\">\n",
        xml_escape_attr(file_path)
    ));

    for kind in [
        StreamKind::General,
        StreamKind::Video,
        StreamKind::Audio,
        StreamKind::Text,
        StreamKind::Other,
        StreamKind::Image,
        StreamKind::Menu,
    ] {
        let count = streams.Count_Get(kind);
        for pos in 0..count {
            // Oracle emits `typeorder="N"` (1-based) only when there are
            // multiple streams of the same kind. Single-stream kinds get
            // a bare `<track type="X">`.
            if count > 1 {
                out.push_str(&format!(
                    "<track type=\"{}\" typeorder=\"{}\">\n",
                    kind.name(),
                    pos + 1
                ));
            } else {
                out.push_str(&format!("<track type=\"{}\">\n", kind.name()));
            }
            if let Some(stream) = streams.stream(kind, pos) {
                emit_stream_fields(&mut out, kind, stream);
            }
            out.push_str("</track>\n");
        }
    }

    out.push_str("</media>\n");
    out.push_str("</MediaInfo>\n");
    // Oracle ends with a trailing blank line; preserve it for byte-equal output.
    out.push('\n');
    out
}

fn emit_stream_fields(out: &mut String, kind: StreamKind, stream: &revelio_core::Stream) {
    let canonical = canonical_field_order(kind);
    let extras = extra_field_order(kind);
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for field in canonical {
        if let Some(z) = stream.get(field) {
            push_field(out, field, z.as_str());
            emitted.insert(*field);
        }
    }
    // Non-canonical, non-extra fields fall through here in insertion order.
    let extras_set: std::collections::HashSet<&'static str> = extras.iter().copied().collect();
    for (k, v) in stream.iter() {
        if !emitted.contains(k) && !extras_set.contains(k) {
            push_field(out, k, v.as_str());
        }
    }
    // Extra fields are wrapped in a <extra>...</extra> section, present
    // only when at least one such field is set on the stream. Sources:
    //   1. Canonical fields that route to extras by name (Apple QT
    //      passthrough keys, ID3v2 COMM, etc.) — via `extra_field_order`.
    //   2. Per-stream extras filled via `Fill_Extra` — these are
    //      arbitrary tag-style fields parsers chose to bucket as extra.
    let mut extra_buf = String::new();
    for field in extras {
        if let Some(z) = stream.get(field) {
            push_field(&mut extra_buf, field, z.as_str());
        }
    }
    for (k, v) in stream.extras_iter() {
        push_field(&mut extra_buf, k, v.as_str());
    }
    if !extra_buf.is_empty() {
        out.push_str("<extra>\n");
        out.push_str(&extra_buf);
        out.push_str("</extra>\n");
    }
}

fn push_field(out: &mut String, name: &str, raw_value: &str) {
    let rendered = render_field_value(name, raw_value);
    out.push_str(&format!(
        "<{name}>{value}</{name}>\n",
        name = name,
        value = xml_escape_text(&rendered),
    ));
}

/// Per-field value transform. Most fields pass through; Duration-family
/// fields are stored as integer milliseconds but emitted as decimal
/// seconds with 3 fraction digits.
pub(crate) fn render_field_value(name: &str, raw: &str) -> String {
    if is_duration_field(name) {
        if let Ok(ms) = raw.parse::<i64>() {
            return format_milliseconds_as_seconds(ms);
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

fn format_milliseconds_as_seconds(ms: i64) -> String {
    let negative = ms < 0;
    let abs = ms.unsigned_abs();
    let whole = abs / 1000;
    let frac = abs % 1000;
    let body = format!("{whole}.{frac:03}");
    if negative { format!("-{body}") } else { body }
}

fn xml_escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn xml_escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Fields that get wrapped in `<extra>...</extra>` when present on a
/// stream. Mirrors MediaInfoLib's `InfoOption_ShowInXml` flagging that
/// segregates secondary / side-channel fields from the main schema.
pub(crate) fn extra_field_order(kind: StreamKind) -> &'static [&'static str] {
    match kind {
        StreamKind::General => &["ErrorDetectionType"],
        StreamKind::Audio => &[
            "MD5_Unencoded",
            "bsid",
            "dialnorm",
            "dsurmod",
            "acmod",
            "lfeon",
            "dialnorm_Average",
            "dialnorm_Minimum",
            "Source_Delay",
            "Source_Delay_Source",
        ],
        StreamKind::Image => &["FrameRate", "DPI", "Density_X", "Density_Y", "Density_Unit", "Density_String"],
        _ => &[],
    }
}

/// Canonical field order per stream kind. Mirrors MediaInfoLib's
/// `MediaInfo_Config_PerPackage` order for the subset of fields we
/// currently fill. Unknown fields fall through in insertion order.
pub(crate) fn canonical_field_order(kind: StreamKind) -> &'static [&'static str] {
    match kind {
        StreamKind::General => &[
            "Count",
            "StreamCount",
            "StreamKind",
            "StreamKind_String",
            "StreamKindID",
            "StreamKindPos",
            "UniqueID",
            "VideoCount",
            "AudioCount",
            "TextCount",
            "OtherCount",
            "ImageCount",
            "MenuCount",
            "Video_Format_WithHint_List",
            "Video_Format_List",
            "Audio_Format_List",
            "CompleteName",
            "FolderName",
            "FileNameExtension",
            "FileName",
            "FileExtension",
            "Format",
            "Format_String",
            "Format_Info",
            "Format_Url",
            "Format_Commercial",
            "Format_Commercial_IfAny",
            "Format_Version",
            "Format_Profile",
            "Format_Level",
            "Format_Compression",
            "Format_Settings",
            "InternetMediaType",
            "CodecID",
            "CodecID_String",
            "CodecID_Info",
            "CodecID_Hint",
            "CodecID_Url",
            "CodecID_Compatible",
            "FileSize",
            "Duration",
            "OverallBitRate_Mode",
            "OverallBitRate_Mode_String",
            "OverallBitRate",
            "OverallBitRate_Minimum",
            "OverallBitRate_Nominal",
            "OverallBitRate_Maximum",
            "StreamSize",
            "HeaderSize",
            "DataSize",
            "FooterSize",
            "IsStreamable",
            // Oracle emits the content dates (Encoded/Tagged) before the
            // filesystem modification dates, then the writing library /
            // application.
            "Encoded_Date",
            "Tagged_Date",
            "File_Modified_Date",
            "File_Modified_Date_Local",
            "Encoded_Library",
            "Encoded_Library_String",
            "Encoded_Application",
            "Encoded_Application_Name",
        ],
        StreamKind::Audio => &[
            "Count",
            "StreamCount",
            "StreamKind",
            "StreamKind_String",
            "StreamKindID",
            "StreamKindPos",
            "StreamOrder",
            "ID",
            "UniqueID",
            "Format",
            "Format_String",
            "Format_Info",
            "Format_Url",
            "Format_Commercial",
            "Format_Commercial_IfAny",
            "Format_Version",
            "Format_Profile",
            "Format_Level",
            "Format_Compression",
            "Format_Settings",
            "Format_Settings_Mode",
            "Format_Settings_ModeExtension",
            "Format_Settings_Endianness",
            "Format_Settings_Sign",
            "Format_Settings_SBR",
            "Format_AdditionalFeatures",
            "CodecID",
            "CodecID_String",
            "CodecID_Info",
            "CodecID_Hint",
            "CodecID_Url",
            "Duration",
            "Source_Duration",
            "Source_Duration_LastFrame",
            "BitRate_Mode",
            "BitRate_Mode_String",
            "BitRate",
            "BitRate_Minimum",
            "BitRate_Nominal",
            "BitRate_Maximum",
            "Channels",
            "Channels_String",
            "ChannelPositions",
            "ChannelLayout",
            "SamplesPerFrame",
            "SamplingRate",
            "SamplingRate_String",
            "SamplingCount",
            "FrameRate",
            "FrameCount",
            "Source_FrameCount",
            "BitDepth",
            "BitDepth_String",
            "Compression_Mode",
            "StreamSize",
            "StreamSize_String",
            "Source_StreamSize",
            "Delay",
            "Delay_Source",
            "Encoded_Library",
            "Encoded_Library_String",
            "Title",
            "Language",
            "Default",
            "Forced",
            "AlternateGroup",
            "ServiceKind",
        ],
        StreamKind::Image => &[
            "Count",
            "StreamCount",
            "StreamKind",
            "StreamKindID",
            "StreamKindPos",
            "ID",
            "Type",
            "Format",
            "Format_Profile",
            "Format_Version",
            "Format_Compression",
            "Format_Settings_Packing",
            "Format_Settings_Endianness",
            "Width",
            "Height",
            "PixelAspectRatio",
            "DisplayAspectRatio",
            "ColorSpace",
            "ChromaSubsampling",
            "BitDepth",
            "Compression_Mode",
            "StreamSize",
        ],
        StreamKind::Video => &[
            "Count",
            "StreamCount",
            "StreamKind",
            "StreamKindID",
            "StreamKindPos",
            "StreamOrder",
            "ID",
            "Format",
            "Format_Profile",
            "Format_Settings",
            "CodecID",
            "Duration",
            "BitRate_Mode",
            "BitRate",
            "Width",
            "Height",
            "DisplayAspectRatio",
            "FrameRate_Mode",
            "FrameRate",
            "FrameCount",
            "ColorSpace",
            "ChromaSubsampling",
            "BitDepth",
            "ScanType",
            "StreamSize",
        ],
        // Other kinds: empty canonical list → falls back to pure insertion order.
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenlib::Ztring;

    fn build_wav_streams() -> StreamCollection {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", Ztring::from("Wave"), false);
        c.Fill(StreamKind::General, 0, "Format_Settings", Ztring::from("PcmWaveformat"), false);
        c.Fill(StreamKind::General, 0, "AudioCount", Ztring::from("1"), false);
        c.Fill(StreamKind::Audio, 0, "Format", Ztring::from("PCM"), false);
        c.Fill(StreamKind::Audio, 0, "Format_Settings_Endianness", Ztring::from("Little"), false);
        c.Fill(StreamKind::Audio, 0, "Format_Settings_Sign", Ztring::from("Signed"), false);
        c.Fill(StreamKind::Audio, 0, "CodecID", Ztring::from("1"), false);
        c.Fill(StreamKind::Audio, 0, "BitRate_Mode", Ztring::from("CBR"), false);
        c.Fill(StreamKind::Audio, 0, "BitRate", Ztring::from("1536000"), false);
        c.Fill(StreamKind::Audio, 0, "Channels", Ztring::from("2"), false);
        c.Fill(StreamKind::Audio, 0, "SamplingRate", Ztring::from("48000"), false);
        c.Fill(StreamKind::Audio, 0, "BitDepth", Ztring::from("16"), false);
        c.Fill(StreamKind::Audio, 0, "StreamSize", Ztring::from("286552"), false);
        c.Fill(StreamKind::Audio, 0, "SamplingCount", Ztring::from("71638"), false);
        c.Fill(StreamKind::Audio, 0, "Duration", Ztring::from("1492"), false);
        c
    }

    #[test]
    fn xml_header_matches_oracle_format() {
        let c = StreamCollection::new();
        let xml = to_xml(&c, "/tmp/foo.wav", "26.05");
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<MediaInfo\n"));
        assert!(xml.contains("    xmlns=\"https://mediaarea.net/mediainfo\"\n"));
        assert!(xml.contains("    xsi:schemaLocation=\"https://mediaarea.net/mediainfo https://mediaarea.net/mediainfo/mediainfo_2_0.xsd\"\n"));
        assert!(xml.contains("    version=\"2.0\">\n"));
        assert!(xml.contains("<creatingLibrary version=\"26.05\" url=\"https://mediaarea.net/MediaInfo\">MediaInfoLib</creatingLibrary>\n"));
        assert!(xml.contains("<media ref=\"/tmp/foo.wav\">\n"));
    }

    #[test]
    fn duration_is_emitted_as_decimal_seconds() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::Audio, 0, "Duration", Ztring::from("1492"), false);
        let xml = to_xml(&c, "/tmp/x.wav", "26.05");
        assert!(xml.contains("<Duration>1.492</Duration>"));

        let mut c2 = StreamCollection::new();
        c2.Fill(StreamKind::Audio, 0, "Duration", Ztring::from("60000"), false);
        assert!(to_xml(&c2, "x", "v").contains("<Duration>60.000</Duration>"));

        let mut c3 = StreamCollection::new();
        c3.Fill(StreamKind::Audio, 0, "Duration", Ztring::from("7"), false);
        assert!(to_xml(&c3, "x", "v").contains("<Duration>0.007</Duration>"));
    }

    #[test]
    fn audio_fields_emitted_in_canonical_order() {
        let c = build_wav_streams();
        let xml = to_xml(&c, "/tmp/x.wav", "26.05");
        let audio_section = xml.split("<track type=\"Audio\">").nth(1).unwrap();
        let audio_section = audio_section.split("</track>").next().unwrap();

        let lines: Vec<&str> = audio_section
            .lines()
            .filter(|l| l.contains('<') && !l.is_empty())
            .collect();

        let names: Vec<String> = lines
            .iter()
            .filter_map(|l| {
                let start = l.find('<')? + 1;
                let end = l[start..].find('>')? + start;
                Some(l[start..end].to_string())
            })
            .collect();

        assert_eq!(
            names,
            vec![
                "Format",
                "Format_Settings_Endianness",
                "Format_Settings_Sign",
                "CodecID",
                "Duration",
                "BitRate_Mode",
                "BitRate",
                "Channels",
                "SamplingRate",
                "SamplingCount",
                "BitDepth",
                "StreamSize",
            ]
        );
    }

    #[test]
    fn empty_collection_produces_well_formed_skeleton() {
        let c = StreamCollection::new();
        let xml = to_xml(&c, "/tmp/x.wav", "26.05");
        assert!(xml.ends_with("</media>\n</MediaInfo>\n\n"));
        assert!(!xml.contains("<track"));
    }

    #[test]
    fn xml_escapes_special_chars_in_values() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", Ztring::from("A & B <C>"), false);
        let xml = to_xml(&c, "/x", "v");
        assert!(xml.contains("<Format>A &amp; B &lt;C&gt;</Format>"));
    }

    #[test]
    fn xml_escapes_quotes_in_attributes() {
        let c = StreamCollection::new();
        let xml = to_xml(&c, "/tmp/with \"quote\".wav", "v");
        assert!(xml.contains("<media ref=\"/tmp/with &quot;quote&quot;.wav\">"));
    }

    #[test]
    fn unknown_fields_fall_through_in_insertion_order() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", Ztring::from("Wave"), false);
        c.Fill(StreamKind::General, 0, "ZZZ_Custom", Ztring::from("first"), false);
        c.Fill(StreamKind::General, 0, "AAA_Custom", Ztring::from("second"), false);
        let xml = to_xml(&c, "x", "v");

        let format_idx = xml.find("<Format>Wave</Format>").unwrap();
        let zzz_idx = xml.find("<ZZZ_Custom>").unwrap();
        let aaa_idx = xml.find("<AAA_Custom>").unwrap();
        assert!(format_idx < zzz_idx);
        assert!(zzz_idx < aaa_idx);
    }
}
