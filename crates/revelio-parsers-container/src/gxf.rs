//! GXF (General eXchange Format, SMPTE 360M) container parser.
//!
//! GXF is a Grass Valley / Thomson broadcast interchange format. The
//! reference implementation lives in `File_Gxf.cpp` (MediaInfoLib). This
//! Rust port mirrors only the `FileHeader_Begin` / `Synchronize` slice:
//! magic detection and General.Format. Full track-map / media parsing is
//! deferred — the C++ parser delegates to File_Mpegv / File_Avc / File_Dv
//! / etc., none of which are wired into this header-only path.
//!
//! Packet header is 16 bytes (SMPTE 360M §6.1):
//!   00 00 00 00 01   packet leader (5 bytes; the 0x01 doubles as start)
//!   TT               packet type (0xBC=map, 0xBF=media, 0xFB=EOS, ...)
//!   SS SS SS SS      packet length (big-endian, includes 16-byte header)
//!   00 00 00 00      reserved
//!   E1 E2            packet trailer
//!
//! Detection: first packet at offset 0 must be a map packet (0xBC), and
//! the trailer bytes at offsets 14..16 must be E1 E2. We additionally
//! validate that the declared packet length lands on a second valid
//! packet header (leader + trailer) when enough bytes are buffered — the
//! C++ Synchronize() applies the same coherence check.

use revelio_core::{FileAnalyze, StreamKind};

const PACKET_HEADER_SIZE: usize = 16;
const TRAILER_E1: u8 = 0xE1;
const TRAILER_E2: u8 = 0xE2;
const PACKET_TYPE_MAP: u8 = 0xBC;

fn is_packet_header(buf: &[u8]) -> bool {
    buf.len() >= PACKET_HEADER_SIZE
        && buf[0] == 0x00
        && buf[1] == 0x00
        && buf[2] == 0x00
        && buf[3] == 0x00
        && buf[4] == 0x01
        && buf[14] == TRAILER_E1
        && buf[15] == TRAILER_E2
}

/// Parse SMPTE 360M GXF container.
///
/// Detection: GXF KLV structure.
/// Fills: Material metadata, timecode, stream descriptors.
pub fn parse_gxf(fa: &mut FileAnalyze) -> bool {
    // Peek up to 64 KiB — enough to validate the first packet's coherence
    // (a map packet is typically a few hundred bytes) without slurping
    // the whole file when callers stream large GXF assets.
    let want = fa.remain().min(64 * 1024);
    let buf = match fa.peek_raw(want) {
        Some(b) => b,
        None => return false,
    };
    if !is_packet_header(buf) {
        return false;
    }
    // GXF's first packet is always a "map" (0xBC); reject anything else
    // to avoid colliding with other 5-byte zero-leader formats.
    if buf[5] != PACKET_TYPE_MAP {
        return false;
    }
    let size = u32::from_be_bytes([buf[6], buf[7], buf[8], buf[9]]) as usize;
    // SMPTE 360M packets include the 16-byte header in their length.
    if size < PACKET_HEADER_SIZE {
        return false;
    }
    // Coherence check: if the next packet header is within our peek
    // window, validate its leader + trailer. If it's past the window,
    // accept on the strength of the first header alone (matches the
    // C++ "need more data" branch which keeps the file accepted).
    if let Some(next) = buf.get(size..size + PACKET_HEADER_SIZE) {
        if !is_packet_header(next) {
            return false;
        }
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "GXF", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_packet(packet_type: u8, payload: &[u8]) -> Vec<u8> {
        let total = PACKET_HEADER_SIZE + payload.len();
        let mut v = Vec::with_capacity(total);
        v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x01]);
        v.push(packet_type);
        v.extend_from_slice(&(total as u32).to_be_bytes());
        v.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        v.extend_from_slice(&[TRAILER_E1, TRAILER_E2]);
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn parses_minimal_gxf_header() {
        // Two back-to-back packets so the coherence check passes.
        let mut buf = make_packet(PACKET_TYPE_MAP, &[0u8; 32]);
        buf.extend_from_slice(&make_packet(0xBF, &[0u8; 16]));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_gxf(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("GXF"),
        );
    }

    #[test]
    fn rejects_non_gxf_buffer() {
        let mut fa = FileAnalyze::new(b"NOT A GXF FILE AT ALL AT ALL AT ALL");
        assert!(!parse_gxf(&mut fa));
    }

    #[test]
    fn rejects_leader_without_trailer() {
        // Correct 5-byte leader but missing E1 E2 trailer — must not match.
        let mut buf = vec![0x00, 0x00, 0x00, 0x00, 0x01, PACKET_TYPE_MAP];
        buf.extend_from_slice(&64u32.to_be_bytes());
        buf.extend_from_slice(&[0u8; 6]); // reserved+trailer all zero
        buf.extend_from_slice(&[0u8; 64]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_gxf(&mut fa));
    }
}
