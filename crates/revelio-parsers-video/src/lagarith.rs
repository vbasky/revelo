use revelio_core::{FileAnalyze, StreamKind};
/// Parse Lagarith lossless video codec.
///
/// Detection: LAGS fourcc.
/// Fills: Dimensions, bit depth.
pub fn parse_lagarith(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    if &buf[0..4] != b"LAGS" {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, pos, "Format", "Lagarith");
    fa.set_field(StreamKind::Video, pos, "Format_Info", "Lagarith lossless");
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"LAGS".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_lagarith(&mut fa));
    }
}
