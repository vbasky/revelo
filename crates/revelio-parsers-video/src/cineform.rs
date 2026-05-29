use revelio_core::{FileAnalyze, StreamKind};
/// Parse GoPro CineForm wavelet intermediate codec.
///
/// Detection: CFHD magic.
/// Fills: Resolution, encoding type.
pub fn parse_cineform(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    if &buf[0..4] != b"CFHD" {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, pos, "Format", "CineForm");
    fa.set_field(StreamKind::Video, pos, "Format_Info", "GoPro CineForm wavelet");
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"CFHD".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_cineform(&mut fa));
    }
}
