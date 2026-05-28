//! Extended Module (.xm) parser — FastTracker II music tracker format.
//!
//! Mirrors MediaInfoLib's `File_ExtendedModule.cpp`. The XM header is a
//! fixed 336-byte prefix (17 + 20 + 1 + 20 + 2 + 4 + 16 + 256) followed
//! by pattern and instrument blocks (not parsed here — the C++ reference
//! only consumes the header before calling `Finish`).
//!
//! Layout:
//!   17 bytes ASCII: "Extended Module: "                       // magic
//!   20 bytes:       Module name (Local/CP-437, space padded)
//!    1 byte  :      0x1A                                      // sentinel
//!   20 bytes:       Tracker name (Local/CP-437, space padded)
//!    1 byte  :      Version (minor)                           // little-endian
//!    1 byte  :      Version (major)
//!    4 bytes LE:    Header size
//!    2 bytes LE:    Song length
//!    2 bytes LE:    Restart position
//!    2 bytes LE:    Number of channels
//!    2 bytes LE:    Number of patterns
//!    2 bytes LE:    Number of instruments
//!    2 bytes LE:    Flags
//!    2 bytes LE:    Tempo
//!    2 bytes LE:    BPM
//!   256 bytes:      Pattern order table

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{Int16u, Int32u, Int8u};

const MAGIC: &[u8; 17] = b"Extended Module: ";
const HEADER_MIN_BYTES: usize = 336;

pub fn parse_extended_module(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < HEADER_MIN_BYTES {
        return false;
    }
    let head = match fa.peek_raw(38) {
        Some(h) => h,
        None => return false,
    };
    if &head[..17] != MAGIC || head[37] != 0x1A {
        return false;
    }

    fa.element_begin("Extended Module");

    // Signature ("Extended Module: ").
    let _ = fa.read_raw(17);

    // Module name: 20 bytes, then 0x1A sentinel.
    let module_name_bytes = fa.read_raw(20).to_vec();
    let module_name = trim_local_string(&module_name_bytes);
    fa.skip_l1("0x1A");

    // Tracker name: 20 bytes.
    let tracker_name_bytes = fa.read_raw(20).to_vec();
    let tracker_name = trim_local_string(&tracker_name_bytes);

    let mut version_minor: Int8u = 0;
    let mut version_major: Int8u = 0;
    fa.get_l1(&mut version_minor, "Version (minor)");
    fa.get_l1(&mut version_major, "Version (major)");

    let mut header_size: Int32u = 0;
    fa.get_l4(&mut header_size, "Header size");

    let mut length: Int16u = 0;
    fa.get_l2(&mut length, "Song Length");
    fa.skip_l2("Restart position");

    let mut channels: Int16u = 0;
    let mut patterns: Int16u = 0;
    let mut instruments: Int16u = 0;
    let mut flags: Int16u = 0;
    let mut tempo: Int16u = 0;
    let mut bpm: Int16u = 0;
    fa.get_l2(&mut channels, "Number of channels");
    fa.get_l2(&mut patterns, "Number of patterns");
    fa.get_l2(&mut instruments, "Number of instruments");
    fa.get_l2(&mut flags, "Flags");
    fa.get_l2(&mut tempo, "Tempo");
    fa.get_l2(&mut bpm, "BPM");
    fa.skip_hexa(256, "Pattern order table");

    fa.element_end();

    // Version string mirrors C++: "<major>.<minor/10><minor%10>" so
    // version 1.04 prints as "1.04" (not "1.4").
    let version_str = format!(
        "{}.{}{}",
        version_major,
        version_minor / 10,
        version_minor % 10
    );

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "Extended Module", false);
    fa.fill(StreamKind::General, 0, "Format_Version", version_str, false);
    if !module_name.is_empty() {
        fa.fill(StreamKind::General, 0, "Track", module_name, false);
    }
    if !tracker_name.is_empty() {
        fa.fill(StreamKind::General, 0, "Encoded_Application", tracker_name, false);
    }
    fa.fill(StreamKind::General, 0, "Tempo", tempo.to_string(), false);
    fa.fill(StreamKind::General, 0, "BPM", bpm.to_string(), false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "Module", false);
    fa.fill(StreamKind::Audio, 0, "Sampler, Channels", channels.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Sampler, Patterns", patterns.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Sampler, Instruments", instruments.to_string(), false);

    let _ = (length, flags, header_size);

    true
}

/// XM stores names as 20-byte fixed-width fields, space-padded on the
/// right. Use lossy UTF-8 decode (most files use printable ASCII; CP-437
/// extended chars degrade gracefully via the replacement char) and trim
/// trailing spaces, matching C++ `Ztring::Trim(' ')`.
fn trim_local_string(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.trim_matches(' ').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_xm(
        module: &[u8; 20],
        tracker: &[u8; 20],
        version_minor: u8,
        version_major: u8,
        channels: u16,
        patterns: u16,
        instruments: u16,
        tempo: u16,
        bpm: u16,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(module);
        buf.push(0x1A);
        buf.extend_from_slice(tracker);
        buf.push(version_minor);
        buf.push(version_major);
        buf.extend_from_slice(&276u32.to_le_bytes()); // header size
        buf.extend_from_slice(&16u16.to_le_bytes());  // song length
        buf.extend_from_slice(&0u16.to_le_bytes());   // restart position
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&patterns.to_le_bytes());
        buf.extend_from_slice(&instruments.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());   // flags
        buf.extend_from_slice(&tempo.to_le_bytes());
        buf.extend_from_slice(&bpm.to_le_bytes());
        buf.extend_from_slice(&[0u8; 256]);           // pattern order table
        buf
    }

    #[test]
    fn rejects_non_xm() {
        let mut fa = FileAnalyze::new(b"NOT an XM file, no way, nope...........");
        assert!(!parse_extended_module(&mut fa));
    }

    #[test]
    fn rejects_missing_0x1a_sentinel() {
        // Magic matches but byte 37 is not 0x1A.
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(b"ModName             "); // 20 bytes
        buf.push(0x00); // wrong sentinel
        buf.resize(HEADER_MIN_BYTES, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_extended_module(&mut fa));
    }

    #[test]
    fn parses_minimal_xm_header() {
        let buf = make_xm(
            b"My Cool Song        ",
            b"FastTracker v2.00   ",
            4,   // minor → "04"
            1,   // major
            8,   // channels
            32,  // patterns
            16,  // instruments
            6,   // tempo
            125, // bpm
        );
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_extended_module(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Extended Module"));
        assert_eq!(g("Format_Version").as_deref(), Some("1.04"));
        assert_eq!(g("Track").as_deref(), Some("My Cool Song"));
        assert_eq!(g("Encoded_Application").as_deref(), Some("FastTracker v2.00"));
        assert_eq!(g("Tempo").as_deref(), Some("6"));
        assert_eq!(g("BPM").as_deref(), Some("125"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(a("Format").as_deref(), Some("Module"));
        assert_eq!(a("Sampler, Channels").as_deref(), Some("8"));
        assert_eq!(a("Sampler, Patterns").as_deref(), Some("32"));
        assert_eq!(a("Sampler, Instruments").as_deref(), Some("16"));
    }

    #[test]
    fn version_string_zero_pads_minor() {
        // minor=4 → "04", minor=23 → "23".
        let buf = make_xm(
            b"                    ",
            b"                    ",
            23, 2, 4, 8, 4, 6, 120,
        );
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_extended_module(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format_Version").as_deref(), Some("2.23"));
        // Empty (all-spaces) names should not produce fields.
        assert!(g("Track").is_none());
        assert!(g("Encoded_Application").is_none());
    }
}
