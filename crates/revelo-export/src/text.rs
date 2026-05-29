//! Human-readable `Text` output — the default `mediainfo` format.
//!
//! Unlike XML/JSON (which emit raw internal field names + values), the
//! Text format uses MediaInfo's friendly labels ("Codec ID", "Bit rate",
//! "Frame rate") and humanised values ("5.26 MiB", "1 min 0 s",
//! "734 kb/s", "640 pixels"). Field selection is curated per stream kind
//! — only the display fields appear, in MediaInfo's order.
//!
//! This is a pragmatic transliteration of `File__Analyze::Inform`: the
//! common standalone + a handful of combined fields are matched; a few
//! fully-computed oracle fields (Bits/(Pixel*Frame), exact DAR ratio
//! reduction) are approximated or omitted.

use revelo_core::{Stream, StreamCollection, StreamKind};

const FIELD_COLUMN_WIDTH: usize = 41;

pub fn to_text(streams: &StreamCollection, path: &str) -> String {
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

    // File size is needed to render Stream size percentages.
    let file_size: Option<u64> = streams
        .stream(StreamKind::General, 0)
        .and_then(|s| s.get("FileSize"))
        .and_then(|z| z.as_str().parse().ok());

    let mut first = true;
    for kind in kinds {
        let count = streams.stream_count(kind);
        for pos in 0..count {
            if !first {
                out.push('\n');
            }
            first = false;
            if count > 1 {
                out.push_str(&format!("{} #{}\n", kind.name(), pos + 1));
            } else {
                out.push_str(kind.name());
                out.push('\n');
            }
            if let Some(stream) = streams.stream(kind, pos) {
                emit_section(&mut out, kind, stream, path, file_size);
            }
        }
    }
    out
}

/// Per-kind ordered list of internal field names to display. Fields not
/// listed are hidden from Text output (they remain in XML/JSON).
fn display_fields(kind: StreamKind) -> &'static [&'static str] {
    match kind {
        StreamKind::General => &[
            "CompleteName",
            "Format",
            "Format_Version",
            "CodecID",
            "FileSize",
            "Duration",
            "OverallBitRate",
            "FrameRate",
            "IsComplete",
            "Encoded_Application",
            "Encoded_Library",
            "Encoded_Date",
            "Tagged_Date",
            "Comment",
        ],
        StreamKind::Video => &[
            "ID",
            "Format",
            "Format_Info",
            "Format_Profile",
            "Format_Settings_CABAC",
            "Format_Settings_RefFrames",
            "Format_Settings_GOP",
            "Format_Settings_SliceCount",
            "CodecID",
            "Duration",
            "BitRate",
            "Width",
            "Height",
            "DisplayAspectRatio",
            "FrameRate_Mode",
            "FrameRate",
            "ColorSpace",
            "ChromaSubsampling",
            "BitDepth",
            "ScanType",
            "Bits_Pixel_Frame",
            "StreamSize",
            "Encoded_Library",
            "Encoded_Library_Settings",
            "Title",
            "Language",
            "Default",
            "Forced",
            "Encoded_Date",
            "Tagged_Date",
            "colour_range",
            "colour_primaries",
            "transfer_characteristics",
            "matrix_coefficients",
            "CodecConfigurationBox",
        ],
        StreamKind::Audio => &[
            "ID",
            "Format",
            "Format_Info",
            "Format_Version",
            "Format_Profile",
            "Format_Settings_Mode",
            "CodecID",
            "Duration",
            "Source_Duration",
            "BitRate_Mode",
            "BitRate",
            "BitRate_Maximum",
            "Channels",
            "ChannelLayout",
            "SamplingRate",
            "BitDepth",
            "FrameRate",
            "Compression_Mode",
            "Delay",
            "StreamSize",
            "Source_StreamSize",
            "Title",
            "Language",
            "Default",
            "Forced",
            "Encoded_Library",
            "Encoded_Date",
            "Tagged_Date",
        ],
        StreamKind::Other => &[
            "ID",
            "Type",
            "Format",
            "CodecID",
            "Duration",
            "Source_Duration",
            "BitRate_Mode",
            "FrameCount",
            "StreamSize",
            "Source_StreamSize",
            "Title",
            "Language",
            "Default",
            "Encoded_Date",
            "Tagged_Date",
        ],
        StreamKind::Image => &[
            "ID",
            "Type",
            "Format",
            "Format_Profile",
            "Width",
            "Height",
            "ColorSpace",
            "ChromaSubsampling",
            "BitDepth",
            "Compression_Mode",
            "StreamSize",
        ],
        _ => &[],
    }
}

