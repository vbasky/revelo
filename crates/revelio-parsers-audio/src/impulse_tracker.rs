//! Impulse Tracker (.it) parser — mirrors MediaInfoLib's
//! `File_ImpulseTracker.cpp`. The IT header is a fixed 192-byte prefix
//! followed by variable-length Orders/Instruments/Samples/Patterns
//! tables; the C++ reference walks those tables but does not extract
//! per-entry data, so this parser does the same and stops at fill.
//!
//! Layout (little-endian):
//!    4 bytes ASCII: "IMPM"                         // magic
//!   26 bytes:       Song name (Local/CP-437, NUL-padded)
//!    2 bytes:       Unknown
//!    2 bytes LE:    Orders count   (OrdNum)
//!    2 bytes LE:    Instruments count (InsNum)
//!    2 bytes LE:    Samples count  (SmpNum)
//!    2 bytes LE:    Patterns count (PatNum)
//!    1 byte  :      Cwt/v (Minor)  — software version
//!    1 byte  :      Cwt/v (Major)
//!    1 byte  :      Cwt   (Minor)  — format version
//!    1 byte  :      Cwt   (Major)
//!    2 bytes LE:    Flags (bit 0 = Stereo)
//!    2 bytes LE:    Special
//!    1 byte  :      Global volume
//!    1 byte  :      Mix volume
//!    1 byte  :      Initial Speed
//!    1 byte  :      Initial Tempo (BPM)
//!    1 byte  :      Panning separation
//!    1 byte  :      Zero
//!    2 bytes LE:    Message length
//!    4 bytes LE:    Message offset
//!    4 bytes:       Unknown
//!    1 byte  :      Unknown
//!   64 bytes:       Channel pan
//!   64 bytes:       Channel volume
//!  OrdNum bytes:    Orders
//!  InsNum*4:        Instruments offsets
//!  SmpNum*4:        Samples offsets
//!  PatNum*4:        Patterns offsets

use revelio_core::{FileAnalyze, Reader, StreamKind};

const MAGIC: &[u8; 4] = b"IMPM";
const FIXED_HEADER_BYTES: usize = 192;

pub fn parse_impulse_tracker(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    let head = r.peek_raw(4)?;
    if head.len() < 4 || &head[..4] != MAGIC {
        return None;
    }
    if r.remain() < FIXED_HEADER_BYTES {
        return None;
    }

    r.element_begin("Impulse Tracker");

    r.be_u32("Signature")?;

    let song_name = trim_local_string(r.read_raw(26)?);
    r.le_u8("Unknown")?;
    r.le_u8("Unknown")?;

    let ord_num = r.le_u16("Orders count")?;
    let ins_num = r.le_u16("Instruments count")?;
    let smp_num = r.le_u16("Samples count")?;
    let pat_num = r.le_u16("Paterns count")?;

    let sw_version_minor = r.le_u8("Cwt/v (Minor)")?;
    let sw_version_major = r.le_u8("Cwt/v (Major)")?;
    let version_minor = r.le_u8("Cwt (Minor)")?;
    let version_major = r.le_u8("Cwt (Major)")?;

    let flags = r.le_u16("Flags")?;
    let stereo = (flags & 0x0001) != 0;
    r.le_u16("Special")?;
    r.le_u8("Global volume")?;
    r.le_u8("Mix volume")?;
    r.le_u8("Initial Speed")?;
    let initial_tempo = r.le_u8("Initial Temp")?;
    r.le_u8("Panning separation between channels")?;
    r.le_u8("0")?;
    r.le_u16("Message Length")?;
    r.le_u32("Message Offset")?;
    r.le_u8("Unknown")?;
    r.le_u8("Unknown")?;
    r.le_u8("Unknown")?;
    r.le_u8("Unknown")?;
    r.le_u8("Unknown")?;
    r.skip(64)?; // Chnl Pan
    r.skip(64)?; // Chnl Vol

    // Variable tables: skip only what the buffer still holds, mirroring
    // the C++ reference which calls Skip_XX past the fixed header.
    let ord_bytes = ord_num as usize;
    let ins_bytes = (ins_num as usize) * 4;
    let smp_bytes = (smp_num as usize) * 4;
    let pat_bytes = (pat_num as usize) * 4;
    if r.remain() >= ord_bytes {
        r.skip(ord_bytes)?;
    }
    if r.remain() >= ins_bytes {
        r.skip(ins_bytes)?;
    }
    if r.remain() >= smp_bytes {
        r.skip(smp_bytes)?;
    }
    if r.remain() >= pat_bytes {
        r.skip(pat_bytes)?;
    }

    r.element_end();

    // Version strings mirror C++: minor is split as minor/16 . minor%16
    // (the high nibble is the decimal tens digit, low nibble the ones),
    // so a stored byte of 0x32 prints as "3.2".
    let format_version =
        format!("Version {}.{}{}", version_major, version_minor / 16, version_minor % 16);
    let encoded_app = format!(
        "Impulse Tracker {}.{}{}",
        sw_version_major,
        sw_version_minor / 16,
        sw_version_minor % 16
    );

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "Impulse Tracker");
    r.set_field(StreamKind::General, 0, "Format_Version", format_version);
    if !song_name.is_empty() {
        r.set_field(StreamKind::General, 0, "Track", song_name);
    }
    r.set_field(StreamKind::General, 0, "Encoded_Application", encoded_app);
    r.set_field(StreamKind::General, 0, "BPM", initial_tempo.to_string());
    r.set_field(StreamKind::General, 0, "AudioCount", "1");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "Module");
    r.set_field(StreamKind::Audio, 0, "Channels", if stereo { "2" } else { "1" });
    Some(())
}

