//! CD-ROM XA (Video CD) container parser.
//!
//! Subset of MediaInfoLib's `File_Cdxa.cpp`. CDXA is a RIFF wrapper around
//! raw CD-ROM XA Mode 2 sectors (2352 bytes each) that typically carry an
//! interleaved MPEG-PS payload — the layout used by Video CD discs.
//!
//! Magic / 44-byte header layout (all multi-byte ints are little-endian
//! except the 4CCs, which are stored as raw ASCII):
//!   0x00  "RIFF"                4 bytes
//!   0x04  RIFF size             4 bytes (LE; == data_size + 0x24)
//!   0x08  "CDXA"                4 bytes  (form type — vs WAV's "WAVE")
//!   0x0C  "fmt "                4 bytes
//!   0x10  fmt size              4 bytes (LE; == 0x10)
//!   0x14  user_id               2 bytes
//!   0x16  group_id              2 bytes
//!   0x18  attributes            2 bytes
//!   0x1A  "XA"                  2 bytes  (xa_signature)
//!   0x1C  xa_track_number       4 bytes
//!   0x20  reserved              4 bytes
//!   0x24  "data"                4 bytes
//!   0x28  data size             4 bytes (LE)
//!
//! We only identify the container — the per-sector MPEG-PS demux is left
//! to a later pass.

use revelio_core::{FileAnalyze, StreamKind};

const CDXA_HEADER_SIZE: usize = 0x2C;

pub fn parse_cdxa(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain().min(CDXA_HEADER_SIZE)) {
        Some(b) if b.len() >= CDXA_HEADER_SIZE => b,
        _ => return false,
    };

    // Mirrors the C++ FileHeader_Begin checks in File_Cdxa.cpp.
    if &buf[0x00..0x04] != b"RIFF"
        || &buf[0x08..0x0C] != b"CDXA"
        || &buf[0x0C..0x10] != b"fmt "
        || u32::from_le_bytes([buf[0x10], buf[0x11], buf[0x12], buf[0x13]]) != 0x10
        || &buf[0x1A..0x1C] != b"XA"
        || &buf[0x24..0x28] != b"data"
    {
        return false;
    }

    // Cross-check the size relationship: riff_size == data_size + 0x24.
    let riff_size = u32::from_le_bytes([buf[0x04], buf[0x05], buf[0x06], buf[0x07]]);
    let data_size = u32::from_le_bytes([buf[0x28], buf[0x29], buf[0x2A], buf[0x2B]]);
    if riff_size != data_size.wrapping_add(0x24) {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "CDXA", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cdxa_header(data_size: u32) -> Vec<u8> {
        let mut buf = Vec::with_capacity(CDXA_HEADER_SIZE);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(data_size + 0x24).to_le_bytes()); // riff size
        buf.extend_from_slice(b"CDXA");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&0x10u32.to_le_bytes()); // fmt size
        buf.extend_from_slice(&0u16.to_le_bytes()); // user_id
        buf.extend_from_slice(&0u16.to_le_bytes()); // group_id
        buf.extend_from_slice(&0u16.to_le_bytes()); // attributes
        buf.extend_from_slice(b"XA"); // xa_signature
        buf.extend_from_slice(&0u32.to_le_bytes()); // xa_track_number
        buf.extend_from_slice(&0u32.to_le_bytes()); // reserved
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf
    }

    #[test]
    fn non_riff_buffer_returns_false() {
        let mut fa = FileAnalyze::new(b"This is not a CDXA file at all, just text....");
        assert!(!parse_cdxa(&mut fa));
    }

    #[test]
    fn riff_wave_returns_false() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&36u32.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.resize(CDXA_HEADER_SIZE + 8, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_cdxa(&mut fa));
    }

    #[test]
    fn minimal_cdxa_header_detected() {
        let buf = make_cdxa_header(2352);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_cdxa(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("CDXA")
        );
    }
}
