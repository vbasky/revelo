//! H.263 video parser. Extracts source format (CIF/QCIF etc.) from picture start codes.

use revelio_core::{FileAnalyze, Reader, StreamKind};

/// Parse H.263 video codec.
///
/// Detection: Picture start code 0x000080 bits.
/// Fills: Source format, custom picture dimensions.
pub fn parse_h263(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    let psc = r.peek_be_u32()?;
    if (psc >> 10) != 0x000080 {
        return None;
    }

    let source_format = (psc >> 3) & 0x07;
    let (w, h): (u32, u32) = match source_format {
        1 => (128, 96),
        2 => (176, 144),
        3 => (352, 288),
        4 => (704, 576),
        5 => (1408, 1152),
        _ => return None,
    };

    r.be_u32("picture start code")?;
    r.element_begin("H.263");
    r.element_end();

    fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, 0, "Format", "H.263");
    fa.set_field(StreamKind::Video, 0, "Width", w.to_string());
    fa.set_field(StreamKind::Video, 0, "Height", h.to_string());
    Some(())
}
