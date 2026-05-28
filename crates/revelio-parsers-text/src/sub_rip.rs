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
//! We only fill format metadata here — full event/duration accounting can be
//! added later, parallel to the C++ side.

use revelio_core::{FileAnalyze, StreamKind};

const DETECT_WINDOW: usize = 512;

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
    let body = if head.len() >= 3 && head[0] == 0xEF && head[1] == 0xBB && head[2] == 0xBF {
        &head[3..]
    } else {
        head
    };

    if !looks_like_subrip(body) {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "SubRip", false);

    fa.stream_prepare(StreamKind::Text);
    fa.fill(StreamKind::Text, 0, "Format", "SubRip", false);
    fa.fill(StreamKind::Text, 0, "Codec", "SubRip", false);

    true
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
}
