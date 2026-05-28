use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_adm(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    if &buf[0..4] != b"ADM " && !buf.windows(4).any(|w| w == b"axml") {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "ADM", false);
    fa.fill(StreamKind::Audio, pos, "Format_Info", "SMPTE ST 2076 Audio Definition Model", false);
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"ADM \x00\x00\x00\x00".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_adm(&mut fa));
    }
}
