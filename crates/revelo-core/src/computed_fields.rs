use super::stream::{StreamCollection, StreamKind};
use revelo_util::Ztring;

pub fn fill_computed_fields(sc: &mut StreamCollection) {
    fill_bits_pixel_frame(sc);
    fill_compression_ratio(sc);
    fill_format_profile_general(sc);
    fill_frame_rate_mode_original(sc);
    fill_bitrate_ranges(sc);
    fill_inferred_av1_level(sc);
}

fn field_val(sc: &StreamCollection, kind: StreamKind, pos: usize, key: &str) -> Option<String> {
    sc.stream(kind, pos).and_then(|s| s.get(key)).map(|z| z.as_str().to_string())
}

fn fill_bits_pixel_frame(sc: &mut StreamCollection) {
    let n = sc.stream_count(StreamKind::Video);
    for i in 0..n {
        let w: f64 = field_val(sc, StreamKind::Video, i, "Width")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let h: f64 = field_val(sc, StreamKind::Video, i, "Height")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let fr: f64 = field_val(sc, StreamKind::Video, i, "FrameRate")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let br: f64 = field_val(sc, StreamKind::Video, i, "BitRate")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        if w > 0.0 && h > 0.0 && fr > 0.0 && br > 0.0 {
            let bpp = br / (w * h * fr);
            sc.set_field(
                StreamKind::Video,
                i,
                "Bits_Pixel_Frame",
                Ztring::from(format!("{:.3}", bpp)),
            );
        }
    }
}

fn fill_compression_ratio(sc: &mut StreamCollection) {
    for kind in &[StreamKind::Video, StreamKind::Audio] {
        let n = sc.stream_count(*kind);
        for i in 0..n {
            let stream_size: u64 =
                field_val(sc, *kind, i, "StreamSize").and_then(|v| v.parse().ok()).unwrap_or(0);
            let dur_s: f64 =
                field_val(sc, *kind, i, "Duration").and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let channels: u64 =
                field_val(sc, *kind, i, "Channels").and_then(|v| v.parse().ok()).unwrap_or(1);
            let bit_depth: u64 =
                field_val(sc, *kind, i, "BitDepth").and_then(|v| v.parse().ok()).unwrap_or(8);
            let sr: u64 =
                field_val(sc, *kind, i, "SamplingRate").and_then(|v| v.parse().ok()).unwrap_or(0);
            let is_audio = matches!(kind, StreamKind::Audio);
            let uncompressed: u64 = if is_audio && sr > 0 && dur_s > 0.0 {
                (channels * sr * bit_depth * (dur_s * 1000.0) as u64) / 8 / 1000
            } else if dur_s > 0.0 {
                let br: u64 =
                    field_val(sc, *kind, i, "BitRate").and_then(|v| v.parse().ok()).unwrap_or(0);
                if br > 0 { (br * (dur_s * 1000.0) as u64) / 8 / 1000 } else { 0 }
            } else {
                0
            };
            if stream_size > 0 && uncompressed > 0 {
                let ratio = uncompressed as f64 / stream_size as f64;
                sc.set_field(*kind, i, "Compression_Ratio", Ztring::from(format!("{:.3}", ratio)));
            }
        }
    }
}

fn fill_bitrate_ranges(sc: &mut StreamCollection) {
    // OverallBitRate_Maximum from the sum of all per-stream BitRate_Maximum values.
    // OverallBitRate_Minimum from BitRate_Minimum.
    // BitRate_Minimum from stream-level delta (if not already filled by parser).
    let mut overall_max: u64 = 0;
    let mut overall_min: u64 = u64::MAX;
    for kind in &[StreamKind::Audio, StreamKind::Video] {
        let n = sc.stream_count(*kind);
        for i in 0..n {
            if let Some(max_str) = field_val(sc, *kind, i, "BitRate_Maximum")
                && let Ok(v) = max_str.parse::<u64>()
            {
                overall_max += v;
            }
            if let Some(br_str) = field_val(sc, *kind, i, "BitRate")
                && let Ok(v) = br_str.parse::<u64>()
                && v < overall_min
            {
                overall_min = v;
            }
            // Fill BitRate_Minimum if empty
            if field_val(sc, *kind, i, "BitRate_Minimum").is_none()
                && let Some(br_str) = field_val(sc, *kind, i, "BitRate")
                && let Ok(v) = br_str.parse::<u64>()
            {
                sc.set_field(*kind, i, "BitRate_Minimum", Ztring::from(format!("{}", v / 2)));
            }
        }
    }
    if overall_max > 0 && field_val(sc, StreamKind::General, 0, "OverallBitRate_Maximum").is_none()
    {
        sc.set_field(
            StreamKind::General,
            0,
            "OverallBitRate_Maximum",
            Ztring::from(format!("{}", overall_max)),
        );
    }
    if overall_min < u64::MAX
        && field_val(sc, StreamKind::General, 0, "OverallBitRate_Minimum").is_none()
    {
        sc.set_field(
            StreamKind::General,
            0,
            "OverallBitRate_Minimum",
            Ztring::from(format!("{}", overall_min)),
        );
    }
}

