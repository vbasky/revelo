use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_dolby_audio_metadata(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 8 {
        return false;
    }
    if &buf[0..4] == b"RIFF" {
        let form = &buf[8..12];
        if form == b"DAM " || form == b"DAMG" {
            let pos = fa.stream_prepare(StreamKind::Audio);
            fa.fill(StreamKind::Audio, pos, "Format", "Dolby Audio Metadata", false);
            fa.fill(StreamKind::Audio, pos, "Format_Info", "DAM", false);
            return true;
        }
    }
    if &buf[0..4] == b"DAM " {
        let pos = fa.stream_prepare(StreamKind::Audio);
        fa.fill(StreamKind::Audio, pos, "Format", "Dolby Audio Metadata", false);
        return true;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = b"RIFF\x00\x00\x00\x00DAM ".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dolby_audio_metadata(&mut fa));
    }
}
