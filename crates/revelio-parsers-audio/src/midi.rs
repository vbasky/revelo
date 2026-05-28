//! MIDI (Musical Instrument Digital Interface) parser.
//!
//! Mirrors MediaInfoLib's `File_Midi.cpp`, which only identifies the
//! format and rejects deeper parsing. Header layout:
//!   "MThd"               (4 bytes, magic)
//!   uint32 BE  chunk_length    (typically 6)
//!   uint16 BE  format_type     (0, 1, or 2)
//!   uint16 BE  track_count
//!   uint16 BE  time_division

use revelio_core::{FileAnalyze, StreamKind};

const MAGIC_MTHD: [u8; 4] = *b"MThd";
const HEADER_LEN: usize = 14;

pub fn parse_midi(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < HEADER_LEN {
        return false;
    }
    let head = match fa.peek_raw(fa.Remain().min(HEADER_LEN)) {
        Some(h) if h.len() >= 4 => h,
        _ => return false,
    };
    if head[..4] != MAGIC_MTHD {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MIDI", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "MIDI", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_midi(format_type: u16, track_count: u16, time_division: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MThd");
        buf.extend_from_slice(&6u32.to_be_bytes());
        buf.extend_from_slice(&format_type.to_be_bytes());
        buf.extend_from_slice(&track_count.to_be_bytes());
        buf.extend_from_slice(&time_division.to_be_bytes());
        buf
    }

    #[test]
    fn parses_basic_midi_header() {
        let buf = make_midi(1, 4, 480);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_midi(&mut fa));

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("MIDI"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("MIDI"));
    }

    #[test]
    fn rejects_non_midi_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a MIDI file at all..");
        assert!(!parse_midi(&mut fa));
    }

    #[test]
    fn rejects_buffer_too_short_for_header() {
        let mut fa = FileAnalyze::new(b"MThd\0\0");
        assert!(!parse_midi(&mut fa));
    }
}