fn emit_section(
    out: &mut String,
    kind: StreamKind,
    stream: &Stream,
    path: &str,
    file_size: Option<u64>,
) {
    for &field in display_fields(kind) {
        let Some((label, value)) = render(kind, field, stream, path, file_size) else {
            continue;
        };
        emit_line(out, label, &value);
    }
}

fn emit_line(out: &mut String, label: &str, value: &str) {
    out.push_str(label);
    let pad = FIELD_COLUMN_WIDTH.saturating_sub(label.chars().count());
    for _ in 0..pad {
        out.push(' ');
    }
    out.push_str(": ");
    out.push_str(value);
    out.push('\n');
}

/// Map an internal field to (display label, rendered value). Returns None
/// when the field is absent or should be suppressed.
fn render(
    kind: StreamKind,
    field: &str,
    s: &Stream,
    path: &str,
    file_size: Option<u64>,
) -> Option<(&'static str, String)> {
    match field {
        "CompleteName" => Some(("Complete name", path.to_owned())),

        "IsComplete" => {
            let v = s.get("IsComplete")?.as_str();
            Some(("Is complete", if v == "Yes" { "Yes" } else { v }.to_owned()))
        }

        "Format" => {
            let fmt = s.get("Format")?.as_str().to_owned();
            // Audio folds Format_AdditionalFeatures into the Format line
            // ("AAC" + "LC" → "AAC LC").
            if kind == StreamKind::Audio
                && let Some(feat) = s.get("Format_AdditionalFeatures")
            {
                return Some(("Format", format!("{fmt} {}", feat.as_str())));
            }
            Some(("Format", fmt))
        }
        "Format_Info" => Some(("Format/Info", s.get("Format_Info")?.as_str().to_owned())),
        "Format_Version" => {
            Some(("Format version", format!("Version {}", s.get("Format_Version")?.as_str())))
        }

        "Format_Profile" => {
            let prof = s.get("Format_Profile")?.as_str().to_owned();
            // Video combines profile + level: "Constrained Baseline@L3".
            if kind == StreamKind::Video
                && let Some(level) = s.get("Format_Level")
            {
                return Some(("Format profile", format!("{prof}@L{}", level.as_str())));
            }
            Some(("Format profile", prof))
        }
        "Format_Settings_Mode" => {
            Some(("Format settings", s.get("Format_Settings_Mode")?.as_str().to_owned()))
        }
        "Format_Settings_CABAC" => {
            Some(("Format settings, CABAC", s.get("Format_Settings_CABAC")?.as_str().to_owned()))
        }
        "Format_Settings_RefFrames" => {
            let n = s.get("Format_Settings_RefFrames")?.as_str().to_owned();
            Some(("Format settings, Reference frames", format!("{n} frames")))
        }
        "Format_Settings_GOP" => {
            Some(("Format settings, GOP", s.get("Format_Settings_GOP")?.as_str().to_owned()))
        }
        "Format_Settings_SliceCount" => {
            let n = s.get("Format_Settings_SliceCount")?.as_str().to_owned();
            Some(("Format settings, Slice count", format!("{n} slices per frame")))
        }

        "CodecID" => {
            let id = s.get("CodecID")?.as_str().to_owned();
            // General folds CodecID_Compatible: "mp42 (mp42/avc1)".
            if kind == StreamKind::General
                && let Some(compat) = s.get("CodecID_Compatible")
            {
                return Some(("Codec ID", format!("{id} ({})", compat.as_str())));
            }
            Some(("Codec ID", id))
        }
        "CodecConfigurationBox" => {
            Some(("Codec configuration box", s.get("CodecConfigurationBox")?.as_str().to_owned()))
        }

        "FileSize" => {
            let bytes: u64 = s.get("FileSize")?.as_str().parse().ok()?;
            Some(("File size", human_size(bytes)))
        }
        "StreamSize" => {
            let bytes: u64 = s.get("StreamSize")?.as_str().parse().ok()?;
            Some(("Stream size", human_size_with_pct(bytes, file_size)))
        }
        "Source_StreamSize" => {
            let bytes: u64 = s.get("Source_StreamSize")?.as_str().parse().ok()?;
            Some(("Source stream size", human_size_with_pct(bytes, file_size)))
        }

        "Duration" => Some(("Duration", human_duration(s.get("Duration")?.as_str()))),
        "Source_Duration" => {
            Some(("Source duration", human_duration(s.get("Source_Duration")?.as_str())))
        }

        "OverallBitRate" => {
            let bps: u64 = s.get("OverallBitRate")?.as_str().parse().ok()?;
            Some(("Overall bit rate", human_bitrate(bps)))
        }
        "OverallBitRate_Mode" => {
            Some(("Overall bit rate mode", mode_word(s.get("OverallBitRate_Mode")?.as_str())))
        }
        "BitRate" => {
            let bps: u64 = s.get("BitRate")?.as_str().parse().ok()?;
            Some(("Bit rate", human_bitrate(bps)))
        }
        "BitRate_Mode" => Some(("Bit rate mode", mode_word(s.get("BitRate_Mode")?.as_str()))),
        "BitRate_Maximum" => {
            let bps: u64 = s.get("BitRate_Maximum")?.as_str().parse().ok()?;
            Some(("Maximum bit rate", human_bitrate(bps)))
        }

        "Width" => Some(("Width", format!("{} pixels", thousands_space(s.get("Width")?.as_str())))),
        "Height" => {
            Some(("Height", format!("{} pixels", thousands_space(s.get("Height")?.as_str()))))
        }
        "DisplayAspectRatio" => {
            Some(("Display aspect ratio", display_aspect(s.get("DisplayAspectRatio")?.as_str())))
        }

        "FrameRate_Mode" => Some(("Frame rate mode", mode_word(s.get("FrameRate_Mode")?.as_str()))),
        "FrameRate" => {
            let fps = s.get("FrameRate")?.as_str().to_owned();
            // Audio appends samples-per-frame: "21.533 FPS (1024 SPF)".
            if kind == StreamKind::Audio
                && let Some(spf) = s.get("SamplesPerFrame")
            {
                return Some(("Frame rate", format!("{fps} FPS ({} SPF)", spf.as_str())));
            }
            Some(("Frame rate", format!("{fps} FPS")))
        }

        "ColorSpace" => Some(("Color space", s.get("ColorSpace")?.as_str().to_owned())),
        "ChromaSubsampling" => {
            let cs = s.get("ChromaSubsampling")?.as_str().to_owned();
            if let Some(posn) = s.get("ChromaSubsampling_Position") {
                return Some(("Chroma subsampling", format!("{cs} ({})", posn.as_str())));
            }
            Some(("Chroma subsampling", cs))
        }
        "BitDepth" => Some(("Bit depth", format!("{} bits", s.get("BitDepth")?.as_str()))),
        "ScanType" => Some(("Scan type", s.get("ScanType")?.as_str().to_owned())),
        // Text-only derived field (absent from XML): bitrate per pixel per
        // frame = BitRate / (Width · Height · FrameRate).
        "Bits_Pixel_Frame" => {
            let bitrate: f64 = s.get("BitRate")?.as_str().parse().ok()?;
            let width: f64 = s.get("Width")?.as_str().parse().ok()?;
            let height: f64 = s.get("Height")?.as_str().parse().ok()?;
            let fr: f64 = s.get("FrameRate")?.as_str().parse().ok()?;
            let denom = width * height * fr;
            if denom <= 0.0 {
                return None;
            }
            Some(("Bits/(Pixel*Frame)", format!("{:.3}", bitrate / denom)))
        }

        "Channels" => {
            let n = s.get("Channels")?.as_str().to_owned();
            let unit = if n == "1" { "channel" } else { "channels" };
            Some(("Channel(s)", format!("{n} {unit}")))
        }
        "ChannelLayout" => Some(("Channel layout", s.get("ChannelLayout")?.as_str().to_owned())),
        "SamplingRate" => {
            let hz: u64 = s.get("SamplingRate")?.as_str().parse().ok()?;
            Some(("Sampling rate", human_sampling_rate(hz)))
        }
        "Compression_Mode" => {
            Some(("Compression mode", s.get("Compression_Mode")?.as_str().to_owned()))
        }

        "ID" => Some(("ID", s.get("ID")?.as_str().to_owned())),
        "Type" => Some(("Type", s.get("Type")?.as_str().to_owned())),
        "FrameCount" => Some(("Frame count", s.get("FrameCount")?.as_str().to_owned())),
        "Title" => Some(("Title", s.get("Title")?.as_str().to_owned())),
        "Language" => Some(("Language", language_name(s.get("Language")?.as_str()))),
        "Default" => Some(("Default", s.get("Default")?.as_str().to_owned())),
        "Forced" => Some(("Forced", s.get("Forced")?.as_str().to_owned())),
        "Delay" => Some(("Delay relative to video", delay_ms(s.get("Delay")?.as_str()))),

        "Encoded_Library" => {
            Some(("Writing library", s.get("Encoded_Library")?.as_str().to_owned()))
        }
        "Encoded_Library_Settings" => {
            Some(("Encoding settings", s.get("Encoded_Library_Settings")?.as_str().to_owned()))
        }
        "Encoded_Application" => {
            Some(("Writing application", s.get("Encoded_Application")?.as_str().to_owned()))
        }
        "Encoded_Date" => Some(("Encoded date", s.get("Encoded_Date")?.as_str().to_owned())),
        "Tagged_Date" => Some(("Tagged date", s.get("Tagged_Date")?.as_str().to_owned())),
        "Comment" => Some(("comment", s.get("Comment")?.as_str().to_owned())),

        "colour_range" => Some(("Color range", s.get("colour_range")?.as_str().to_owned())),
        "colour_primaries" => {
            Some(("Color primaries", s.get("colour_primaries")?.as_str().to_owned()))
        }
        "transfer_characteristics" => Some((
            "Transfer characteristics",
            s.get("transfer_characteristics")?.as_str().to_owned(),
        )),
        "matrix_coefficients" => {
            Some(("Matrix coefficients", s.get("matrix_coefficients")?.as_str().to_owned()))
        }

        _ => None,
    }
}

