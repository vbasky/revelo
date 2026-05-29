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

use revelio_core::{FileAnalyze, Reader, StreamKind};

const HEADER_MIN_BYTES: usize = 96;
const SCRM_OFFSET: usize = 0x2C;
const SENTINEL_OFFSET: usize = 28;

pub fn parse_scream_tracker3(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < HEADER_MIN_BYTES {
        return None;
    }
    let head = r.peek_raw(HEADER_MIN_BYTES)?;
    if head[SENTINEL_OFFSET] != 0x1A || &head[SCRM_OFFSET..SCRM_OFFSET + 4] != b"SCRM" {
        return None;
    }

    r.element_begin("Scream Tracker 3");

    let song_name = trim_local_string(r.read_raw(28)?);
    r.le_u8("0x1A")?;
    r.le_u8("Type")?;
    r.le_u8("Unknown")?;
    r.le_u8("Unknown")?;

    let ord_num = r.le_u16("Orders count")?;
    let ins_num = r.le_u16("Instruments count")?;
    let pat_num = r.le_u16("Paterns count")?;
    r.le_u16("Flags")?;

    let sw_major = r.le_u8("Cwt/v (Major)")?;
    let sw_minor = r.le_u8("Cwt/v (Minor)")?;
    r.le_u16("File format information")?;
    r.be_u32("Signature")?;
    r.le_u8("global volume")?;

    r.le_u8("Initial Speed")?;
    let initial_tempo = r.le_u8("Initial Temp")?;
    r.le_u8("master volume")?;
    r.le_u8("ultra click removal")?;
    r.le_u8("Default channel pan positions are present")?;
    for _ in 0..8 {
        r.le_u8("Unknown")?;
    }
    r.le_u16("Special")?;
    r.skip(32)?; // Channel settings

    // Skip variable-length tail — bounded by Remain so a truncated
    // buffer still completes filling without panicking.
    let orders_len = (ord_num as usize).min(r.remain());
    r.skip(orders_len)?;
    let ins_len = (ins_num as usize * 2).min(r.remain());
    r.skip(ins_len)?;
    let pat_len = (pat_num as usize * 2).min(r.remain());
    r.skip(pat_len)?;

    r.element_end();

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "Scream Tracker 3");
    if !song_name.is_empty() {
        r.set_field(StreamKind::General, 0, "Track", song_name);
    }
    // C++ only emits Encoded_Application when major nibble == 0x1
    // (Scream Tracker family); other trackers (Impulse Tracker, etc.)
    // also write S3M but with different signatures.
    if (sw_major & 0xF0) == 0x10 {
        let app = format!("Scream Tracker {}.{}{}", sw_major, sw_minor / 16, sw_minor % 16);
        r.set_field(StreamKind::General, 0, "Encoded_Application", app);
    }
    r.set_field(StreamKind::General, 0, "BPM", initial_tempo.to_string());
    r.set_field(StreamKind::General, 0, "AudioCount", "1");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "Module");
    Some(())
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
