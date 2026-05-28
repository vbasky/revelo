//! ScreamTracker 3 (.s3m) parser — mirrors MediaInfoLib's `File_ScreamTracker3.cpp`.
//!
//! The S3M header is a fixed 96-byte structure. We only need the first
//! 96 bytes to populate everything the C++ reference fills; the
//! variable-length channel settings / orders / instruments / patterns
//! tail is skipped to match `Read_Buffer_Continue` consumption.
//!
//! Layout (little-endian):
//!   28 bytes: Song name (CP-437, null/space padded)
//!    1 byte : 0x1A                                       // sentinel
//!    1 byte : Type
//!    2 bytes: Unknown
//!    2 bytes: Orders count
//!    2 bytes: Instruments count
//!    2 bytes: Patterns count
//!    2 bytes: Flags
//!    1 byte : Cwt/v major
//!    1 byte : Cwt/v minor
//!    2 bytes: File format information
//!    4 bytes: "SCRM" signature                           // at offset 0x2C
//!    1 byte : Global volume
//!    1 byte : Initial speed
//!    1 byte : Initial tempo
//!   ... plus more fields and 32-byte channel settings.

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{Int8u, Int16u};

const HEADER_MIN_BYTES: usize = 96;
const SCRM_OFFSET: usize = 0x2C;
const SENTINEL_OFFSET: usize = 28;

pub fn parse_scream_tracker3(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < HEADER_MIN_BYTES {
        return false;
    }
    // peek_raw(min(N, Remain)) per requirements — head is at least
    // HEADER_MIN_BYTES from the early-out above.
    let head = match fa.peek_raw(fa.remain().min(HEADER_MIN_BYTES)) {
        Some(h) => h,
        None => return false,
    };
    if head[SENTINEL_OFFSET] != 0x1A || &head[SCRM_OFFSET..SCRM_OFFSET + 4] != b"SCRM" {
        return false;
    }

    fa.element_begin("Scream Tracker 3");

    let song_name_bytes = fa.read_raw(28).to_vec();
    let song_name = trim_local_string(&song_name_bytes);
    fa.skip_l1("0x1A");
    fa.skip_l1("Type");
    fa.skip_l1("Unknown");
    fa.skip_l1("Unknown");

    let mut ord_num: Int16u = 0;
    let mut ins_num: Int16u = 0;
    let mut pat_num: Int16u = 0;
    let mut flags: Int16u = 0;
    fa.get_l2(&mut ord_num, "Orders count");
    fa.get_l2(&mut ins_num, "Instruments count");
    fa.get_l2(&mut pat_num, "Paterns count");
    fa.get_l2(&mut flags, "Flags");

    let mut sw_major: Int8u = 0;
    let mut sw_minor: Int8u = 0;
    fa.get_l1(&mut sw_major, "Cwt/v (Major)");
    fa.get_l1(&mut sw_minor, "Cwt/v (Minor)");
    fa.skip_l2("File format information");
    fa.skip_b4("Signature");
    fa.skip_l1("global volume");

    let mut initial_speed: Int8u = 0;
    let mut initial_tempo: Int8u = 0;
    fa.get_l1(&mut initial_speed, "Initial Speed");
    fa.get_l1(&mut initial_tempo, "Initial Temp");
    fa.skip_l1("master volume");
    fa.skip_l1("ultra click removal");
    fa.skip_l1("Default channel pan positions are present");
    for _ in 0..8 {
        fa.skip_l1("Unknown");
    }
    let mut special: Int16u = 0;
    fa.get_l2(&mut special, "Special");
    fa.skip_hexa(32, "Channel settings");

    // Skip variable-length tail — bounded by Remain so a truncated
    // buffer still completes filling without panicking.
    let orders_len = (ord_num as usize).min(fa.remain());
    fa.skip_hexa(orders_len, "Orders");
    let ins_len = (ins_num as usize * 2).min(fa.remain());
    fa.skip_hexa(ins_len, "Instruments");
    let pat_len = (pat_num as usize * 2).min(fa.remain());
    fa.skip_hexa(pat_len, "Patterns");

    fa.element_end();

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "Scream Tracker 3", false);
    if !song_name.is_empty() {
        fa.fill(StreamKind::General, 0, "Track", song_name, false);
    }
    // C++ only emits Encoded_Application when major nibble == 0x1
    // (Scream Tracker family); other trackers (Impulse Tracker, etc.)
    // also write S3M but with different signatures.
    if (sw_major & 0xF0) == 0x10 {
        let app = format!("Scream Tracker {}.{}{}", sw_major, sw_minor / 16, sw_minor % 16);
        fa.fill(StreamKind::General, 0, "Encoded_Application", app, false);
    }
    fa.fill(StreamKind::General, 0, "BPM", initial_tempo.to_string(), false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "Module", false);

    let _ = (flags, special, initial_speed);

    true
}

