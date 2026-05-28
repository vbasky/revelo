//! H.263 video parser. Extracts source format (CIF/QCIF etc.) from picture start codes.

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_h263(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < 4 {
        return false;
    }

    let mut psc: u32 = 0;
    fa.peek_b4(&mut psc);

    if (psc >> 10) != 0x000080 {
        return false;
    }

    let source_format = (psc >> 3) & 0x07;

    let (w, h): (u32, u32) = match source_format {
        1 => (128, 96),
        2 => (176, 144),
        3 => (352, 288),
        4 => (704, 576),
        5 => (1408, 1152),
        _ => return false,
    };

    fa.skip_b4("picture start code");

    fa.element_begin("H.263");
    fa.element_end();

    fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, 0, "Format", "H.263", false);
    fa.fill(StreamKind::Video, 0, "Width", w.to_string(), false);
    fa.fill(StreamKind::Video, 0, "Height", h.to_string(), false);
    true
}
