use revelio_core::{FileAnalyze, StreamKind};
/// Parse PDF document.
///
/// Detection: `%PDF-` magic.
/// Fills: Format.
pub fn parse_pdf(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 5 {
        return false;
    }
    if &buf[0..5] != b"%PDF-" {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, pos, "Format", "PDF");
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"%PDF-1.4".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pdf(&mut fa));
    }
}
