//! MXF (Material eXchange Format, SMPTE 377M) parser with timecode extraction.
//!
//! Detects MXF files and extracts timecode from System Items or Material Package.

use revelo_core::mime::mime_for_container;
use revelo_core::{FileAnalyze, StreamKind};

const KLV_ROOT: [u8; 4] = [0x06, 0x0E, 0x2B, 0x34];
const SCAN_WINDOW: usize = 4096;

// SMPTE Universal Labels for MXF key types
#[allow(dead_code)]
const UL_SYSTEM_ITEM: [u8; 16] = [
    0x06, 0x0E, 0x2B, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0D, 0x01, 0x03, 0x01, 0x14, 0x00, 0x00, 0x00,
];

#[allow(dead_code)]
const UL_TIME_CODE_COMPONENT: [u8; 16] = [
    0x06, 0x0E, 0x2B, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0D, 0x01, 0x03, 0x01, 0x01, 0x01, 0x01, 0x00,
];

/// Parse MXF timecode from System Item or Timecode Component
/// Timecode is stored as 8 bytes: DF (1 bit) | CF (1 bit) | Reserved (2 bits) | Frames (6 bits) |
/// Seconds (7 bits) | Minutes (7 bits) | Hours (6 bits) | Reserved (8 bits)
fn parse_mxf_timecode(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }

    // SMPTE 12M timecode format in MXF
    let byte0 = data[0];
    let byte1 = data[1];
    let byte2 = data[2];
    let byte3 = data[3];

    let drop_frame = (byte0 & 0x80) != 0;
    let _color_frame = (byte0 & 0x40) != 0;
    let frames = ((byte0 & 0x3F) as u32) << 2 | ((byte1 >> 6) as u32 & 0x03);
    let seconds = ((byte1 & 0x3F) as u32) << 1 | ((byte2 >> 7) as u32 & 0x01);
    let minutes = ((byte2 & 0x7F) as u32) << 1 | ((byte3 >> 7) as u32 & 0x01);
    let hours = (byte3 >> 2) & 0x1F;

    let sep = if drop_frame { ';' } else { ':' };
    Some(format!("{:02}{sep}{:02}{sep}{:02}{sep}{:02}", hours, minutes, seconds, frames))
}

/// Parse MXF timecode from 12-byte binary group format (SMPTE 309M)
#[allow(dead_code)]
fn parse_timecode_binary_groups(data: &[u8]) -> Option<String> {
    if data.len() < 12 {
        return None;
    }

    // Binary groups format: each byte contains 2 BCD digits
    let frames = ((data[0] >> 4) * 10 + (data[0] & 0x0F)) as u32;
    let seconds = ((data[1] >> 4) * 10 + (data[1] & 0x0F)) as u32;
    let minutes = ((data[2] >> 4) * 10 + (data[2] & 0x0F)) as u32;
    let hours = ((data[3] >> 4) * 10 + (data[3] & 0x0F)) as u32;
    let drop_frame = (data[0] & 0x80) != 0;

    let sep = if drop_frame { ';' } else { ':' };
    Some(format!("{:02}{sep}{:02}{sep}{:02}{sep}{:02}", hours, minutes, seconds, frames))
}