fn fill_format_profile_general(sc: &mut StreamCollection) {
    let format = field_val(sc, StreamKind::General, 0, "Format").unwrap_or_default();
    let kind = field_val(sc, StreamKind::General, 0, "CodecID").unwrap_or_default();
    if format == "MPEG-4" {
        let profile = match kind.as_str() {
            "mp42" => Some("Base Media / Version 2"),
            "isom" => Some("Base Media / Version 1"),
            "avc1" => Some("Base Media"),
            _ => None,
        };
        if let Some(p) = profile {
            sc.set_field(StreamKind::General, 0, "Format_Profile", Ztring::from(p));
        }
    }
}

fn fill_frame_rate_mode_original(sc: &mut StreamCollection) {
    // Set FrameRate_Mode_Original from the first Video FR mode before any CFR override.
    let n = sc.stream_count(StreamKind::Video);
    for i in 0..n {
        if let Some(mode) = field_val(sc, StreamKind::Video, i, "FrameRate_Mode") {
            sc.set_field(StreamKind::Video, i, "FrameRate_Mode_Original", Ztring::from(mode));
        }
    }
}

/// Infer the minimum AV1 level required for a given resolution and framerate,
/// then set `Format_Level_Inferred` if it differs from the reported level.
///
/// AV1 level table (section A.3 — example values for the L5 tier maximums):
///
/// | Level | Max resolution @ 30fps | Max resolution @ 60fps |
/// |-------|------------------------|------------------------|
/// | 2.0   | 426×240                | 426×240                |
/// | 2.1   | 640×360                | 640×360                |
/// | 3.0   | 854×480                | 854×480                |
/// | 3.1   | 1280×720               | 1280×720               |
/// | 4.0   | 1920×1080              | 1920×1080 @ 30         |
/// | 4.1   | 1920×1080              | 1920×1080 @ 60         |
/// | 5.0   | 3840×2160              | 3840×2160 @ 30         |
/// | 5.1   | 3840×2160              | 3840×2160 @ 60         |
/// | 5.2   | 3840×2160              | 3840×2160 @ 120        |
/// | 5.3   | 3840×2160              | 3840×2160 @ 200        |
/// | 6.0   | 7680×4320              | 7680×4320 @ 30         |
/// | 6.1   | 7680×4320              | 7680×4320 @ 60         |
/// | 6.2   | 7680×4320              | 7680×4320 @ 120        |
/// | 6.3   | 7680×4320              | 7680×4320 @ 200        |
fn fill_inferred_av1_level(sc: &mut StreamCollection) {
    let n = sc.stream_count(StreamKind::Video);
    for i in 0..n {
        let format = field_val(sc, StreamKind::Video, i, "Format").unwrap_or_default();
        if format != "AV1" {
            continue;
        }
        let Some(profile_str) = field_val(sc, StreamKind::Video, i, "Format_Profile") else {
            continue;
        };
        let width: u64 =
            match field_val(sc, StreamKind::Video, i, "Width").and_then(|v| v.parse().ok()) {
                Some(w) => w,
                None => continue,
            };
        let height: u64 =
            match field_val(sc, StreamKind::Video, i, "Height").and_then(|v| v.parse().ok()) {
                Some(h) => h,
                None => continue,
            };
        let frame_rate: f64 =
            match field_val(sc, StreamKind::Video, i, "FrameRate").and_then(|v| v.parse().ok()) {
                Some(f) => f,
                None => continue,
            };

        let max_dim = width.max(height);
        let inferred_level_idx = av1_inferred_level_idx(max_dim, frame_rate);

        if let Some(inferred) = inferred_level_idx {
            // Only emit if it differs from the reported level (extract level from @L suffix)
            let reported_level_idx =
                profile_str.split("@L").nth(1).and_then(|s| s.parse::<u8>().ok());

            if reported_level_idx != Some(inferred) {
                let level_name = av1_level_name(inferred);
                sc.set_field(
                    StreamKind::Video,
                    i,
                    "Format_Level_Inferred",
                    Ztring::from(level_name),
                );
            }
        }
    }
}

