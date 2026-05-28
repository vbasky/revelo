use revelio_core::{FileAnalyze, StreamKind};
pub fn parse_ps2_audio(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.Remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 8 { return false; }
    let magic = &buf[0..4];
    if magic != b"SSmp" && magic != b"VSab" && magic != b"VSaf" { return false; }
    let pos = fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, pos, "Format", "PS2 Audio", false);
    fa.Fill(StreamKind::Audio, pos, "Format_Info", "PlayStation 2 ADPCM", false);
    true
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test_ps2() { let buf = b"SSmp\x00\x00\x00\x00".to_vec(); let mut fa = FileAnalyze::new(&buf); assert!(parse_ps2_audio(&mut fa)); }
}
