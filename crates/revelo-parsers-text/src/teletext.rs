use revelo_core::{FileAnalyze, StreamKind};

/// Parse Teletext subtitle.
///
/// Detection: Clock run-in 0x55 0x55 + framing code 0x27.
/// Fills: Format, page info.
pub fn parse_teletext(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(3) {
        Some(b) => b,
        None => return false,
    };

    // Teletext sync: 0x55 0x55 0x27
    if buf[0] != 0x55 || buf[1] != 0x55 || buf[2] != 0x27 {
        return false;
    }

    // The clock run-in + framing code confirm teletext; fill the format fields
    // whether or not the full 45-byte packet is present. (Previously a partial
    // packet returned `true` with no fields set, leaving the stream
    // inconsistent for callers that act on the return value.)
    let pos = fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, pos, "Format", "Teletext");
    fa.set_field(StreamKind::Text, pos, "MuxingMode", "Teletext");

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_core::FileAnalyze;

    #[test]
    fn teletext_detects_sync_bytes() {
        let mut buf = vec![0u8; 45];
        buf[0] = 0x55;
        buf[1] = 0x55;
        buf[2] = 0x27;
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_teletext(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("Teletext".into())
        );
    }

    #[test]
    fn teletext_rejects_garbage() {
        let buf = vec![0u8; 45];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_teletext(&mut fa));
    }

    #[test]
    fn teletext_does_not_request_full_payload() {
        let mut buf = vec![0u8; 1024 * 1024];
        buf[0] = 0x55;
        buf[1] = 0x55;
        buf[2] = 0x27;
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_teletext(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 3);
    }
}
