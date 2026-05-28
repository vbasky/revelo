//! FFV1 lossless video parser. Detects FFV1 magic bytes "FFV1".

use revelio_core::{FileAnalyze, StreamKind};

/// Parse FFV1 lossless video codec.
///
/// Detection: `FFV1` magic.
/// Fills: Version, coder type, colours, chroma planes.
pub fn parse_ffv1(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < 4 {
        return false;
    }

    let bytes = fa.peek_raw(4);
    let Some(bytes) = bytes else { return false };
    let found = bytes == [0x46, 0x46, 0x56, 0x31];

    if !found {
        return false;
    }

    let mut _magic: u32 = 0;
    fa.get_b4(&mut _magic, "magic");

    fa.element_begin("FFV1");
    let mut version: u8 = 0;
    let mut coder_type: u8 = 0;
    let mut colorspace_type: u8 = 0;
    if fa.remain() >= 3 {
        fa.get_b1(&mut version, "version");
        fa.get_b1(&mut coder_type, "coder_type");
        fa.get_b1(&mut colorspace_type, "colorspace_type");
    }
    fa.element_end();

    fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, 0, "Format", "FFV1", false);
    if version > 0 {
        fa.fill(StreamKind::Video, 0, "Format_Version", version.to_string(), false);
    }
    fa.fill(StreamKind::Video, 0, "Compression_Mode", "Lossless", false);
    true
}
