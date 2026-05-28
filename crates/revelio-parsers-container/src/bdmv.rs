//! BDMV (Blu-ray Disc Movie) sidecar parser.
//!
//! Mirrors the FileHeader_Begin path of MediaInfoLib's `File_Bdmv.cpp`.
//! BDMV directories pair MPEG-2 TS streams with several typed metadata
//! files. Each metadata file opens with an 8-byte header:
//!
//!   0x00  C4  TypeIndicator  ASCII FourCC selecting the role
//!   0x04  C4  VersionNumber  ASCII digits, e.g. "0100", "0200", "0300"
//!
//! TypeIndicator values (per the `Elements` namespace in the C++):
//!   "INDX" 0x494E4458 → index.bdmv          (top-level index)
//!   "MOBJ" 0x4D4F424A → MovieObject.bdmv    (navigation commands)
//!   "MPLS" 0x4D504C53 → *.mpls              (PlayList)
//!   "HDMV" 0x48444D56 → *.clpi              (ClipInfo; magic is "HDMV"
//!                                            despite the .clpi suffix)
//!
//! This is a header-only identifier: it stamps General.Format = "BDMV"
//! plus a Format_Profile sub-type and a Format_Version derived from the
//! ASCII version digits. No streams are emitted — the elementary streams
//! live in the companion .m2ts files (parsed by `mpeg_ts`).

use revelio_core::{FileAnalyze, StreamKind};

const HEADER_SIZE: usize = 8;

const TYPE_INDX: u32 = u32::from_be_bytes(*b"INDX");
const TYPE_MOBJ: u32 = u32::from_be_bytes(*b"MOBJ");
const TYPE_MPLS: u32 = u32::from_be_bytes(*b"MPLS");
// ClipInfo files use "HDMV" as their on-disk type indicator even though
// the filename extension is ".clpi". This matches `Elements::CLPI` in the
// C++ implementation.
const TYPE_CLPI: u32 = u32::from_be_bytes(*b"HDMV");

pub fn parse_bdmv(fa: &mut FileAnalyze) -> bool {
    let header = match fa.peek_raw(fa.remain().min(HEADER_SIZE)) {
        Some(b) if b.len() >= HEADER_SIZE => b,
        _ => return false,
    };

    let type_indicator = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let profile = match type_indicator {
        TYPE_INDX => "Index",
        TYPE_MOBJ => "MovieObject",
        TYPE_MPLS => "PlayList",
        TYPE_CLPI => "ClipInfo",
        _ => return false,
    };

    // Version digits are required to be ASCII; reject anything else so we
    // don't claim non-BDMV buffers that happen to share a FourCC.
    let version_bytes = [header[4], header[5], header[6], header[7]];
    if !version_bytes.iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // BDMV stores VersionNumber as 4 ASCII digits split <major:2>.<minor:2>
    // (e.g. "0200" → "02.00"). Keeping the literal two-digit groups avoids
    // ambiguity when either side has a leading zero.
    let version = format!(
        "{}{}.{}{}",
        version_bytes[0] as char,
        version_bytes[1] as char,
        version_bytes[2] as char,
        version_bytes[3] as char,
    );

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "BDMV", false);
    fa.fill(StreamKind::General, 0, "Format_Profile", profile, false);
    fa.fill(StreamKind::General, 0, "Format_Version", version, false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_header(magic: &[u8; 4], version: &[u8; 4]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(HEADER_SIZE);
        buf.extend_from_slice(magic);
        buf.extend_from_slice(version);
        buf
    }

    #[test]
    fn parses_mpls_playlist_header() {
        let buf = make_header(b"MPLS", b"0200");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_bdmv(&mut fa));
        let g = |k: &str| {
            fa.retrieve(StreamKind::General, 0, k)
                .map(|z| z.as_str().to_owned())
        };
        assert_eq!(g("Format").as_deref(), Some("BDMV"));
        assert_eq!(g("Format_Profile").as_deref(), Some("PlayList"));
        assert_eq!(g("Format_Version").as_deref(), Some("02.00"));
    }

    #[test]
    fn parses_clpi_header_using_hdmv_magic() {
        // ClipInfo (.clpi) files start with "HDMV", not "CLPI".
        let buf = make_header(b"HDMV", b"0100");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_bdmv(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format_Profile")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("ClipInfo"),
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format_Version")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("01.00"),
        );
    }

    #[test]
    fn rejects_unknown_magic() {
        let buf = make_header(b"RIFF", b"0100");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_bdmv(&mut fa));
    }

    #[test]
    fn rejects_non_ascii_version() {
        let buf = make_header(b"INDX", &[0x00, 0x01, 0x00, 0x00]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_bdmv(&mut fa));
    }

    #[test]
    fn rejects_short_buffer() {
        let buf = b"MPLS".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_bdmv(&mut fa));
    }
}
