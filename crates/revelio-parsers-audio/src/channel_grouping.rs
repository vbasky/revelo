use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_channel_grouping(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    let magic = std::str::from_utf8(&buf[0..4]).unwrap_or("");
    if magic != "CHGR" { return false; }
    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "Channel Grouping", false);
    fa.fill(StreamKind::Audio, pos, "Format_Info", "Multi-stream channel grouping", false);
    true
}
#[cfg(test)] mod tests { use super::*; use revelio_core::FileAnalyze;
    #[test] fn test_group() { let buf = b"CHGR\x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_channel_grouping(&mut fa)); }
}