// IT stores the song name as a 26-byte fixed-width field, NUL-padded.
// Use lossy UTF-8 (printable ASCII / CP-437 degrades to replacement
// chars) and trim trailing NULs/spaces, matching C++ `Get_Local`.
fn trim_local_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let s = String::from_utf8_lossy(&bytes[..end]);
    s.trim_matches(|c: char| c == ' ' || c == '\0').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)] // fixture builder mirrors the binary header layout
    fn make_it(
        song_name: &[u8; 26],
        ord_num: u16,
        ins_num: u16,
        smp_num: u16,
        pat_num: u16,
        sw_minor: u8,
        sw_major: u8,
        ver_minor: u8,
        ver_major: u8,
        flags: u16,
        tempo: u8,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC); // 4
        buf.extend_from_slice(song_name); // 26
        buf.push(0); // unknown
        buf.push(0); // unknown
        buf.extend_from_slice(&ord_num.to_le_bytes());
        buf.extend_from_slice(&ins_num.to_le_bytes());
        buf.extend_from_slice(&smp_num.to_le_bytes());
        buf.extend_from_slice(&pat_num.to_le_bytes());
        buf.push(sw_minor);
        buf.push(sw_major);
        buf.push(ver_minor);
        buf.push(ver_major);
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // special
        buf.push(128); // global volume
        buf.push(48); // mix volume
        buf.push(6); // initial speed
        buf.push(tempo); // initial tempo
        buf.push(128); // panning separation
        buf.push(0); // 0
        buf.extend_from_slice(&0u16.to_le_bytes()); // message length
        buf.extend_from_slice(&0u32.to_le_bytes()); // message offset
        buf.push(0);
        buf.push(0);
        buf.push(0);
        buf.push(0);
        buf.push(0); // 5 unknowns
        buf.extend_from_slice(&[0u8; 64]); // Chnl Pan
        buf.extend_from_slice(&[0u8; 64]); // Chnl Vol
        // Variable tables.
        buf.extend_from_slice(&vec![0u8; ord_num as usize]);
        buf.extend_from_slice(&vec![0u8; (ins_num as usize) * 4]);
        buf.extend_from_slice(&vec![0u8; (smp_num as usize) * 4]);
        buf.extend_from_slice(&vec![0u8; (pat_num as usize) * 4]);
        buf
    }

    #[test]
    fn rejects_non_it() {
        let mut fa = FileAnalyze::new(b"NOT an IT file at all, no way............");
        assert!(!parse_impulse_tracker(&mut fa));
    }

    #[test]
    fn rejects_too_short_after_magic() {
        // Magic present but buffer shorter than fixed header.
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.resize(50, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_impulse_tracker(&mut fa));
    }

    #[test]
    fn parses_minimal_it_header_stereo() {
        let mut song = [b' '; 26];
        song[..9].copy_from_slice(b"My IT Sng");
        let buf = make_it(
            &song, 4,      // ord_num
            2,      // ins_num
            3,      // smp_num
            1,      // pat_num
            0x32,   // sw minor: "3.2" → split as 3/16=0, 50%16=2 → "0.02" — see below
            2,      // sw major
            0x14,   // ver minor → 1/16=0, 20%16=4 → "0.04"
            2,      // ver major
            0x0001, // Stereo
            125,    // tempo
        );
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_impulse_tracker(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Impulse Tracker"));
        // 0x14 = 20 → 20/16=1, 20%16=4 → "1.4"
        assert_eq!(g("Format_Version").as_deref(), Some("Version 2.14"));
        assert_eq!(g("Track").as_deref(), Some("My IT Sng"));
        // 0x32 = 50 → 50/16=3, 50%16=2 → "3.2"
        assert_eq!(g("Encoded_Application").as_deref(), Some("Impulse Tracker 2.32"));
        assert_eq!(g("BPM").as_deref(), Some("125"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(a("Format").as_deref(), Some("Module"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
    }

    #[test]
    fn mono_when_stereo_flag_clear() {
        let song = [0u8; 26];
        let buf = make_it(
            &song, 0, 0, 0, 0, 0, 1, 0, 1, 0x0000, // Stereo bit clear → mono
            120,
        );
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_impulse_tracker(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channels").as_deref(), Some("1"));
        // Empty/NUL song name should not produce a Track field.
        assert!(g("Track").is_none());
    }
}
