//! DTS-UHD (DTS:X) parser. Sync detection only; full bitstream parsing
//! per ETSI TS 103 491 is out of scope here.

use revelio_core::{FileAnalyze, StreamKind};

const SYNC_DTSUHD: u32 = 0x40411BF2;

pub fn parse_dts_uhd(fa: &mut FileAnalyze) -> bool {
    let n = fa.remain().min(4);
    let head = fa.peek_raw(n);
    let Some(h) = head else { return false };
    if h.len() < 4 {
        return false;
    }
    let sync = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if sync != SYNC_DTSUHD {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DTS", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "DTS", false);
    fa.fill(StreamKind::Audio, 0, "Format_Profile", "UHD", false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_dts_uhd() {
        let mut fa = FileAnalyze::new(b"NOT A DTS UHD FILE......");
        assert!(!parse_dts_uhd(&mut fa));
    }

    #[test]
    fn rejects_dts_core_sync() {
        // DTS Core (0x7FFE8001) must not be matched as UHD.
        let buf = [0x7Fu8, 0xFE, 0x80, 0x01, 0, 0, 0, 0];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_dts_uhd(&mut fa));
    }

    #[test]
    fn parses_dts_uhd_sync() {
        let mut buf = vec![0x40u8, 0x41, 0x1B, 0xF2];
        buf.resize(64, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dts_uhd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("DTS"));
        assert_eq!(a("Format").as_deref(), Some("DTS"));
        assert_eq!(a("Format_Profile").as_deref(), Some("UHD"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
    }
}
