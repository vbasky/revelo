//! H.263 video parser. Extracts source format (CIF/QCIF etc.) from picture start codes.

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_h263(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < 4 {
        return false;
    }

    let mut psc: u32 = 0;
    fa.Peek_B4(&mut psc);

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

    fa.Skip_B4("picture start code");

    fa.Element_Begin("H.263");
    fa.Element_End();

    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "H.263", false);
    fa.Fill(StreamKind::Video, 0, "Width", w.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Height", h.to_string(), false);
    true
}
