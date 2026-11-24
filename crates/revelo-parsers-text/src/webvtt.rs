use revelo_core::{FileAnalyze, StreamKind};

/// Cue counts stay inside one bounded read. A bare `peek_raw` of the whole
/// remainder would fault a large file during detection.
const CUE_SCAN_LIMIT: usize = 1024 * 1024;

/// WebVTT subtitle parser. Detects the "WEBVTT" magic string followed by
/// optional header metadata.
/// Parse WebVTT subtitle format.
///
/// Detection: `WEBVTT\n` header.
/// Fills: Track info, cue blocks.
pub fn parse_webvtt(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(6) else { return false };

    let magic = std::str::from_utf8(&buf[0..6]).unwrap_or("");
    if magic != "WEBVTT" {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, pos, "Format", "WebVTT");
    fa.set_field(StreamKind::Text, pos, "MuxingMode", "WebVTT");

    if let Some(all) = fa.peek_raw(fa.remain().min(CUE_SCAN_LIMIT)) {
        let (events, duration_ms) = vtt_stats(all);
        if events > 0 {
            fa.set_field(StreamKind::Text, pos, "Events_Total", events.to_string());
            fa.set_field(StreamKind::Text, pos, "Duration", duration_ms.to_string());
            fa.stream_prepare(StreamKind::General);
            fa.set_field(StreamKind::General, 0, "Duration", duration_ms.to_string());
        }
    }

    true
}

fn vtt_stats(buf: &[u8]) -> (u32, u64) {
    let mut events = 0u32;
    let mut last_end = 0u64;
    for line in buf.split(|&b| b == b'\n') {
        let line = trim_ascii(line);
        if let Some(end) = vtt_cue_end(line) {
            events += 1;
            last_end = last_end.max(end);
        }
    }
    (events, last_end)
}

fn vtt_cue_end(line: &[u8]) -> Option<u64> {
    let sep = find_subseq(line, b" --> ")?;
    let start = trim_ascii(&line[..sep]);
    let end_side = trim_ascii(&line[sep + 5..]);
    let end = end_side.split(|b| b.is_ascii_whitespace()).next().unwrap_or(end_side);
    if vtt_timestamp_ms(start).is_none() {
        return None;
    }
    vtt_timestamp_ms(end)
}

fn vtt_timestamp_ms(ts: &[u8]) -> Option<u64> {
    if ts.len() == 12 && ts[2] == b':' && ts[5] == b':' && (ts[8] == b'.' || ts[8] == b',') {
        let h = digits(&ts[0..2])?;
        let m = digits(&ts[3..5])?;
        let s = digits(&ts[6..8])?;
        let ms = digits(&ts[9..12])?;
        return Some((h * 3600 + m * 60 + s) * 1000 + ms);
    }
    if ts.len() == 9 && ts[2] == b':' && (ts[5] == b'.' || ts[5] == b',') {
        let m = digits(&ts[0..2])?;
        let s = digits(&ts[3..5])?;
        let ms = digits(&ts[6..9])?;
        return Some((m * 60 + s) * 1000 + ms);
    }
    None
}

fn digits(bytes: &[u8]) -> Option<u64> {
    if !bytes.iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut n = 0u64;
    for b in bytes {
        n = n * 10 + u64::from(b - b'0');
    }
    Some(n)
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

fn find_subseq(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webvtt_detects_magic() {
        let buf = b"WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_webvtt(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("WebVTT".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Events_Total").map(|z| z.as_str().to_owned()),
            Some("1".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Duration").map(|z| z.as_str().to_owned()),
            Some("1000".into())
        );
        assert_eq!(fa.access_stats().max_request_len, buf.len());
    }

    #[test]
    fn webvtt_rejects_garbage() {
        let buf = b"Not a WebVTT file".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_webvtt(&mut fa));
    }
}
