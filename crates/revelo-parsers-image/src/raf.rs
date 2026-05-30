use revelo_core::{FileAnalyze, StreamKind};

/// Fujifilm RAF (Raw) format parser.
///
/// RAF files start with "FUJIFILM" at offset 0, followed by:
///   bytes 8-11:  format version (e.g. "0x0100")
///   bytes 12-13: camera model ID
pub fn parse_raf(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw_at(0, 12) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 12 {
        return false;
    }
    if &buf[0..8] != b"FUJIFILM" {
        return false;
    }

    fa.set_field(StreamKind::General, 0, "Format", "RAF");
    fa.set_field(StreamKind::General, 0, "Format_Commercial", "Fujifilm RAW");
    true
}
