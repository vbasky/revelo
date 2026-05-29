//! Summary output — aggregate statistics across all streams.
//!
//! Produces a compact report showing file summary, video/audio/text/image
//! stream counts with unique codecs, resolution ranges, and other aggregates.

use revelo_core::{StreamCollection, StreamKind};
use std::collections::BTreeSet;

pub fn to_summary(streams: &StreamCollection, path: &str) -> String {
    let mut out = String::new();

    // File header
    out.push_str(&format!("File: {path}\n"));

    // General
    if let Some(s) = streams.stream(StreamKind::General, 0) {
        if let Some(fmt) = s.get("Format") {
            out.push_str(&format!("Container: {}\n", fmt.as_str()));
        }
        if let Some(sz) = s.get("FileSize") {
            if let Ok(bytes) = sz.as_str().parse::<u64>() {
                out.push_str(&format!("Size: {}\n", human_size(bytes)));
            }
        }
        if let Some(dur) = s.get("Duration") {
            if let Ok(ms) = dur.as_str().parse::<i64>() {
                out.push_str(&format!("Duration: {}\n", human_duration(ms)));
            }
        }
        if let Some(br) = s.get("OverallBitRate") {
            if let Ok(bps) = br.as_str().parse::<u64>() {
                out.push_str(&format!("Overall bit rate: {}\n", human_bitrate(bps)));
            }
        }
    }

    out.push('\n');

    // Stream counts
    let vc = streams.stream_count(StreamKind::Video);
    let ac = streams.stream_count(StreamKind::Audio);
    let tc = streams.stream_count(StreamKind::Text);
    let ic = streams.stream_count(StreamKind::Image);
    out.push_str(&format!("Streams: {} video, {} audio, {} text, {} image\n\n", vc, ac, tc, ic));

    // Video summary
    if vc > 0 {
        out.push_str("Video\n");
        out.push_str("-----\n");

        let mut codecs: BTreeSet<String> = BTreeSet::new();
        let mut min_width = u64::MAX;
        let mut max_width = 0u64;
        let mut min_height = u64::MAX;
        let mut max_height = 0u64;
        let mut frame_rates: BTreeSet<String> = BTreeSet::new();
        let mut bit_depths: BTreeSet<u64> = BTreeSet::new();
        let mut scan_types: BTreeSet<String> = BTreeSet::new();

        for pos in 0..vc {
            if let Some(s) = streams.stream(StreamKind::Video, pos) {
                if let Some(f) = s.get("Format") {
                    codecs.insert(f.as_str().to_owned());
                }
                if let Some(w) = s.get("Width") {
                    if let Ok(wv) = w.as_str().parse::<u64>() {
                        min_width = min_width.min(wv);
                        max_width = max_width.max(wv);
                    }
                }
                if let Some(h) = s.get("Height") {
                    if let Ok(hv) = h.as_str().parse::<u64>() {
                        min_height = min_height.min(hv);
                        max_height = max_height.max(hv);
                    }
                }
                if let Some(fr) = s.get("FrameRate") {
                    frame_rates.insert(fr.as_str().to_owned());
                }
                if let Some(bd) = s.get("BitDepth") {
                    if let Ok(bv) = bd.as_str().parse::<u64>() {
                        bit_depths.insert(bv);
                    }
                }
                if let Some(st) = s.get("ScanType") {
                    scan_types.insert(st.as_str().to_owned());
                }
            }
        }

        let codec_list: Vec<&str> = codecs.iter().map(|s| s.as_str()).collect();
        out.push_str(&format!("  Codec(s): {}\n", codec_list.join(", ")));

        if vc == 1 {
            out.push_str(&format!("  Resolution: {}x{}\n", max_width, max_height));
        } else {
            out.push_str(&format!(
                "  Resolution: {}x{} - {}x{}\n",
                min_width, min_height, max_width, max_height
            ));
        }

        let fr_list: Vec<&str> = frame_rates.iter().map(|s| s.as_str()).collect();
        out.push_str(&format!("  Frame rate(s): {}\n", fr_list.join(", ")));

        for bd in &bit_depths {
            out.push_str(&format!("  Bit depth: {} bit\n", bd));
        }

        for st in &scan_types {
            out.push_str(&format!("  Scan type: {}\n", st));
        }

        out.push('\n');
    }

    // Audio summary
    if ac > 0 {
        out.push_str("Audio\n");
        out.push_str("-----\n");

        let mut codecs: BTreeSet<String> = BTreeSet::new();
        let mut channels: BTreeSet<u64> = BTreeSet::new();
        let mut sample_rates: BTreeSet<u64> = BTreeSet::new();
        let mut bit_depths: BTreeSet<u64> = BTreeSet::new();
        let mut languages: BTreeSet<String> = BTreeSet::new();

        for pos in 0..ac {
            if let Some(s) = streams.stream(StreamKind::Audio, pos) {
                if let Some(f) = s.get("Format") {
                    codecs.insert(f.as_str().to_owned());
                }
                if let Some(ch) = s.get("Channels") {
                    if let Ok(cv) = ch.as_str().parse::<u64>() {
                        channels.insert(cv);
                    }
                }
                if let Some(sr) = s.get("SamplingRate") {
                    if let Ok(srv) = sr.as_str().parse::<u64>() {
                        sample_rates.insert(srv);
                    }
                }
                if let Some(bd) = s.get("BitDepth") {
                    if let Ok(bv) = bd.as_str().parse::<u64>() {
                        bit_depths.insert(bv);
                    }
                }
                if let Some(lang) = s.get("Language") {
                    languages.insert(lang.as_str().to_owned());
                }
            }
        }

        let codec_list: Vec<&str> = codecs.iter().map(|s| s.as_str()).collect();
        out.push_str(&format!("  Codec(s): {}\n", codec_list.join(", ")));

        let ch_list: Vec<String> = channels.iter().map(|c| c.to_string()).collect();
        out.push_str(&format!("  Channel(s): {}\n", ch_list.join(", ")));

        let sr_list: Vec<String> = sample_rates.iter().map(|s| format!("{} Hz", s)).collect();
        out.push_str(&format!("  Sample rate(s): {}\n", sr_list.join(", ")));

        for bd in &bit_depths {
            out.push_str(&format!("  Bit depth: {} bit\n", bd));
        }

        if !languages.is_empty() {
            let lang_list: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();
            out.push_str(&format!("  Language(s): {}\n", lang_list.join(", ")));
        }

        out.push('\n');
    }

    // Text summary
    if tc > 0 {
        out.push_str("Text\n");
        out.push_str("----\n");

        let mut formats: BTreeSet<String> = BTreeSet::new();
        let mut languages: BTreeSet<String> = BTreeSet::new();

        for pos in 0..tc {
            if let Some(s) = streams.stream(StreamKind::Text, pos) {
                if let Some(f) = s.get("Format") {
                    formats.insert(f.as_str().to_owned());
                }
                if let Some(lang) = s.get("Language") {
                    languages.insert(lang.as_str().to_owned());
                }
            }
        }

        let fmt_list: Vec<&str> = formats.iter().map(|s| s.as_str()).collect();
        out.push_str(&format!("  Format(s): {}\n", fmt_list.join(", ")));

        if !languages.is_empty() {
            let lang_list: Vec<&str> = languages.iter().map(|s| s.as_str()).collect();
            out.push_str(&format!("  Language(s): {}\n", lang_list.join(", ")));
        }

        out.push('\n');
    }

    // Image summary
    if ic > 0 {
        out.push_str("Image\n");
        out.push_str("-----\n");

        let mut formats: BTreeSet<String> = BTreeSet::new();
        let mut resolutions: Vec<String> = Vec::new();

        for pos in 0..ic {
            if let Some(s) = streams.stream(StreamKind::Image, pos) {
                if let Some(f) = s.get("Format") {
                    formats.insert(f.as_str().to_owned());
                }
                let w = s.get("Width").and_then(|v| v.as_str().parse::<u64>().ok());
                let h = s.get("Height").and_then(|v| v.as_str().parse::<u64>().ok());
                if let (Some(wi), Some(hi)) = (w, h) {
                    resolutions.push(format!("{}x{}", wi, hi));
                }
            }
        }

        let fmt_list: Vec<&str> = formats.iter().map(|s| s.as_str()).collect();
        out.push_str(&format!("  Format(s): {}\n", fmt_list.join(", ")));

        if !resolutions.is_empty() {
            let r_list: Vec<&str> = resolutions.iter().map(|s| s.as_str()).collect();
            out.push_str(&format!("  Resolution(s): {}\n", r_list.join(", ")));
        }

        out.push('\n');
    }

    out
}

fn human_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["Bytes", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", size, UNITS[unit])
    }
}

fn human_duration(ms: i64) -> String {
    if ms < 1000 {
        return format!("{} ms", ms);
    }
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;
    let rem_secs = secs % 60;
    let rem_mins = mins % 60;
    if hours > 0 {
        format!("{} h {} m {} s", hours, rem_mins, rem_secs)
    } else if mins > 0 {
        format!("{} m {} s", mins, rem_secs)
    } else {
        format!("{} s", secs)
    }
}

fn human_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.2} Mb/s", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.2} kb/s", bps as f64 / 1_000.0)
    } else {
        format!("{} b/s", bps)
    }
}