/// CFR/CBR → "Constant", VFR/VBR → "Variable". Pass through anything else.
fn mode_word(raw: &str) -> String {
    match raw {
        "CFR" | "CBR" => "Constant".to_owned(),
        "VFR" | "VBR" => "Variable".to_owned(),
        other => other.to_owned(),
    }
}

/// Byte count → MediaInfo's "X.XX MiB" / "XXX KiB" form (~3 sig figs).
fn human_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GIB {
        format!("{} GiB", trim3(b / GIB))
    } else if b >= MIB {
        format!("{} MiB", trim3(b / MIB))
    } else if b >= KIB {
        format!("{} KiB", trim3(b / KIB))
    } else {
        format!("{bytes} Bytes")
    }
}

fn human_size_with_pct(bytes: u64, file_size: Option<u64>) -> String {
    let base = human_size(bytes);
    if let Some(fs) = file_size
        && fs > 0
    {
        let pct = (bytes as f64 * 100.0 / fs as f64).round() as u64;
        return format!("{base} ({pct}%)");
    }
    base
}

/// Round to ~3 significant figures, dropping trailing ".0".
fn trim3(v: f64) -> String {
    if v >= 100.0 {
        format!("{:.0}", v)
    } else if v >= 10.0 {
        format!("{:.1}", v)
    } else {
        format!("{:.2}", v)
    }
}

