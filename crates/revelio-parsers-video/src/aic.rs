use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_aic(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[4..8] == b"aic " || &buf[0..4] == b"AIC " {
        let pos = fa.stream_prepare(StreamKind::Video);
        fa.fill(StreamKind::Video, pos, "Format", "Apple Intermediate Codec", false);
        fa.fill(StreamKind::Video, pos, "Format_Info", "AIC", false);
        return true;
    }
    false
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let mut buf = vec![0u8; 8]; buf[4..8].copy_from_slice(b"aic "); let mut fa = FileAnalyze::new(&buf); assert!(parse_aic(&mut fa)); }
}
