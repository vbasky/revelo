use revelio_core::{FileAnalyze, StreamKind};

/// Musepack SV8 parser. Detects "MPCK" magic (SV8 stream version marker).
/// SV7 is handled by the existing mpc.rs parser.
pub fn parse_mpc_sv8(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.Remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }

    let magic = &buf[0..4];
    if magic != b"MPCK" { return false; }

    let pos = fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, pos, "Format", "Musepack", false);
    fa.Fill(StreamKind::Audio, pos, "Format_Version", "SV8", false);
    fa.Fill(StreamKind::Audio, pos, "Format_Info", "Musepack SV8", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mpc_sv8_detects_magic() {
        let buf: Vec<u8> = vec![0x4D, 0x50, 0x43, 0x4B]; // MPCK
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpc_sv8(&mut fa));
        assert_eq!(fa.Retrieve(StreamKind::Audio, 0, "Format_Version").map(|z| z.as_str().to_owned()), Some("SV8".into()));
    }
}
