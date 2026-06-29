use revelo_core::{FileAnalyze, StreamKind};
/// Parse Apple Intermediate Codec.
///
/// Detection: aic/AIC fourcc.
/// Fills: Dimensions.
pub fn parse_aic(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(8) else { return false };
    if &buf[4..8] == b"aic " || &buf[0..4] == b"AIC " {
        let pos = fa.stream_prepare(StreamKind::Video);
        fa.set_field(StreamKind::Video, pos, "Format", "Apple Intermediate Codec");
        fa.set_field(StreamKind::Video, pos, "Format_Info", "AIC");
        return true;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let mut buf = vec![0u8; 8];
        buf[4..8].copy_from_slice(b"aic ");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aic(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 8);
    }
}