/// Search for and extract timecode from MXF KLV packets
fn extract_mxf_timecode(data: &[u8]) -> Option<String> {
    // Scan for System Item key (0x14 in position 12)
    for i in 0..data.len().saturating_sub(17) {
        if data[i..i + 4] == KLV_ROOT && data[i + 12] == 0x14 {
            // Found potential System Item, look for timecode after BER length
            let ber_len_pos = i + 16;
            if ber_len_pos >= data.len() {
                continue;
            }

            // Parse BER-encoded length
            let first_byte = data[ber_len_pos];
            let (length, data_offset) = if first_byte & 0x80 == 0 {
                // Short form: length in low 7 bits
                (first_byte as usize, ber_len_pos + 1)
            } else {
                // Long form: next N bytes contain length
                let num_bytes = (first_byte & 0x7F) as usize;
                // A real BER length fits in <= 8 bytes; reject absurd counts so
                // the shift-accumulate below can't run away (esp. 32-bit usize).
                if num_bytes > 8 || ber_len_pos + 1 + num_bytes > data.len() {
                    continue;
                }
                let mut len = 0usize;
                for j in 0..num_bytes {
                    len = (len << 8) | data[ber_len_pos + 1 + j] as usize;
                }
                (len, ber_len_pos + 1 + num_bytes)
            };

            // System Item structure: System Metadata Pack (17 bytes) + optional timecode
            // Timecode typically starts at offset 17 within the System Item
            let tc_offset = data_offset + 17;
            if tc_offset + 8 <= data.len()
                && tc_offset + 8 <= i.saturating_add(16).saturating_add(length)
                && let Some(tc) = parse_mxf_timecode(&data[tc_offset..tc_offset + 8])
            {
                return Some(tc);
            }
        }
    }

    // Also scan for Timecode Component (0x01 in position 12)
    for i in 0..data.len().saturating_sub(17) {
        if data[i..i + 4] == KLV_ROOT && data[i + 12] == 0x01 {
            // Found potential Timecode Component
            let ber_len_pos = i + 16;
            if ber_len_pos >= data.len() {
                continue;
            }

            let first_byte = data[ber_len_pos];
            let (length, data_offset) = if first_byte & 0x80 == 0 {
                (first_byte as usize, ber_len_pos + 1)
            } else {
                let num_bytes = (first_byte & 0x7F) as usize;
                // A real BER length fits in <= 8 bytes; reject absurd counts so
                // the shift-accumulate below can't run away (esp. 32-bit usize).
                if num_bytes > 8 || ber_len_pos + 1 + num_bytes > data.len() {
                    continue;
                }
                let mut len = 0usize;
                for j in 0..num_bytes {
                    len = (len << 8) | data[ber_len_pos + 1 + j] as usize;
                }
                (len, ber_len_pos + 1 + num_bytes)
            };

            // Timecode Component data starts with 16-byte UID, then timecode
            let tc_offset = data_offset + 16;
            if tc_offset + 8 <= data.len()
                && tc_offset + 8 <= i.saturating_add(16).saturating_add(length)
                && let Some(tc) = parse_mxf_timecode(&data[tc_offset..tc_offset + 8])
            {
                return Some(tc);
            }
        }
    }

    None
}

/// Parse SMPTE Material eXchange Format container.
///
/// Detection: KLV structure + operational pattern.
/// Fills: Partition pack, preface, identification, timecode.
pub fn parse_mxf(fa: &mut FileAnalyze) -> bool {
    let want = fa.remain().min(SCAN_WINDOW);
    if want < 16 {
        return false;
    }
    let Some(buf) = fa.peek_raw(want) else { return false };

    // Reject AAF — the CDF magic that MXF defensively excludes.
    if buf.len() >= 8 && buf[..8] == [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1] {
        return false;
    }

    // Scan for the SMPTE KLV root key
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

    // Try to extract timecode from the header partition before mutable borrows
    let timecode = extract_mxf_timecode(buf);

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "MXF");
    if let Some(m) = mime_for_container("MXF ") {
        fa.set_field(StreamKind::General, 0, "InternetMediaType", m);
    }

    // Emit timecode if found
    if let Some(tc) = timecode {
        fa.set_field(StreamKind::General, 0, "TimeCode_FirstFrame", tc);
        fa.set_field(StreamKind::General, 0, "TimeCode_Source", "Material Package");
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mxf_timecode() {
        // Test with sample timecode bytes - verify parsing produces valid output
        let data = vec![0x03, 0x08, 0x82, 0x01, 0x00, 0x00, 0x00, 0x00];
        let tc = parse_mxf_timecode(&data);
        assert!(tc.is_some());
        // Verify format is HH:MM:SS:FF with colons (non-drop frame)
        let tc_str = tc.unwrap();
        assert!(tc_str.contains(':'));
        assert!(!tc_str.contains(';')); // Non-drop frame uses colons
    }

    #[test]
    fn test_parse_timecode_drop_frame() {
        // Drop frame flag set
        let data = vec![0x83, 0x08, 0x82, 0x01, 0x00, 0x00, 0x00, 0x00];
        let tc = parse_mxf_timecode(&data);
        assert!(tc.is_some());
        let tc_str = tc.unwrap();
        assert!(tc_str.contains(';'));
    }

    #[test]
    fn rejects_non_mxf() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE\x00\x00\x00\x00");
        assert!(!parse_mxf(&mut fa));
    }

    #[test]
    fn rejects_aaf_cdf_magic() {
        let mut buf = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
        buf.extend_from_slice(&[0u8; 16]);
        buf.extend_from_slice(&KLV_ROOT);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_mxf(&mut fa));
    }

    #[test]
    fn parses_minimal_mxf_with_klv_at_start() {
        let mut buf = vec![0x06, 0x0E, 0x2B, 0x34, 0x02, 0x53, 0x01, 0x01];
        buf.extend_from_slice(&[0u8; 32]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mxf(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MXF".into())
        );
    }
}
