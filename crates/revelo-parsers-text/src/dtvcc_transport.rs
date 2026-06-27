use revelo_core::{FileAnalyze, StreamKind};
/// Parse CTA-708 DTVCC transport stream.
///
/// Detection: CC_type_1 0x03 0x00.
/// Fills: Format.
pub fn parse_dtvcc_transport(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(2) else { return false };
    if buf[0] == 0x03 && buf[1] == 0x00 {
        // CC_type_1 in CTA-708
        let pos = fa.stream_prepare(StreamKind::Text);
        fa.set_field(StreamKind::Text, pos, "Format", "DTVCC");
        fa.set_field(StreamKind::Text, pos, "Format_Info", "Digital Television Closed Captioning");
        return true;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = vec![0x03, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dtvcc_transport(&mut fa));
    }
}
