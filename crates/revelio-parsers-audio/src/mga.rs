use revelio_core::{FileAnalyze, StreamKind};
/// Parse MPEG-4 General Audio container.
///
/// Detection: `MGA` magic.
/// Fills: Format.
pub fn parse_mga(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] != b"MGA " { return false; }
    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "MPEG Audio", false);
    fa.fill(StreamKind::Audio, pos, "Format_Info", "MPEG-4 General Audio", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"MGA \x00\x00\x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_mga(&mut fa)); }
}
