use revelo_core::{FileAnalyze, StreamKind};
/// Parse PAC subtitle format.
///
/// Detection: `PAC` magic.
/// Fills: Format.
pub fn parse_pac(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    if &buf[0..4] != b"PAC " && &buf[0..4] != b"PAC\x00" {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, pos, "Format", "PAC");
    fa.set_field(StreamKind::Text, pos, "Format_Info", "PAC subtitle format");
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"PAC \x00\x00\x00\x00".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pac(&mut fa));
    }
}
