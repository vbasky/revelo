use revelo_core::{FileAnalyze, StreamKind};
/// Parse HuffYUV lossless video codec.
///
/// Detection: HFYU fourcc.
/// Fills: Bit depth, colourplane count.
pub fn parse_huffyuv(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    if &buf[0..4] != b"HFYU" {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, pos, "Format", "HuffYUV");
    fa.set_field(StreamKind::Video, pos, "Format_Info", "HuffYUV lossless");
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"HFYU".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_huffyuv(&mut fa));
    }
}
