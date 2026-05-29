//! Module (.mod) parser — SoundTracker / ProTracker / NoiseTracker.
//!
//! Mirrors MediaInfoLib's `File_Module.cpp`. The MOD format places its
//! identifying 4-byte tag at offset 1080, after a 20-byte song title and
//! 31 sample headers (31 * 30 = 930 bytes). The minimum buffer required
//! to detect and parse is 1084 bytes.
//!
//! Layout:
//!   20 bytes:        Song title (ASCII, NUL/space padded)
//!   31 * 30 bytes:   Sample headers
//!     22 bytes:        Sample name
//!      2 bytes BE:     Sample length (words)
//!      1 byte:         Finetune
//!      1 byte:         Volume
//!      2 bytes BE:     Repeat offset (words)
//!      2 bytes BE:     Repeat length (words)
//!    1 byte:          Number of song positions
//!    1 byte:          0x7F (historical 0x8F in the C++ trace name)
//!  128 bytes:        Pattern table
//!    4 bytes ASCII:   Signature tag (M.K., M!K!, FLT4, FLT8, 6CHN, 8CHN)

use revelo_core::{FileAnalyze, Reader, StreamKind};

const HEADER_MIN_BYTES: usize = 1084;
const SIGNATURE_OFFSET: usize = 1080;

fn is_valid_signature(sig: &[u8]) -> bool {
    matches!(sig, b"M.K." | b"M!K!" | b"FLT4" | b"FLT8" | b"6CHN" | b"8CHN")
}

pub fn parse_module(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < HEADER_MIN_BYTES {
        return None;
    }
    let head = r.peek_raw(HEADER_MIN_BYTES)?;
    if !is_valid_signature(&head[SIGNATURE_OFFSET..SIGNATURE_OFFSET + 4]) {
        return None;
    }

    r.element_begin("Module");

    let module_name = trim_local_string(r.read_raw(20)?);

    for _ in 0..31 {
        r.element_begin("Sample");
        r.read_raw(22)?;
        r.be_u16("Sample length")?;
        r.be_u8("Finetune value for the sample")?;
        r.be_u8("Volume of the sample")?;
        r.be_u16("Start of sample repeat offset")?;
        r.be_u16("Length of sample repeat")?;
        r.element_end();
    }
    r.be_u8("Number of song positions")?;
    r.be_u8("0x7F")?;
    r.skip(128)?; // Pattern table
    r.fourcc("Signature")?;

    r.element_end();

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "Module");
    if !module_name.is_empty() {
        r.set_field(StreamKind::General, 0, "Track", module_name);
    }
    r.set_field(StreamKind::General, 0, "AudioCount", "1");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "Module");
    Some(())
}

// Names are fixed-width fields, NUL- or space-padded. Use lossy UTF-8
// decode (printable ASCII in practice) and trim trailing NULs and spaces
// to mirror C++ `Get_Local` + `Ztring::Trim`.
fn trim_local_string(bytes: &[u8]) -> String {
    let end = bytes.iter().rposition(|&b| b != 0 && b != b' ').map(|i| i + 1).unwrap_or(0);
    String::from_utf8_lossy(&bytes[..end]).trim_matches(' ').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mod(name: &[u8; 20], signature: &[u8; 4]) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_MIN_BYTES];
        buf[..20].copy_from_slice(name);
        buf[SIGNATURE_OFFSET..SIGNATURE_OFFSET + 4].copy_from_slice(signature);
        buf
    }

    #[test]
    fn rejects_short_buffer() {
        let mut fa = FileAnalyze::new(&[0u8; 100]);
        assert!(!parse_module(&mut fa));
    }

    #[test]
    fn rejects_unknown_signature() {
        let mut buf = vec![0u8; HEADER_MIN_BYTES];
        buf[SIGNATURE_OFFSET..SIGNATURE_OFFSET + 4].copy_from_slice(b"XXXX");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_module(&mut fa));
    }

    #[test]
    fn parses_mkdot_signature_with_name() {
        let buf = make_mod(b"My Mod Tune         ", b"M.K.");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_module(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Module"));
        assert_eq!(g("Track").as_deref(), Some("My Mod Tune"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("Module"));
    }

    #[test]
    fn accepts_all_signature_variants() {
        for sig in [b"M.K.", b"M!K!", b"FLT4", b"FLT8", b"6CHN", b"8CHN"] {
            let buf = make_mod(b"                    ", sig);
            let mut fa = FileAnalyze::new(&buf);
            assert!(parse_module(&mut fa), "should accept {:?}", sig);
        }
    }
}
