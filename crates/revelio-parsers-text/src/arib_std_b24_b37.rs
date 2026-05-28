//! ARIB STD-B24 / STD-B37 caption data parser (Japanese broadcast).
//!
//! MediaInfoLib's `File_AribStdB24B37` (Source/MediaInfo/Text/File_AribStdB24B37.cpp)
//! is a substream parser invoked from MPEG-PS/TS/ANC contexts and does not
//! expose a `FileHeader_Begin`. For a raw-buffer detection path we recognize
//! the on-the-wire "synchronized PES data" data_group structure:
//!
//!   byte 0      : data_group_id (1 bit update flag | 5 bits id | 2 bits version)
//!   byte 1      : data_group_link_number
//!   byte 2      : last_data_group_link_number
//!   bytes 3..5  : data_group_size (big-endian u16)
//!   bytes 5..5+size : payload
//!   last 2 bytes    : CRC-16-CCITT (Xmodem) over header+payload+CRC == 0
//!
//! The CRC check is what discriminates true ARIB data groups from random
//! bytes that happen to start with plausible field values.

use revelio_core::{FileAnalyze, StreamKind};

const HEADER_LEN: usize = 5;
const CRC_LEN: usize = 2;
const PEEK_LIMIT: usize = 4096;

// CRC-16-CCITT (Xmodem) table — identical to AribStdB24B37_CRC_CCIT_Xmodem_Table
// in File_AribStdB24B37.cpp. Polynomial 0x1021, initial value 0x0000.
const CRC_TABLE: [u16; 256] = [
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50A5, 0x60C6, 0x70E7,
    0x8108, 0x9129, 0xA14A, 0xB16B, 0xC18C, 0xD1AD, 0xE1CE, 0xF1EF,
    0x1231, 0x0210, 0x3273, 0x2252, 0x52B5, 0x4294, 0x72F7, 0x62D6,
    0x9339, 0x8318, 0xB37B, 0xA35A, 0xD3BD, 0xC39C, 0xF3FF, 0xE3DE,
    0x2462, 0x3443, 0x0420, 0x1401, 0x64E6, 0x74C7, 0x44A4, 0x5485,
    0xA56A, 0xB54B, 0x8528, 0x9509, 0xE5EE, 0xF5CF, 0xC5AC, 0xD58D,
    0x3653, 0x2672, 0x1611, 0x0630, 0x76D7, 0x66F6, 0x5695, 0x46B4,
    0xB75B, 0xA77A, 0x9719, 0x8738, 0xF7DF, 0xE7FE, 0xD79D, 0xC7BC,
    0x48C4, 0x58E5, 0x6886, 0x78A7, 0x0840, 0x1861, 0x2802, 0x3823,
    0xC9CC, 0xD9ED, 0xE98E, 0xF9AF, 0x8948, 0x9969, 0xA90A, 0xB92B,
    0x5AF5, 0x4AD4, 0x7AB7, 0x6A96, 0x1A71, 0x0A50, 0x3A33, 0x2A12,
    0xDBFD, 0xCBDC, 0xFBBF, 0xEB9E, 0x9B79, 0x8B58, 0xBB3B, 0xAB1A,
    0x6CA6, 0x7C87, 0x4CE4, 0x5CC5, 0x2C22, 0x3C03, 0x0C60, 0x1C41,
    0xEDAE, 0xFD8F, 0xCDEC, 0xDDCD, 0xAD2A, 0xBD0B, 0x8D68, 0x9D49,
    0x7E97, 0x6EB6, 0x5ED5, 0x4EF4, 0x3E13, 0x2E32, 0x1E51, 0x0E70,
    0xFF9F, 0xEFBE, 0xDFDD, 0xCFFC, 0xBF1B, 0xAF3A, 0x9F59, 0x8F78,
    0x9188, 0x81A9, 0xB1CA, 0xA1EB, 0xD10C, 0xC12D, 0xF14E, 0xE16F,
    0x1080, 0x00A1, 0x30C2, 0x20E3, 0x5004, 0x4025, 0x7046, 0x6067,
    0x83B9, 0x9398, 0xA3FB, 0xB3DA, 0xC33D, 0xD31C, 0xE37F, 0xF35E,
    0x02B1, 0x1290, 0x22F3, 0x32D2, 0x4235, 0x5214, 0x6277, 0x7256,
    0xB5EA, 0xA5CB, 0x95A8, 0x8589, 0xF56E, 0xE54F, 0xD52C, 0xC50D,
    0x34E2, 0x24C3, 0x14A0, 0x0481, 0x7466, 0x6447, 0x5424, 0x4405,
    0xA7DB, 0xB7FA, 0x8799, 0x97B8, 0xE75F, 0xF77E, 0xC71D, 0xD73C,
    0x26D3, 0x36F2, 0x0691, 0x16B0, 0x6657, 0x7676, 0x4615, 0x5634,
    0xD94C, 0xC96D, 0xF90E, 0xE92F, 0x99C8, 0x89E9, 0xB98A, 0xA9AB,
    0x5844, 0x4865, 0x7806, 0x6827, 0x18C0, 0x08E1, 0x3882, 0x28A3,
    0xCB7D, 0xDB5C, 0xEB3F, 0xFB1E, 0x8BF9, 0x9BD8, 0xABBB, 0xBB9A,
    0x4A75, 0x5A54, 0x6A37, 0x7A16, 0x0AF1, 0x1AD0, 0x2AB3, 0x3A92,
    0xFD2E, 0xED0F, 0xDD6C, 0xCD4D, 0xBDAA, 0xAD8B, 0x9DE8, 0x8DC9,
    0x7C26, 0x6C07, 0x5C64, 0x4C45, 0x3CA2, 0x2C83, 0x1CE0, 0x0CC1,
    0xEF1F, 0xFF3E, 0xCF5D, 0xDF7C, 0xAF9B, 0xBFBA, 0x8FD9, 0x9FF8,
    0x6E17, 0x7E36, 0x4E55, 0x5E74, 0x2E93, 0x3EB2, 0x0ED1, 0x1EF0,
];