/// ms → "1 h 2 min" / "3 min 29 s" / "29 s" / "95 ms".
/// Also handles float seconds (e.g. "4050.208" from some parsers).
fn human_duration(raw: &str) -> String {
    let total_ms = raw
        .parse::<i64>()
        .or_else(|_| raw.parse::<f64>().map(|secs| (secs * 1000.0).round() as i64));
    let ms: i64 = match total_ms {
        Ok(v) => v,
        Err(_) => return raw.to_owned(),
    };
    let total_s = ms / 1000;
    let h = total_s / 3600;
    let min = (total_s % 3600) / 60;
    let s = total_s % 60;
    let rem_ms = ms % 1000;
    if h > 0 {
        format!("{h} h {min} min")
    } else if min > 0 {
        format!("{min} min {s} s")
    } else if s > 0 {
        format!("{s} s")
    } else {
        format!("{rem_ms} ms")
    }
}

/// Delay value as ms → "7 ms" / "0 ms". Handles float seconds too.
fn delay_ms(raw: &str) -> String {
    let ms = raw
        .parse::<i64>()
        .or_else(|_| raw.parse::<f64>().map(|secs| (secs * 1000.0).round() as i64))
        .unwrap_or(0);
    format!("{ms} ms")
}

/// bps → "734 kb/s" / "1.50 Mb/s" (~3 sig figs).
fn human_bitrate(bps: u64) -> String {
    let kb = bps as f64 / 1000.0;
    if kb >= 1000.0 {
        format!("{} Mb/s", trim3(kb / 1000.0))
    } else {
        format!("{} kb/s", trim3(kb))
    }
}

/// Hz → "48.0 kHz" / "22.05 kHz".
fn human_sampling_rate(hz: u64) -> String {
    let khz = hz as f64 / 1000.0;
    // MediaInfo keeps up to 3 significant figures but preserves the
    // ".05" of 22050 → "22.05 kHz", and shows "48.0 kHz" for 48000.
    if (khz.fract() * 100.0).round() % 100.0 == 0.0 {
        format!("{:.1} kHz", khz)
    } else {
        let s = format!("{:.3}", khz);
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        format!("{trimmed} kHz")
    }
}

