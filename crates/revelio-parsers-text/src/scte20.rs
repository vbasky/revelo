use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_scte20(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.Remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] == b"SCTE" || &buf[0..4] == b"scte" {
        let pos = fa.Stream_Prepare(StreamKind::Text);
        fa.Fill(StreamKind::Text, pos, "Format", "SCTE-20", false);
        fa.Fill(StreamKind::Text, pos, "Format_Info", "SCTE 20 closed captioning", false);
        return true;
    }
    false
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"SCTE\x00\x00\x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_scte20(&mut fa)); }
}
