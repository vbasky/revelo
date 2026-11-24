//! SubRip (.srt) subtitle parser.
//!
//! Mirrors MediaInfoLib's `File_SubRip.cpp` detection path. A SubRip file is
//! a sequence of cues with the shape:
//!
//!   <counter>\n
//!   HH:MM:SS,mmm --> HH:MM:SS,mmm\n
//!   <one or more lines of text>\n
//!   \n
//!
//! Detection accepts an optional UTF-8 BOM and either LF or CRLF line endings.
//! Event and duration counts read at most 1 MiB. The reject path stops after
//! 512 bytes so a large non-SubRip file is not scanned.

use revelo_core::{FileAnalyze, StreamKind};

const DETECT_WINDOW: usize = 512;
/// Upper bound for the post-detection cue walk. A bare `peek_raw` of the
/// whole remainder would fault a large file during detection.
const CUE_SCAN_LIMIT: usize = 1024 * 1024;

pub fn parse_sub_rip(fa: &mut FileAnalyze) -> bool {
    let want = fa.remain().min(DETECT_WINDOW);
    if want < 16 {
        return false;
    }
    let head = match fa.peek_raw(want) {
        Some(b) => b,
        None => return false,
    };

    // Skip UTF-8 BOM so the first textual line is reachable.
    let body = strip_bom(head);

    if !looks_like_subrip(body) {
        return false;
    }
    // `head` borrows `fa`. The short branch reuses that window; the long
    // branch peeks a capped span so cues past the detection window count.
    let scan = fa.remain().min(CUE_SCAN_LIMIT);
    let short = scan <= head.len();
    let (events, duration_ms) = if short {
        cue_stats(body)
    } else {
        let Some(all) = fa.peek_raw(scan) else {
            return false;
        };
        cue_stats(strip_bom(all))
    };

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "SubRip");

    fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, 0, "Format", "SubRip");
    fa.set_field(StreamKind::Text, 0, "Codec", "SubRip");
    if events > 0 {
        fa.set_field(StreamKind::Text, 0, "Events_Total", events.to_string());
        fa.set_field(StreamKind::Text, 0, "Duration", duration_ms.to_string());
        fa.set_field(StreamKind::General, 0, "Duration", duration_ms.to_string());
    }

    true
}

fn strip_bom(head: &[u8]) -> &[u8] {
    if head.len() >= 3 && head[0] == 0xEF && head[1] == 0xBB && head[2] == 0xBF {
        &head[3..]
    } else {
        head
    }
}

fn cue_stats(buf: &[u8]) -> (u32, u64) {
    let mut events = 0u32;
    let mut last_end = 0u64;
    for line in buf.split(|&b| b == b'\n') {
        let line = trim_ascii(line);
        if let Some(end) = cue_end_ms(line) {
            events += 1;
            last_end = last_end.max(end);
        }
    }
    (events, last_end)
}

fn cue_end_ms(line: &[u8]) -> Option<u64> {
    if !is_timecode_line(line) {
        return None;
    }
    let sep = find_subseq(line, b" --> ")?;
    timestamp_ms(&line[sep + 5..sep + 17])
}

fn timestamp_ms(ts: &[u8]) -> Option<u64> {
    if !is_subrip_timestamp(ts) {
        return None;
    }
    let h = parse_digits(&ts[0..2])?;
    let m = parse_digits(&ts[3..5])?;
    let s = parse_digits(&ts[6..8])?;
    let ms = parse_digits(&ts[9..12])?;
    Some((h * 3600 + m * 60 + s) * 1000 + ms)
}

fn parse_digits(bytes: &[u8]) -> Option<u64> {
    let mut n = 0u64;
    for b in bytes {
        n = n * 10 + u64::from(b.wrapping_sub(b'0'));
    }
    Some(n)
}

fn looks_like_subrip(buf: &[u8]) -> bool {
    let mut lines = buf.split(|&b| b == b'\n');

    // First non-empty line must be a pure-digit counter (commonly "1").
    let counter = loop {
        match lines.next() {
            Some(line) => {
                let trimmed = trim_ascii(line);
                if trimmed.is_empty() {
                    continue;
                }
                break trimmed;
            }
            None => return false,
        }
    };
    if counter.is_empty() || !counter.iter().all(|b| b.is_ascii_digit()) {
        return false;
    }

    // Next non-empty line must be a SubRip timecode arrow.
    loop {
        match lines.next() {
            Some(line) => {
                let trimmed = trim_ascii(line);
                if trimmed.is_empty() {
                    continue;
                }
                return is_timecode_line(trimmed);
            }
            None => return false,
        }
    }
}

