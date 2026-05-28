//! Dolby AC-4 parser — sync-based.
//!
//! Frame layout:
//!   0xAC40 or 0xAC41 (big-endian)   sync word
//!   The low bit of the sync word selects between two frame variants
//!   (with/without trailing CRC-16).

use revelio_core::{FileAnalyze, StreamKind};

const AC4_SYNC_0: [u8; 2] = [0xAC, 0x40];
const AC4_SYNC_1: [u8; 2] = [0xAC, 0x41];

pub fn parse_ac4(fa: &mut FileAnalyze) -> bool {
    let n = fa.remain().min(2);
    let head = fa.peek_raw(n);
    let Some(h) = head else { return false };
    if h.len() < 2 || (h != AC4_SYNC_0 && h != AC4_SYNC_1) {
        return false;
    }

    let file_size = fa.remain();

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "AC-4", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);
    fa.fill(StreamKind::General, 0, "StreamSize", "0", true);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "AC-4", false);
    // BitRate_Mode=VBR — AC-4 frames are variable-length by design.
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.fill(StreamKind::Audio, 0, "StreamSize", file_size.to_string(), false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ac4() {
        let mut fa = FileAnalyze::new(b"NOT AC4");
        assert!(!parse_ac4(&mut fa));
    }

    #[test]
    fn accepts_sync_ac40() {
        let mut buf = vec![0xAC, 0x40];
        buf.extend(std::iter::repeat(0u8).take(64));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac4(&mut fa));
    }

    #[test]
    fn accepts_sync_ac41() {
        let mut buf = vec![0xAC, 0x41];
        buf.extend(std::iter::repeat(0u8).take(64));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac4(&mut fa));
    }
}
