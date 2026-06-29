use revelo_core::{FileAnalyze, StreamKind};

/// Musepack SV8 parser. Detects "MPCK" magic (SV8 stream version marker).
/// SV7 is handled by the existing mpc.rs parser.
pub fn parse_mpc_sv8(fa: &mut FileAnalyze) -> bool {
    let magic = match fa.peek_raw(4) {
        Some(buf) => buf,
        None => return false,
    };
    if magic != b"MPCK" {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, pos, "Format", "Musepack");
    fa.set_field(StreamKind::Audio, pos, "Format_Version", "SV8");
    fa.set_field(StreamKind::Audio, pos, "Format_Info", "Musepack SV8");
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
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format_Version").map(|z| z.as_str().to_owned()),
            Some("SV8".into())
        );
    }

    #[test]
    fn mpc_sv8_does_not_request_full_payload() {
        let mut buf = vec![0u8; 1024 * 1024];
        buf[0..4].copy_from_slice(b"MPCK");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpc_sv8(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 4);
    }
}
