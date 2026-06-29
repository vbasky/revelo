use revelo_core::{FileAnalyze, StreamKind};

/// CELT ultra-low-delay audio codec parser. Detects "CELT" magic in
/// identification header or CELT frames.
pub fn parse_celt(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(8) else { return false };

    // CELT identification header starts with "CELT" + version
    let magic = std::str::from_utf8(&buf[0..4]).unwrap_or("");
    if magic != "CELT" {
        return false;
    }

    let bitstream_version = buf[4] as u32;
    let channels = buf[5] as u32;
    let mode = buf[7] as u32;

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, pos, "Format", "CELT");
    fa.set_field(StreamKind::Audio, pos, "Format_Info", "Ultra-low-delay codec");
    fa.set_field(StreamKind::Audio, pos, "Format_Version", format!("{}", bitstream_version));
    fa.set_field(StreamKind::Audio, pos, "Channels", channels.to_string());
    fa.set_field(StreamKind::Audio, pos, "Format_Settings_Mode", mode.to_string());

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn celt_detects_magic() {
        let buf: Vec<u8> = b"CELT\x00\x02\x00\x00".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_celt(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 8);
    }
}
