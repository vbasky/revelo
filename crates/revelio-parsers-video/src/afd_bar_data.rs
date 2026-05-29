use revelio_core::{FileAnalyze, StreamKind};
/// Parse AFD/Bar data (SMPTE 2016-1).
/// Fills: Format, active format descriptor.
pub fn parse_afd_bar_data(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    if &buf[0..4] == b"AFBd" || &buf[0..4] == b"BARD" {
        let pos = fa.stream_prepare(StreamKind::Video);
        fa.set_field(StreamKind::Video, pos, "Format", "AFD/Bar Data");
        fa.set_field(StreamKind::Video, pos, "Format_Info", "SMPTE 2016-1");
        return true;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"AFBd".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_afd_bar_data(&mut fa));
    }
}
