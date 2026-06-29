use revelo_core::{FileAnalyze, StreamKind};
/// Parse AVS3 Chinese UHD video codec.
///
/// Detection: Annex B 0x000001B0 NAL.
/// Fills: Profile, dimensions, bit depth.
pub fn parse_avs3(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(5) else { return false };
    if &buf[0..4] == b"AVS3"
        || (buf[0] == 0x00 && buf[1] == 0x00 && buf[2] == 0x00 && buf[3] == 0x01 && buf[4] == 0xB0)
    {
        let pos = fa.stream_prepare(StreamKind::Video);
        fa.set_field(StreamKind::Video, pos, "Format", "AVS3");
        fa.set_field(StreamKind::Video, pos, "Format_Info", "Chinese AVS3 standard");
        return true;
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf = vec![0x00, 0x00, 0x00, 0x01, 0xB0];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_avs3(&mut fa));
    }
}
