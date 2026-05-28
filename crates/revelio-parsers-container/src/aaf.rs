//! AAF (Advanced Authoring Format) parser.
//!
//! AAF files are wrapped in a Microsoft Compound Document Binary (CDF /
//! OLE2 Structured Storage). The CDF header magic is the eight bytes
//! `D0 CF 11 E0 A1 B1 1A E1` at file offset 0. AAF differentiates
//! itself from generic CDF/OLE files (e.g. .doc, .xls) by following the
//! 8-byte CDF magic with a 16-byte AAF-specific signature:
//!   41 41 46 42 0D 00 4F 4D 06 0E 2B 34 01 01 01 FF
//! which spells "AAFB\r\0OM" followed by a SMPTE UL prefix.
//!
//! This parser performs magic-only detection sufficient to set
//! General.Format = "AAF". A full implementation would walk the CDF
//! FAT/MiniFAT/Directory streams (see C++ File_Aaf.cpp) to enumerate
//! referenced essence files; that requires Reference-file resolution
//! which the rust-engine does not yet support.

use revelio_core::{FileAnalyze, StreamKind};

const CDF_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const AAF_SIG: [u8; 16] = [
    0x41, 0x41, 0x46, 0x42, 0x0D, 0x00, 0x4F, 0x4D, 0x06, 0x0E, 0x2B, 0x34, 0x01, 0x01, 0x01, 0xFF,
];

pub fn parse_aaf(fa: &mut FileAnalyze) -> bool {
    // Need the 8-byte CDF header + 16-byte AAF signature = 24 bytes minimum.
    let buf = match fa.peek_raw(fa.Remain().min(24)) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 24 {
        return false;
    }
    if buf[..8] != CDF_MAGIC {
        return false;
    }
    if buf[8..24] != AAF_SIG {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "AAF", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    fn build_aaf_header() -> Vec<u8> {
        let mut v = Vec::with_capacity(64);
        v.extend_from_slice(&CDF_MAGIC);
        v.extend_from_slice(&AAF_SIG);
        // Pad out to a plausible header size with zeros.
        v.resize(64, 0);
        v
    }

    #[test]
    fn rejects_non_aaf() {
        let mut fa = FileAnalyze::new(b"NOT AN AAF FILE AT ALL, JUST GARBAGE BYTES HERE!!!");
        assert!(!parse_aaf(&mut fa));
    }

    #[test]
    fn rejects_cdf_without_aaf_signature() {
        // Pure CDF (e.g. a .doc / .xls) — magic matches but AAF sig does not.
        let mut buf = Vec::new();
        buf.extend_from_slice(&CDF_MAGIC);
        buf.resize(64, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_aaf(&mut fa));
    }

    #[test]
    fn accepts_aaf_header() {
        let buf = build_aaf_header();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aaf(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("AAF".to_owned())
        );
    }
}
