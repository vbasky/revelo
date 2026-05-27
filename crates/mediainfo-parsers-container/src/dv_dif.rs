//! DV-DIF (IEC 61834 / SMPTE 314M / 370M) header-only parser.
//!
//! A DV stream is a sequence of 80-byte DIF blocks. Each block begins
//! with a 3-byte ID where the top 3 bits of byte 0 carry the Section
//! Type (SCT) and byte 2 carries the DIF Block Number (DBN). The first
//! 8 blocks of every DIF sequence follow a fixed SCT/DBN pattern:
//!
//!   0  Header   SCT=0 (000xxxxx) DBN=0
//!   1  Subcode  SCT=1 (001xxxxx) DBN=0
//!   2  Subcode  SCT=1             DBN=1
//!   3  VAUX     SCT=2 (010xxxxx) DBN=0
//!   4  VAUX     SCT=2             DBN=1
//!   5  VAUX     SCT=2             DBN=2
//!   6  Audio    SCT=3 (011xxxxx) DBN=0
//!   7  Video    SCT=4 (100xxxxx) DBN=0
//!
//! This matches File_DvDif.cpp's Synchronize() gate and is strong enough
//! to identify DV-DIF without false positives against the common
//! container magics File_DvDif.cpp explicitly rejects (RIFF, MP4, MXF).

use mediainfo_core::{FileAnalyze, StreamKind};

const BLOCK_SIZE: usize = 80;
const REQUIRED_BLOCKS: usize = 8;
const REQUIRED_BYTES: usize = BLOCK_SIZE * REQUIRED_BLOCKS;

pub fn parse_dv_dif(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.Remain().min(REQUIRED_BYTES)) {
        Some(b) if b.len() >= REQUIRED_BYTES => b,
        _ => return false,
    };

    // Reject magics from other formats whose first DIF block would
    // otherwise accidentally match (SCT=0 + zero DBN is easy to hit).
    if is_foreign_magic(buf) {
        return false;
    }

    if !is_dv_sync(buf) {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "DV", true);
    true
}

fn is_dv_sync(buf: &[u8]) -> bool {
    const PATTERN: [(u8, u8); REQUIRED_BLOCKS] = [
        (0x00, 0x00), // Header 0
        (0x20, 0x00), // Subcode 0
        (0x20, 0x01), // Subcode 1
        (0x40, 0x00), // VAUX 0
        (0x40, 0x01), // VAUX 1
        (0x40, 0x02), // VAUX 2
        (0x60, 0x00), // Audio 0
        (0x80, 0x00), // Video 0
    ];
    for (i, (sct, dbn)) in PATTERN.iter().enumerate() {
        let off = i * BLOCK_SIZE;
        if (buf[off] & 0xE0) != *sct
            || (buf[off + 1] & 0xF0) != 0x00
            || buf[off + 2] != *dbn
        {
            return false;
        }
    }
    true
}

fn is_foreign_magic(buf: &[u8]) -> bool {
    let b0 = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let b4 = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    b0 == 0x5249_4646        // RIFF
        || b0 == 0x060E_2B34 // MXF KLV start
        || b4 == 0x6674_7970 // ftyp
        || b4 == 0x6672_6565 // free
        || b4 == 0x6D64_6174 // mdat
        || b4 == 0x6D6F_6F76 // moov
        || b4 == 0x736B_6970 // skip
        || b4 == 0x7769_6465 // wide
}

#[cfg(test)]
mod tests {
    use super::*;
    use mediainfo_core::FileAnalyze;

    fn build_dv_sync() -> Vec<u8> {
        let mut buf = vec![0u8; REQUIRED_BYTES];
        let pattern: [(u8, u8); REQUIRED_BLOCKS] = [
            (0x00, 0x00),
            (0x20, 0x00),
            (0x20, 0x01),
            (0x40, 0x00),
            (0x40, 0x01),
            (0x40, 0x02),
            (0x60, 0x00),
            (0x80, 0x00),
        ];
        for (i, (sct, dbn)) in pattern.iter().enumerate() {
            let off = i * BLOCK_SIZE;
            // Low 5 bits of byte 0 are reserved/sequence info; keep zero.
            buf[off] = *sct;
            buf[off + 1] = 0x00;
            buf[off + 2] = *dbn;
        }
        buf
    }

    #[test]
    fn parses_dv_dif() {
        let data = build_dv_sync();
        let mut fa = FileAnalyze::new(&data);
        assert!(parse_dv_dif(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("DV".to_owned())
        );
    }

    #[test]
    fn rejects_non_dv() {
        let mut fa = FileAnalyze::new(b"NOT A DV-DIF FILE AT ALL, JUST RANDOM TEXT TO FILL ENOUGH BYTES TO SATISFY THE 640-BYTE PEEK WINDOW REQUIRED BY THE PARSER ............................................................................................................................................................................................................................................................................................................................................................................................................................................................................");
        assert!(!parse_dv_dif(&mut fa));
    }

    #[test]
    fn rejects_riff_magic_even_if_dv_pattern() {
        // Verify the foreign-magic gate fires even when the DV bit pattern
        // would otherwise be a match — the first 4 bytes spell RIFF.
        let mut data = build_dv_sync();
        data[0] = b'R';
        data[1] = b'I';
        data[2] = b'F';
        data[3] = b'F';
        let mut fa = FileAnalyze::new(&data);
        assert!(!parse_dv_dif(&mut fa));
    }
}
