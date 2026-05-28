use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_afd_bar_data(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] == b"AFBd" || &buf[0..4] == b"BARD" {
        let pos = fa.stream_prepare(StreamKind::Video);
        fa.fill(StreamKind::Video, pos, "Format", "AFD/Bar Data", false);
        fa.fill(StreamKind::Video, pos, "Format_Info", "SMPTE 2016-1", false);
        return true;
    }
    false
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"AFBd".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_afd_bar_data(&mut fa)); }
}
