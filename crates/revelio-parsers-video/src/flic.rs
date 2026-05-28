use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_flic(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 6 { return false; }
    let _file_size = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let magic = u16::from_le_bytes([buf[4], buf[5]]);
    if magic != 0xAF11 && magic != 0xAF12 { return false; }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, pos, "Format", "FLIC", false);
    fa.fill(StreamKind::Video, pos, "Format_Info", "Autodesk Animator", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf: Vec<u8> = vec![0x08, 0x00, 0x00, 0x00, 0x11, 0xAF]; let mut fa = FileAnalyze::new(&buf); assert!(parse_flic(&mut fa)); }
}
