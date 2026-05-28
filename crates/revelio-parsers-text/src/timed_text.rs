use revelio_core::{FileAnalyze, StreamKind};

/// Parse 3GPP/MOV Timed Text track.
///
/// Detection: 16-bit BE length + UTF-8/UTF-16 text.
/// Fills: Format.
pub fn parse_timed_text(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain() as usize) {
        Some(b) => b,
        None => return false,
    };

    if buf.len() < 2 {
        return false;
    }

    // 3GPP Timed Text: 16-bit big-endian length prefix followed by UTF-8/UTF-16 text
    let size = u16::from_be_bytes([buf[0], buf[1]]) as usize;
    if size == 0 || size > buf.len().saturating_sub(2) {
        return false;
    }

    let text_data = &buf[2..2 + size.min(buf.len() - 2)];
    if text_data.is_empty() {
        return false;
    }

    // Check it's valid UTF-8 text
    if std::str::from_utf8(text_data).is_err() {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Text);
    fa.fill(StreamKind::Text, pos, "Format", "Timed Text", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn timed_text_binary_length_prefix() {
        let text = b"Hello World";
        let mut buf = vec![0u8; 2 + text.len()];
        buf[0] = 0;
        buf[1] = text.len() as u8;
        buf[2..].copy_from_slice(text);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_timed_text(&mut fa));
    }

    #[test]
    fn timed_text_rejects_binary() {
        let buf = vec![0xFF, 0xFE, 0x00, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_timed_text(&mut fa));
    }
}
