use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_avs(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] != b"AVS " { return false; }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, pos, "Format", "AVS", false);
    fa.fill(StreamKind::Video, pos, "Format_Info", "Chinese AVS standard", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"AVS \x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_avs(&mut fa)); }
}
