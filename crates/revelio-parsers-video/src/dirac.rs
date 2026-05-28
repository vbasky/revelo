use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_dirac(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.Remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] != b"BBCD" { return false; }
    let pos = fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, pos, "Format", "Dirac", false);
    fa.Fill(StreamKind::Video, pos, "Format_Info", "BBC Dirac wavelet", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"BBCD\x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_dirac(&mut fa)); }
}
