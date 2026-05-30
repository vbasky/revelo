use revelo_core::{FileAnalyze, StreamKind};

/// Canon CR2 (Raw v2) format parser.
///
/// CR2 is a TIFF variant with a 16-byte header:
///   bytes 0-1:   "II" (little-endian)
///   bytes 2-3:   0x002A (TIFF magic)
///   bytes 4-7:   IFD0 offset (typically 0x10)
///   bytes 8-9:   "CR" (Canon Raw identifier)
///   bytes 10:    major version (2)
///   bytes 11:    minor version (0)
///   bytes 12-15: RAW IFD offset
pub fn parse_cr2(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw_at(0, 16) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 16 {
        return false;
    }
    if &buf[0..2] != b"II" {
        return false;
    }
    if buf[2..4] != [42, 0] {
        return false;
    }
    if &buf[8..10] != b"CR" {
        return false;
    }

    let major = buf[10];
    let minor = buf[11];
    let ver = format!("{}.{}", major, minor);

    fa.set_field(StreamKind::General, 0, "Format", "CR2");
    fa.set_field(StreamKind::General, 0, "Format_Version", ver.as_str());
    true
}
