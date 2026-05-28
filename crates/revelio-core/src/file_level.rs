//! File-level field derivation shared by every front-end (CLI, diff
//! harness, future C ABI shim).
//!
//! Some `General`-stream fields are not derivable from the media bytes
//! alone — they come from the path / fstat / cross-stream arithmetic.
//! On the C++ side these are filled by `MediaInfo_Internal` (the wrapper
//! that drives the parser). Previously this logic lived only in the diff
//! harness, so the CLI emitted incomplete output (no FileSize,
//! OverallBitRate, Duration, FileExtension, dates, or StreamSize
//! overhead). Centralising it here keeps every front-end consistent.

use crate::{FileAnalyze, StreamKind};

/// Inputs the engine can't read from the media stream itself. The caller
/// (which has filesystem access) supplies them.
pub struct FileLevelInfo<'a> {
    /// Total file size in bytes (from fstat).
    pub file_size: u64,
    /// Lowercase-or-as-is file extension, e.g. "mp4" (no leading dot).
    pub extension: Option<&'a str>,
    /// File modification time as a Unix timestamp (seconds), if known.
    pub modified_unix_secs: Option<i64>,
    /// Local timezone offset in seconds east of UTC, for the
    /// `_Local` date variant (e.g. +36000 for AEST).
    pub local_offset_secs: i64,
}

/// Fill the derived `General`-stream fields. Mirrors what the C++
/// `MediaInfo_Internal` wrapper does after a parser finishes.
pub fn fill_file_level_fields(fa: &mut FileAnalyze, info: &FileLevelInfo<'_>) {
    let file_size = info.file_size;
    fa.Fill(StreamKind::General, 0, "FileSize", file_size.to_string(), false);

    if let Some(ext) = info.extension {
        fa.Fill(StreamKind::General, 0, "FileExtension", ext.to_owned(), false);
    }

    let audio_stream_size: Option<u64> = fa
        .Retrieve(StreamKind::Audio, 0, "StreamSize")
        .and_then(|z| z.as_str().parse().ok());

    // Prefer a parser-filled General.Duration (always integer ms).
    // Fall back to Audio.Duration for parsers that only emit there in
    // int-ms form. A float Audio.Duration (e.g. MKV "1.500000000")
    // parses as None here — fine, since those parsers also fill
    // General.Duration.
    let duration_ms: Option<u64> = fa
        .Retrieve(StreamKind::General, 0, "Duration")
        .and_then(|z| z.as_str().parse().ok())
        .or_else(|| {
            fa.Retrieve(StreamKind::Audio, 0, "Duration")
                .and_then(|z| z.as_str().parse().ok())
        });

    if let Some(ms) = duration_ms {
        fa.Fill(StreamKind::General, 0, "Duration", ms.to_string(), false);
    }

    // OverallBitRate = FileSize × 8 × 1000 / Duration_ms, rounded to
    // nearest. OverallBitRate_Mode:
    //   * audio-only file: mirror Audio.BitRate_Mode
    //   * video present: "VBR" only when Video is VFR (authored MP4s
    //     with VBR AAC inside CFR video omit the field, matching oracle)
    if let Some(ms) = duration_ms {
        if ms > 0 {
            let overall = ((file_size as f64) * 8.0 * 1000.0 / (ms as f64)).round() as u64;
            let has_video = fa.Count_Get(StreamKind::Video) > 0;
            let overall_mode = if !has_video {
                fa.Retrieve(StreamKind::Audio, 0, "BitRate_Mode")
                    .map(|z| z.as_str().to_owned())
            } else {
                let video_fr_mode = fa
                    .Retrieve(StreamKind::Video, 0, "FrameRate_Mode")
                    .map(|z| z.as_str().to_owned());
                if video_fr_mode.as_deref() == Some("VFR") {
                    Some("VBR".to_owned())
                } else {
                    None
                }
            };
            if let Some(mode) = overall_mode {
                fa.Fill(StreamKind::General, 0, "OverallBitRate_Mode", mode, false);
            }
            fa.Fill(StreamKind::General, 0, "OverallBitRate", overall.to_string(), false);
        }
    }

    // Propagate the primary video stream's frame rate + total frame
    // count up to the General stream — MediaInfo surfaces them at the
    // container level (e.g. big_buck_bunny General FrameRate=24.000,
    // FrameCount=1440). Only when a video track exists and the parser
    // hasn't already set them on General.
    if fa.Count_Get(StreamKind::Video) > 0 {
        if fa.Retrieve(StreamKind::General, 0, "FrameRate").is_none() {
            if let Some(fr) = fa
                .Retrieve(StreamKind::Video, 0, "FrameRate")
                .map(|z| z.as_str().to_owned())
            {
                fa.Fill(StreamKind::General, 0, "FrameRate", fr, false);
            }
        }
        if fa.Retrieve(StreamKind::General, 0, "FrameCount").is_none() {
            if let Some(fc) = fa
                .Retrieve(StreamKind::Video, 0, "FrameCount")
                .map(|z| z.as_str().to_owned())
            {
                fa.Fill(StreamKind::General, 0, "FrameCount", fc, false);
            }
        }
    }

    // General StreamSize = container overhead = FileSize − elementary
    // stream sizes. Skipped when elementary ≥ file_size (e.g. Ogg/Vorbis
    // reports a bitrate-derived StreamSize larger than the file).
    let video_stream_size: u64 = fa
        .Retrieve(StreamKind::Video, 0, "StreamSize")
        .and_then(|z| z.as_str().parse().ok())
        .unwrap_or(0);
    if let Some(audio_size) = audio_stream_size {
        let elementary = audio_size + video_stream_size;
        if elementary < file_size {
            fa.Fill(StreamKind::General, 0, "StreamSize", (file_size - elementary).to_string(), false);
        }
    } else if video_stream_size > 0 && video_stream_size < file_size {
        fa.Fill(StreamKind::General, 0, "StreamSize", (file_size - video_stream_size).to_string(), false);
    }

    if let Some(unix_secs) = info.modified_unix_secs {
        fa.Fill(
            StreamKind::General,
            0,
            "File_Modified_Date",
            format_utc(unix_secs),
            false,
        );
        fa.Fill(
            StreamKind::General,
            0,
            "File_Modified_Date_Local",
            format_local(unix_secs, info.local_offset_secs),
            false,
        );
    }
}