fn crc_ccit_xmodem(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in bytes {
        let idx = ((crc >> 8) as u8) ^ b;
        crc = (crc << 8) ^ CRC_TABLE[idx as usize];
    }
    crc
}

/// Parse ARIB STD-B24/B37 Japanese ISDB captions.
///
/// Detection: ARIB data group header.
/// Fills: Format.
pub fn parse_arib_std_b24_b37(fa: &mut FileAnalyze) -> bool {
    let peek_len = fa.remain().min(PEEK_LIMIT);
    if peek_len < HEADER_LEN + CRC_LEN {
        return false;
    }
    let Some(buf) = fa.peek_raw(peek_len) else {
        return false;
    };

    let data_group_id_byte = buf[0];
    // data_group_id (5 bits after the update flag) must be 0..=8 per the
    // spec — 0 caption_management, 1..=8 caption_statement (1st..8th).
    let data_group_id = (data_group_id_byte >> 2) & 0x1F;
    if data_group_id > 8 {
        return false;
    }
    let data_group_size = u16::from_be_bytes([buf[3], buf[4]]) as usize;
    let total = HEADER_LEN + data_group_size + CRC_LEN;
    if total > buf.len() || data_group_size == 0 {
        return false;
    }

    // The C++ Data_Parse walks data_group_id..end-of-payload-and-CRC and
    // requires the running CRC to be zero; replicate that exactly.
    if crc_ccit_xmodem(&buf[..total]) != 0 {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "ARIB STD B24/B37", true);

    fa.stream_prepare(StreamKind::Text);
    fa.fill(StreamKind::Text, 0, "Format", "ARIB STD B24/B37", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_data_group(data_group_id: u8, payload: &[u8]) -> Vec<u8> {
        let size = payload.len() as u16;
        let mut buf = Vec::with_capacity(HEADER_LEN + payload.len() + CRC_LEN);
        // 1-bit update flag (0) | 5-bit data_group_id | 2-bit version (0)
        buf.push((data_group_id & 0x1F) << 2);
        buf.push(0); // data_group_link_number
        buf.push(0); // last_data_group_link_number
        buf.extend_from_slice(&size.to_be_bytes());
        buf.extend_from_slice(payload);
        let crc = crc_ccit_xmodem(&buf);
        buf.extend_from_slice(&crc.to_be_bytes());
        buf
    }

    #[test]
    fn parses_valid_caption_management_data_group() {
        let buf = build_data_group(0, &[0x00, 0x01, 0x02, 0x03]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_arib_std_b24_b37(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("ARIB STD B24/B37")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("ARIB STD B24/B37")
        );
    }

    #[test]
    fn rejects_bad_crc() {
        let mut buf = build_data_group(1, &[0xAA, 0xBB, 0xCC]);
        // Corrupt the CRC trailer.
        let last = buf.len() - 1;
        buf[last] ^= 0xFF;
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_arib_std_b24_b37(&mut fa));
    }

    #[test]
    fn rejects_unrelated_bytes() {
        let mut fa = FileAnalyze::new(b"NOT ARIB DATA GROUP CONTENT");
        assert!(!parse_arib_std_b24_b37(&mut fa));
    }
}
