use revelio_core::{FileAnalyze, StreamKind};

/// WebVTT subtitle parser. Detects the "WEBVTT" magic string followed by
/// optional header metadata.
pub fn parse_webvtt(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.Remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 6 { return false; }

    let magic = std::str::from_utf8(&buf[0..6]).unwrap_or("");
    if magic != "WEBVTT" { return false; }

    let pos = fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, pos, "Format", "WebVTT", false);
    fa.Fill(StreamKind::Text, pos, "MuxingMode", "WebVTT", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webvtt_detects_magic() {
        let buf = b"WEBVTT\n\n00:00:00.000 --> 00:00:01.000\nHello\n".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_webvtt(&mut fa));
        assert_eq!(fa.Retrieve(StreamKind::Text, 0, "Format").map(|z| z.as_str().to_owned()), Some("WebVTT".into()));
    }

    #[test]
    fn webvtt_rejects_garbage() {
        let buf = b"Not a WebVTT file".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_webvtt(&mut fa));
    }
}