fn trim_local_string(bytes: &[u8]) -> String {
    // S3M song names are typically null-terminated within a 28-byte
    // field; strip at the first NUL then trim trailing spaces.
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s = String::from_utf8_lossy(&bytes[..end]);
    s.trim_matches(' ').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_s3m(
        song_name: &[u8; 28],
        ord_num: u16,
        ins_num: u16,
        pat_num: u16,
        sw_major: u8,
        sw_minor: u8,
        initial_tempo: u8,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(song_name);
        buf.push(0x1A); // sentinel at offset 28
        buf.push(16); // Type
        buf.push(0); // Unknown
        buf.push(0); // Unknown
        buf.extend_from_slice(&ord_num.to_le_bytes());
        buf.extend_from_slice(&ins_num.to_le_bytes());
        buf.extend_from_slice(&pat_num.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // Flags
        buf.push(sw_major);
        buf.push(sw_minor);
        buf.extend_from_slice(&0u16.to_le_bytes()); // File format info
        buf.extend_from_slice(b"SCRM"); // signature at 0x2C (44)
        buf.push(64); // global volume
        buf.push(6); // initial speed
        buf.push(initial_tempo); // initial tempo
        buf.push(48); // master volume
        buf.push(16); // ultra click removal
        buf.push(252); // default channel pan
        for _ in 0..8 {
            buf.push(0);
        }
        buf.extend_from_slice(&0u16.to_le_bytes()); // Special
        buf.extend_from_slice(&[255u8; 32]); // Channel settings
        assert_eq!(buf.len(), 96);
        // Variable tail: orders + instruments(*2) + patterns(*2).
        buf.resize(buf.len() + ord_num as usize + ins_num as usize * 2 + pat_num as usize * 2, 0);
        buf
    }

    #[test]
    fn rejects_non_s3m() {
        let zeros = vec![0u8; 128];
        let mut fa = FileAnalyze::new(&zeros);
        assert!(!parse_scream_tracker3(&mut fa));
    }

    #[test]
    fn rejects_missing_scrm_signature() {
        let mut buf = make_s3m(b"Song name                   ", 4, 2, 2, 0x13, 0x20, 125);
        // Corrupt the SCRM signature.
        buf[0x2C..0x2C + 4].copy_from_slice(b"XXXX");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_scream_tracker3(&mut fa));
    }

    #[test]
    fn parses_minimal_s3m_header() {
        // sw_major=0x13 (Scream Tracker 3), sw_minor=0x20 → "0.20" per C++ (minor/16=2, minor%16=0).
        let name = b"My S3M Song\0                ";
        let buf = make_s3m(name, 4, 3, 2, 0x13, 0x20, 125);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_scream_tracker3(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Scream Tracker 3"));
        assert_eq!(g("Track").as_deref(), Some("My S3M Song"));
        assert_eq!(g("Encoded_Application").as_deref(), Some("Scream Tracker 19.20"));
        assert_eq!(g("BPM").as_deref(), Some("125"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("Module"));
    }
}
