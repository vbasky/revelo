use revelio_core::{FileAnalyze, StreamKind};
/// Parse Session Description Protocol.
///
/// Detection: `v=0` + `m=` lines.
/// Fills: Format, session info.
pub fn parse_sdp(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    let text = std::str::from_utf8(&buf).unwrap_or("");
    if !text.starts_with("v=0") || !text.contains("m=") { return false; }
    let pos = fa.stream_prepare(StreamKind::Text);
    fa.fill(StreamKind::Text, pos, "Format", "SDP", false);
    fa.fill(StreamKind::Text, pos, "Format_Info", "Session Description Protocol", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=Test\r\nm=audio 0 RTP/AVP 0".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_sdp(&mut fa)); }
}
