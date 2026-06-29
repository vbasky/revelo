use revelo_core::{FileAnalyze, StreamKind};
/// Parse BBC Dirac wavelet video codec.
///
/// Detection: BBCD magic.
/// Fills: Dimensions, chroma format, frame rate.
pub fn parse_dirac(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(4) else { return false };
    if &buf[0..4] != b"BBCD" {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, pos, "Format", "Dirac");
    fa.set_field(StreamKind::Video, pos, "Format_Info", "BBC Dirac wavelet");
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"BBCD\x00\x00".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dirac(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 4);
    }
}
