//! NSV (Nullsoft Streaming Video) container parser.
//!
//! Mirrors detection performed by `File_Nsv.cpp::FileHeader_Parse`. The NSV
//! stream begins with either of two 4-byte tags:
//!   - `"NSVf"` — file header with size/duration/metadata/TOC info
//!   - `"NSVs"` — sync header (mid-stream entry point)
//!
//! We only recognise the format and emit `General.Format=NSV`; codec-level
//! detail (video/audio fourcc, framerate, dimensions) lives inside `NSVs`
//! sync frames and is intentionally not parsed here.

use revelo_core::{FileAnalyze, StreamKind};

const MAGIC_NSVF: [u8; 4] = *b"NSVf";
const MAGIC_NSVS: [u8; 4] = *b"NSVs";

pub fn parse_nsv(fa: &mut FileAnalyze) -> bool {
    // Only the first four bytes are load-bearing for recognition; the
    // NSVf header coherency checks done by File_Nsv.cpp need >=28 bytes,
    // but matching the magic alone is sufficient for Format identification.
    let header = match fa.peek_raw(fa.remain().min(4)) {
        Some(b) if b.len() >= 4 => b,
        _ => return false,
    };
    let magic = [header[0], header[1], header[2], header[3]];
    if magic != MAGIC_NSVF && magic != MAGIC_NSVS {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "NSV");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nsvf_magic() {
        // 28-byte NSVf header with zeroed fields — only the magic matters
        // for Format recognition at this layer.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"NSVf");
        buf.extend_from_slice(&[0u8; 24]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_nsv(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("NSV")
        );
    }

    #[test]
    fn parses_nsvs_sync_magic() {
        // Streams cut mid-flight begin at an NSVs sync frame rather than
        // an NSVf header; both must be recognised as NSV.
        let buf = b"NSVs\x00\x00\x00\x00";
        let mut fa = FileAnalyze::new(buf);
        assert!(parse_nsv(&mut fa));
    }

    #[test]
    fn rejects_non_nsv_buffer() {
        let mut fa = FileAnalyze::new(b"RIFFxxxxAVI ");
        assert!(!parse_nsv(&mut fa));
    }

    #[test]
    fn rejects_truncated_buffer() {
        let mut fa = FileAnalyze::new(b"NSV");
        assert!(!parse_nsv(&mut fa));
    }
}
