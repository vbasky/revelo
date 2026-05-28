use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_canopus(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 8 { return false; }
    let magic = &buf[4..8];
    let canopus = [b"CHQX", b"CHQX", b"CHQH", b"CHQS", b"CHQL", b"CHQX"];
    if !canopus.iter().any(|m| &m[..] == magic) { return false; }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, pos, "Format", "Canopus HQ", false);
    fa.fill(StreamKind::Video, pos, "Format_Info", "Grass Valley HQX", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let mut buf = vec![0u8; 8]; buf[4..8].copy_from_slice(b"CHQX"); let mut fa = FileAnalyze::new(&buf); assert!(parse_canopus(&mut fa)); }
}
