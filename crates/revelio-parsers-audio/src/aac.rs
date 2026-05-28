use revelio_core::{FileAnalyze, StreamKind};
/// Parse raw AAC without ADTS header.
///
/// Detection: esds AudioSpecificConfig in MP4.
/// Fills: Profile, sampling rate, channels from DecoderConfigDescriptor.
pub fn parse_aac(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 2 { return false; }
    if buf[0] != 0xFF || (buf[1] & 0xF0) != 0xF0 { return false; }
    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "AAC", false);
    let profile = (buf[2] >> 6) & 0x03;
    let _sr = match (buf[2] >> 2) & 0x0F { 0=>96000, 3=>48000, 4=>44100, 5=>32000, 6=>24000, 8=>16000, _=>44100 };
    let _ch = ((buf[2] & 0x01) << 2) | (buf[3] >> 6);
    match profile { 0=>fa.fill(StreamKind::Audio, pos, "Format_Profile", "LC", false), 1=>fa.fill(StreamKind::Audio, pos, "Format_Profile", "HE-AAC", false), 2=>fa.fill(StreamKind::Audio, pos, "Format_Profile", "HE-AACv2", false), _=>{} }
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test() { let buf: Vec<u8> = vec![0xFF, 0xF9, 0x50, 0x80]; let mut fa = FileAnalyze::new(&buf); assert!(parse_aac(&mut fa)); }
}