/// Return the minimum AV1 level index that supports `max_dim` at `fps`.
fn av1_inferred_level_idx(max_dim: u64, fps: f64) -> Option<u8> {
    // AV1 level limits from spec Table A.3 (Main tier, pic_size in pixels × 1M, samples per sec × 1M).
    let levels: &[(u8, u64, u64)] = &[
        (0, 147_456, 4_915_200),         // 2.0
        (1, 262_144, 8_847_360),         // 2.1
        (2, 491_520, 16_515_072),        // 3.0
        (3, 1_048_576, 35_389_440),      // 3.1
        (4, 2_088_960, 69_632_000),      // 4.0
        (5, 2_088_960, 139_264_000),     // 4.1
        (6, 8_912_896, 297_096_000),     // 5.0
        (7, 8_912_896, 594_192_000),     // 5.1
        (8, 8_912_896, 1_188_384_000),   // 5.2
        (9, 8_912_896, 1_980_640_000),   // 5.3
        (10, 35_651_584, 1_188_384_000), // 6.0
        (11, 35_651_584, 2_376_768_000), // 6.1
        (12, 35_651_584, 4_753_536_000), // 6.2
        (13, 35_651_584, 7_922_560_000), // 6.3
    ];

    // Round dimensions up to nearest multiple of 16 (AV1 superblock alignment)
    let w16 = ((max_dim + 15) / 16) * 16;
    let h16 = (((max_dim as f64 * 9.0 / 16.0).ceil() as u64 + 15) / 16) * 16;
    let pic_size = w16 * h16;
    let samples_per_sec = (pic_size as f64 * fps) as u64;

    for &(idx, max_pic, max_sps) in levels {
        if pic_size <= max_pic && samples_per_sec <= max_sps {
            return Some(idx);
        }
    }

    let pic_size = (max_dim as f64 * max_dim as f64 * 9.0 / 16.0).round() as u64;
    let samples_per_sec = pic_size as f64 * fps;

    for &(idx, max_pic, max_sps) in levels {
        if pic_size <= max_pic && (samples_per_sec as u64) <= max_sps {
            return Some(idx);
        }
    }

    None
}

fn av1_level_name(idx: u8) -> &'static str {
    match idx {
        0 => "2.0",
        1 => "2.1",
        2 => "3.0",
        3 => "3.1",
        4 => "4.0",
        5 => "4.1",
        6 => "5.0",
        7 => "5.1",
        8 => "5.2",
        9 => "5.3",
        10 => "6.0",
        11 => "6.1",
        12 => "6.2",
        13 => "6.3",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bpp() {
        let mut sc = StreamCollection::new();
        sc.stream_prepare(StreamKind::Video);
        sc.set_field(StreamKind::Video, 0, "Width", Ztring::from("1920"));
        sc.set_field(StreamKind::Video, 0, "Height", Ztring::from("1080"));
        sc.set_field(StreamKind::Video, 0, "FrameRate", Ztring::from("25.000"));
        sc.set_field(StreamKind::Video, 0, "BitRate", Ztring::from("5000000"));
        fill_computed_fields(&mut sc);
        assert_eq!(field_val(&sc, StreamKind::Video, 0, "Bits_Pixel_Frame").unwrap(), "0.096");
    }
    #[test]
    fn test_compression_ratio() {
        let mut sc = StreamCollection::new();
        sc.stream_prepare(StreamKind::Audio);
        sc.set_field(StreamKind::Audio, 0, "StreamSize", Ztring::from("1000"));
        sc.set_field(StreamKind::Audio, 0, "Duration", Ztring::from("1.000"));
        sc.set_field(StreamKind::Audio, 0, "Channels", Ztring::from("2"));
        sc.set_field(StreamKind::Audio, 0, "BitDepth", Ztring::from("16"));
        sc.set_field(StreamKind::Audio, 0, "SamplingRate", Ztring::from("44100"));
        fill_computed_fields(&mut sc);
        assert!(field_val(&sc, StreamKind::Audio, 0, "Compression_Ratio").is_some());
    }
    #[test]
    fn test_format_profile_general() {
        let mut sc = StreamCollection::new();
        sc.stream_prepare(StreamKind::General);
        sc.set_field(StreamKind::General, 0, "Format", Ztring::from("MPEG-4"));
        sc.set_field(StreamKind::General, 0, "CodecID", Ztring::from("mp42"));
        fill_computed_fields(&mut sc);
        assert_eq!(
            field_val(&sc, StreamKind::General, 0, "Format_Profile").unwrap(),
            "Base Media / Version 2"
        );
    }
}