/// Format integer string with space as thousands separator: "1920" → "1 920".
fn thousands_space(raw: &str) -> String {
    let mut s = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut count = 0;
    for c in chars.iter().rev() {
        if count > 0 && count % 3 == 0 {
            s.push(' ');
        }
        s.push(*c);
        count += 1;
    }
    s.chars().rev().collect()
}

/// Common display-aspect decimals → ratio strings. Falls back to the
/// raw decimal when unrecognised.
fn display_aspect(raw: &str) -> String {
    match raw {
        "1.778" => "16:9".to_owned(),
        "1.333" => "4:3".to_owned(),
        "1.600" => "16:10".to_owned(),
        "2.350" | "2.35" => "2.35:1".to_owned(),
        "2.400" | "2.40" => "2.40:1".to_owned(),
        "1.850" => "1.85:1".to_owned(),
        "0.562" => "9:16".to_owned(),
        "0.750" => "3:4".to_owned(),
        other => other.to_owned(),
    }
}

/// ISO 639-1/639-2 code → English language name. Subset covering the
/// common cases; unknown codes pass through unchanged.
fn language_name(code: &str) -> String {
    match code {
        "en" | "eng" => "English",
        "es" | "spa" => "Spanish",
        "fr" | "fre" | "fra" => "French",
        "de" | "ger" | "deu" => "German",
        "it" | "ita" => "Italian",
        "ja" | "jpn" => "Japanese",
        "ko" | "kor" => "Korean",
        "zh" | "chi" | "zho" => "Chinese",
        "ru" | "rus" => "Russian",
        "pt" | "por" => "Portuguese",
        "nl" | "dut" | "nld" => "Dutch",
        "ar" | "ara" => "Arabic",
        "hi" | "hin" => "Hindi",
        other => return other.to_owned(),
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_util::Ztring;

    fn stream_with(kind: StreamKind, fields: &[(&str, &str)]) -> StreamCollection {
        let mut c = StreamCollection::new();
        for (k, v) in fields {
            c.set_field(kind, 0, k, Ztring::from(*v));
        }
        c
    }

    #[test]
    fn general_uses_friendly_labels_and_humanized_values() {
        let c = stream_with(
            StreamKind::General,
            &[
                ("Format", "MPEG-4"),
                ("FileSize", "5510872"),
                ("Duration", "60095"),
                ("OverallBitRate", "733621"),
            ],
        );
        let t = to_text(&c, "/tmp/x.mp4");
        assert!(t.contains("Complete name"), "{t}");
        assert!(t.contains("File size") && t.contains("5.26 MiB"), "{t}");
        assert!(t.contains("Duration") && t.contains("1 min 0 s"), "{t}");
        assert!(t.contains("Overall bit rate") && t.contains("734 kb/s"), "{t}");
        // raw internal name must NOT appear
        assert!(!t.contains("OverallBitRate "), "{t}");
    }

    #[test]
    fn video_combines_profile_level_and_humanizes() {
        let c = stream_with(
            StreamKind::Video,
            &[
                ("Format", "AVC"),
                ("Format_Profile", "Constrained Baseline"),
                ("Format_Level", "3"),
                ("Width", "640"),
                ("Height", "360"),
                ("FrameRate_Mode", "CFR"),
                ("BitDepth", "8"),
            ],
        );
        let t = to_text(&c, "/tmp/x.mp4");
        assert!(t.contains("Constrained Baseline@L3"), "{t}");
        assert!(t.contains("640 pixels"), "{t}");
        assert!(t.contains("Frame rate mode") && t.contains("Constant"), "{t}");
        assert!(t.contains("8 bits"), "{t}");
    }

    #[test]
    fn audio_merges_additional_features_and_channels() {
        let c = stream_with(
            StreamKind::Audio,
            &[
                ("Format", "AAC"),
                ("Format_AdditionalFeatures", "LC"),
                ("Channels", "2"),
                ("SamplingRate", "22050"),
                ("BitRate_Mode", "CBR"),
            ],
        );
        let t = to_text(&c, "/tmp/x.mp4");
        assert!(t.contains("AAC LC"), "{t}");
        assert!(t.contains("2 channels"), "{t}");
        assert!(t.contains("22.05 kHz"), "{t}");
        assert!(t.contains("Bit rate mode") && t.contains("Constant"), "{t}");
    }

    #[test]
    fn language_code_expands_to_name() {
        let c = stream_with(StreamKind::Audio, &[("Format", "AAC"), ("Language", "en")]);
        let t = to_text(&c, "/tmp/x.mp4");
        assert!(t.contains("Language") && t.contains("English"), "{t}");
    }
}
