use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_avs3(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.Remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] == b"AVS3" || (buf.len() >= 5 && buf[0] == 0x00 && buf[1] == 0x00 && buf[2] == 0x00 && buf[3] == 0x01 && buf[4] == 0xB0) {
        let pos = fa.Stream_Prepare(StreamKind::Video);
        fa.Fill(StreamKind::Video, pos, "Format", "AVS3", false);
        fa.Fill(StreamKind::Video, pos, "Format_Info", "Chinese AVS3 standard", false);
        return true;
    }
    false
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = vec![0x00, 0x00, 0x00, 0x01, 0xB0]; let mut fa = FileAnalyze::new(&buf); assert!(parse_avs3(&mut fa)); }
}