fn format_utc(unix_secs: i64) -> String {
    let (y, m, d, hh, mm, ss) = civil_from_unix(unix_secs);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC")
}

fn format_local(unix_secs: i64, local_offset_secs: i64) -> String {
    let (y, m, d, hh, mm, ss) = civil_from_unix(unix_secs + local_offset_secs);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02}")
}

/// Unix timestamp → (year, month, day, hour, min, sec) via Howard
/// Hinnant's `days_from_civil` algorithm (proleptic Gregorian).
fn civil_from_unix(unix_secs: i64) -> (i32, u8, u8, u8, u8, u8) {
    let days = unix_secs.div_euclid(86400);
    let rem = unix_secs.rem_euclid(86400);
    let hh = (rem / 3600) as u8;
    let mm = ((rem % 3600) / 60) as u8;
    let ss = (rem % 60) as u8;

    let z = days + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let year = if m <= 2 { y + 1 } else { y } as i32;
    (year, m, d, hh, mm, ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_filesize_and_extension() {
        let mut fa = FileAnalyze::new(b"");
        fa.Stream_Prepare(StreamKind::General);
        let info = FileLevelInfo {
            file_size: 12548,
            extension: Some("jpg"),
            modified_unix_secs: None,
            local_offset_secs: 0,
        };
        fill_file_level_fields(&mut fa, &info);
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "FileSize").map(|z| z.as_str().to_owned()).as_deref(),
            Some("12548")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "FileExtension").map(|z| z.as_str().to_owned()).as_deref(),
            Some("jpg")
        );
    }

    #[test]
    fn propagates_video_framerate_and_framecount_to_general() {
        let mut fa = FileAnalyze::new(b"");
        fa.Stream_Prepare(StreamKind::General);
        fa.Fill(StreamKind::Video, 0, "FrameRate", "24.000", false);
        fa.Fill(StreamKind::Video, 0, "FrameCount", "1440", false);
        let info = FileLevelInfo {
            file_size: 1,
            extension: None,
            modified_unix_secs: None,
            local_offset_secs: 0,
        };
        fill_file_level_fields(&mut fa, &info);
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "FrameRate").map(|z| z.as_str().to_owned()).as_deref(),
            Some("24.000")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "FrameCount").map(|z| z.as_str().to_owned()).as_deref(),
            Some("1440")
        );
    }

    #[test]
    fn does_not_propagate_framerate_without_video() {
        let mut fa = FileAnalyze::new(b"");
        fa.Stream_Prepare(StreamKind::General);
        fa.Fill(StreamKind::Audio, 0, "Format", "AAC", false);
        let info = FileLevelInfo {
            file_size: 1,
            extension: None,
            modified_unix_secs: None,
            local_offset_secs: 0,
        };
        fill_file_level_fields(&mut fa, &info);
        assert!(fa.Retrieve(StreamKind::General, 0, "FrameRate").is_none());
    }

    #[test]
    fn computes_overall_bitrate_from_duration() {
        let mut fa = FileAnalyze::new(b"");
        fa.Stream_Prepare(StreamKind::General);
        fa.Fill(StreamKind::General, 0, "Duration", "1000", false);
        let info = FileLevelInfo {
            file_size: 125_000, // 125000 bytes over 1 s = 1_000_000 bps
            extension: None,
            modified_unix_secs: None,
            local_offset_secs: 0,
        };
        fill_file_level_fields(&mut fa, &info);
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "OverallBitRate").map(|z| z.as_str().to_owned()).as_deref(),
            Some("1000000")
        );
    }

    #[test]
    fn formats_modified_date_utc_and_local() {
        let mut fa = FileAnalyze::new(b"");
        fa.Stream_Prepare(StreamKind::General);
        // 2021-01-01 00:00:00 UTC = 1609459200
        let info = FileLevelInfo {
            file_size: 1,
            extension: None,
            modified_unix_secs: Some(1_609_459_200),
            local_offset_secs: 3600, // +01:00
        };
        fill_file_level_fields(&mut fa, &info);
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "File_Modified_Date").map(|z| z.as_str().to_owned()).as_deref(),
            Some("2021-01-01 00:00:00 UTC")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "File_Modified_Date_Local").map(|z| z.as_str().to_owned()).as_deref(),
            Some("2021-01-01 01:00:00")
        );
    }
}
