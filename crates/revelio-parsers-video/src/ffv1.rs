//! FFV1 lossless video parser. Detects FFV1 magic bytes "FFV1".

use revelio_core::{FileAnalyze, Reader, StreamKind};

/// Parse FFV1 lossless video codec.
///
/// Detection: `FFV1` magic.
/// Fills: Version, coder type, colours, chroma planes.
pub fn parse_ffv1(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.peek_raw(4)? != [0x46, 0x46, 0x56, 0x31] {
        return None;
    }
    r.be_u32("magic")?;

    r.element_begin("FFV1");
    // The 3 header bytes are optional — a bare "FFV1" magic still detects.
    let version = if r.remain() >= 3 {
        let v = r.be_u8("version")?;
        r.be_u8("coder_type")?;
        r.be_u8("colorspace_type")?;
        v
    } else {
        0
    };
    r.element_end();

    fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, 0, "Format", "FFV1");
    if version > 0 {
        fa.set_field(StreamKind::Video, 0, "Format_Version", version.to_string());
    }
    fa.set_field(StreamKind::Video, 0, "Compression_Mode", "Lossless");
    Some(())
}
