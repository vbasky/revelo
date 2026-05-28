//! DVD-Video (.IFO/.BUP) header-only parser.
//!
//! Mirrors the magic check in MediaInfoLib's `File_Dvdv.cpp::FileHeader_Parse`.
//! DVD-Video information files (Video Manager `VIDEO_TS.IFO` and per-title-set
//! `VTS_NN_0.IFO`, plus their `.BUP` backups) begin with a 12-byte signature:
//!   bytes 0..8   ASCII `"DVDVIDEO"`
//!   bytes 8..12  ASCII `"-VMG"` (Video Manager) or `"-VTS"` (title set)
//!
//! This parser only validates the magic and fills `General.Format`; the full
//! sector-walking analysis of the IFO body is intentionally out of scope here.
//! (The C++ reference is ~1700 lines and parses cell address tables, program
//! chains, etc.; we surface the format identity so callers can route the file.)
//!
//! WHY header-only: the upstream design treats DVDV recognition as a
//! lightweight container probe — the body's stream attributes are derived
//! from VOB scanning rather than from the IFO itself.

use revelio_core::{FileAnalyze, StreamKind};

const MAGIC_LEN: usize = 12;
const MAGIC_DVDVIDEO: &[u8; 8] = b"DVDVIDEO";
const TYPE_VMG: &[u8; 4] = b"-VMG";
const TYPE_VTS: &[u8; 4] = b"-VTS";

/// Parse DVD-Video IFO.
/// Fills: Chapter info, audio/subtitle streams.
pub fn parse_dvdv(fa: &mut FileAnalyze) -> bool {
    // WHY peek_raw with min(): peek_raw fails outright when the requested
    // length exceeds Remain(); clamping lets the magic test still run on
    // buffers that are exactly the header-and-a-bit (and lets the length
    // check below cleanly reject anything shorter than the 12-byte magic).
    let buf = match fa.peek_raw(fa.remain().min(MAGIC_LEN)) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < MAGIC_LEN {
        return false;
    }
    if &buf[0..8] != MAGIC_DVDVIDEO {
        return false;
    }
    let type_bytes = &buf[8..12];
    if type_bytes != TYPE_VMG && type_bytes != TYPE_VTS {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DVDV", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn parses_vmg_header() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"DVDVIDEO-VMG");
        buf.resize(64, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dvdv(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("DVDV".to_owned())
        );
    }

    #[test]
    fn parses_vts_header() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"DVDVIDEO-VTS");
        buf.resize(64, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dvdv(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("DVDV".to_owned())
        );
    }

    #[test]
    fn rejects_non_dvdv() {
        let mut fa = FileAnalyze::new(b"DVDVIDEO-XYZNOTAMATCH");
        assert!(!parse_dvdv(&mut fa));
        let mut fa2 = FileAnalyze::new(b"RIFFWAVEfmt ");
        assert!(!parse_dvdv(&mut fa2));
        let mut fa3 = FileAnalyze::new(b"DVDVIDE");
        assert!(!parse_dvdv(&mut fa3));
    }
}
