use super::stream::{StreamCollection, StreamKind};
use zenlib::Ztring;

pub fn fill_computed_fields(sc: &mut StreamCollection) {
    fill_bits_pixel_frame(sc);
    fill_compression_ratio(sc);
    fill_format_profile_general(sc);
    fill_frame_rate_mode_original(sc);
    fill_bitrate_ranges(sc);
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
