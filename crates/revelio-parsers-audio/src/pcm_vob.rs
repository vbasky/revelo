use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_pcm_vob(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    if &buf[0..4] != b"DVD " && !buf.windows(4).any(|w| w == b"LPCM") { return false; }
    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "PCM", false);
    fa.fill(StreamKind::Audio, pos, "Format_Info", "VOB LPCM", false);
    fa.fill(StreamKind::Audio, pos, "Format_Settings_Endianness", "Big", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf = b"DVD \x00\x00\x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_pcm_vob(&mut fa)); }
}
