use revelio_core::{FileAnalyze, StreamKind};
/// Parse Chinese HDR Vivid metadata.
///
/// Detection: HDRV/HVIV magic.
/// Fills: HDR metadata fields.
pub fn parse_hdr_vivid(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] == b"HDRV" || &buf[0..4] == b"HVIV" {
        let pos = fa.stream_prepare(StreamKind::Video);
        fa.fill(StreamKind::Video, pos, "Format", "HDR Vivid", false);
        fa.fill(StreamKind::Video, pos, "HDR_Format", "HDR Vivid", false);
        fa.fill(StreamKind::Video, pos, "Format_Info", "Chinese HDR Vivid", false);
        return true;
    }
    false
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"HDRV\x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_hdr_vivid(&mut fa)); }
}