fn trim_ascii(mut s: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = s {
        if first.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = s {
        if last.is_ascii_whitespace() {
            s = rest;
        } else {
            break;
        }
    }
    s
}

fn is_timecode_line(line: &[u8]) -> bool {
    // Minimum "HH:MM:SS,mmm --> HH:MM:SS,mmm" = 29 bytes.
    if line.len() < 29 {
        return false;
    }
    // Find " --> " separator; the C++ parser uses the same anchor.
    let sep = match find_subseq(line, b" --> ") {
        Some(i) => i,
        None => return false,
    };
    is_subrip_timestamp(&line[..sep]) && is_subrip_timestamp_prefix(&line[sep + 5..])
}

fn find_subseq(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

fn is_subrip_timestamp(ts: &[u8]) -> bool {
    // Strict form "HH:MM:SS,mmm" or "HH:MM:SS.mmm" — 12 bytes, fixed positions.
    if ts.len() != 12 {
        return false;
    }
    let d = |i: usize| ts[i].is_ascii_digit();
    d(0) && d(1)
        && ts[2] == b':'
        && d(3)
        && d(4)
        && ts[5] == b':'
        && d(6)
        && d(7)
        && (ts[8] == b',' || ts[8] == b'.')
        && d(9)
        && d(10)
        && d(11)
}

fn is_subrip_timestamp_prefix(ts: &[u8]) -> bool {
    // End-timestamp may be followed by VTT-style positioning hints, so we
    // only validate the leading 12-byte timestamp portion.
    if ts.len() < 12 {
        return false;
    }
    is_subrip_timestamp(&ts[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"1\n00:00:01,000 --> 00:00:04,000\nHello world\n\n2\n00:00:05,000 --> 00:00:07,500\nSecond cue\n";

    #[test]
    fn parses_minimal_subrip() {
        let mut fa = FileAnalyze::new(SAMPLE);
        assert!(parse_sub_rip(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("SubRip"));
        assert_eq!(t("Format").as_deref(), Some("SubRip"));
        assert_eq!(t("Codec").as_deref(), Some("SubRip"));
        assert_eq!(t("Events_Total").as_deref(), Some("2"));
        assert_eq!(t("Duration").as_deref(), Some("7500"));
        assert_eq!(g("Duration").as_deref(), Some("7500"));
    }

    #[test]
    fn parses_subrip_with_bom_and_crlf() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
        buf.extend_from_slice(b"1\r\n00:00:01,000 --> 00:00:04,000\r\nHi\r\n");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_sub_rip(&mut fa));
        let t = |k: &str| fa.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(t("Format").as_deref(), Some("SubRip"));
    }

    #[test]
    fn rejects_non_subrip_buffer() {
        let mut fa = FileAnalyze::new(b"WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHello\n");
        // WebVTT starts with a header, not a numeric counter — must reject.
        assert!(!parse_sub_rip(&mut fa));

        let mut fa2 =
            FileAnalyze::new(b"This is just a plain text file, no timecodes here at all.\n");
        assert!(!parse_sub_rip(&mut fa2));
    }

    #[test]
    fn counts_cues_past_the_detection_window() {
        let mut buf = b"1\n00:00:01,000 --> 00:00:02,000\n".to_vec();
        buf.extend(std::iter::repeat_n(b'x', 600));
        buf.extend_from_slice(b"\n\n2\n00:00:03,000 --> 00:00:09,000\nLater\n");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_sub_rip(&mut fa));
        let t = |k: &str| fa.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(t("Events_Total").as_deref(), Some("2"));
        assert_eq!(t("Duration").as_deref(), Some("9000"));
        assert!(fa.access_stats().max_request_len > DETECT_WINDOW);
        assert!(fa.access_stats().max_request_len <= CUE_SCAN_LIMIT);
    }

    #[test]
    fn reject_path_stops_at_the_detection_window() {
        let mut buf = b"This is just prose.\n".to_vec();
        buf.resize(4096, b'a');
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_sub_rip(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, DETECT_WINDOW);
    }

    #[test]
    fn cue_scan_is_capped() {
        let mut buf = b"1\n00:00:01,000 --> 00:00:02,000\nHi\n".to_vec();
        buf.resize(CUE_SCAN_LIMIT + 64, b'\n');
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_sub_rip(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, CUE_SCAN_LIMIT);
    }
}
