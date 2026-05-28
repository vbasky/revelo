use revelio_core::{FileAnalyze, StreamKind};
/// Parse CTA-708 DTVCC transport stream.
///
/// Detection: CC_type_1 0x03 0x00.
/// Fills: Format.
pub fn parse_dtvcc_transport(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 2 { return false; }
    if buf[0] == 0x03 && buf[1] == 0x00 { // CC_type_1 in CTA-708
        let pos = fa.stream_prepare(StreamKind::Text);
        fa.fill(StreamKind::Text, pos, "Format", "DTVCC", false);
        fa.fill(StreamKind::Text, pos, "Format_Info", "Digital Television Closed Captioning", false);
        return true;
    }
    false
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = vec![0x03, 0x00]; let mut fa = FileAnalyze::new(&buf); assert!(parse_dtvcc_transport(&mut fa)); }
}
