//! MXF (Material eXchange Format, SMPTE 377M) identification.
//!
//! Detects MXF files by scanning the first 4 KiB for the SMPTE KLV
//! root prefix `06 0E 2B 34` (Universal Label header). The C++
//! `File_Mxf::FileHeader_Begin` explicitly rejects AAF (CDF magic
//! starting with `D0 CF 11 E0`) before accepting, so the dispatch
//! must run AAF before MXF — which is already true given AAF's
//! position in the alphabetical pub-mod list.
//!
//! Full essence/header partition walking is deferred; this is a
//! container-level identification only.

use mediainfo_core::{FileAnalyze, StreamKind};

const KLV_ROOT: [u8; 4] = [0x06, 0x0E, 0x2B, 0x34];
const SCAN_WINDOW: usize = 4096;

pub fn parse_mxf(fa: &mut FileAnalyze) -> bool {
    let want = fa.Remain().min(SCAN_WINDOW);
    if want < 16 {
        return false;
    }
    let Some(buf) = fa.peek_raw(want) else { return false };
    // Reject AAF — the CDF magic that MXF defensively excludes.
    if buf.len() >= 8 && &buf[..8] == &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] {
        return false;
    }
    // Scan for the SMPTE KLV root key 06 0E 2B 34 in the first window.
    // MXF Header Partition starts at offset 0 in well-formed files but
    // RIP / KAG / pre-padding can offset it slightly.
    let mut found = false;
    for i in 0..buf.len().saturating_sub(4) {
        if buf[i..i + 4] == KLV_ROOT {
            found = true;
            break;
        }
    }
    if !found {
        return false;
    }
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MXF", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_mxf() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE\x00\x00\x00\x00");
        assert!(!parse_mxf(&mut fa));
    }

    #[test]
    fn rejects_aaf_cdf_magic() {
        // The 8-byte CDF magic; would otherwise pass since KLV root
        // may appear elsewhere in an AAF structured storage file.
        let mut buf = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        buf.extend_from_slice(&[0u8; 16]);
        // Even if a KLV-like key were present further in:
        buf.extend_from_slice(&KLV_ROOT);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_mxf(&mut fa));
    }

    #[test]
    fn parses_minimal_mxf_with_klv_at_start() {
        // Header Partition KLV key prefix.
        let mut buf = vec![0x06, 0x0E, 0x2B, 0x34, 0x02, 0x05, 0x01, 0x01];
        buf.extend_from_slice(&[0u8; 32]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mxf(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MXF".into())
        );
    }

    #[test]
    fn parses_mxf_with_klv_after_padding() {
        // RIP-style padding can put the first KLV slightly offset.
        let mut buf = vec![0u8; 64];
        buf.extend_from_slice(&KLV_ROOT);
        buf.extend_from_slice(&[0u8; 32]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mxf(&mut fa));
    }
}
