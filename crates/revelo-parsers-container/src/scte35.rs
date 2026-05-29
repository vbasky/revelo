//! SCTE-35 splice_info_section parser.
//!
//! Parses ANSI/SCTE 35 splice information sections used for digital ad
//! insertion in MPEG-TS streams. Detects the table_id (0xFC) and extracts
//! splice command type, PTS adjustment, segmentation descriptors, and
//! UPID (Unique Program Identifier).

use revelo_core::{FileAnalyze, StreamKind};

/// SCTE-35 segmentation_upid_type meanings
const UPID_TYPES: &[(u8, &str)] = &[
    (0x00, "Not Used"),
    (0x01, "Deprecated"),
    (0x02, "Deprecated"),
    (0x03, "Ad ID"),
    (0x04, "UMID"),
    (0x05, "ISAN"),
    (0x06, "ISAN Version"),
    (0x07, "TIF"),
    (0x08, "ADI"),
    (0x09, "EIDR"),
    (0x0A, "ATSC Content ID"),
    (0x0B, "MPU"),
    (0x0C, "MID"),
    (0x0D, "ADS Info"),
    (0x0E, "URI"),
    (0x0F, "UUID"),
    (0x10, "SCTE 35 USB"),
    (0x11, "SCTE 35 104"),
];

fn upid_type_name(t: u8) -> &'static str {
    for &(code, name) in UPID_TYPES {
        if code == t {
            return name;
        }
    }
    "Unknown"
}

/// SCTE-35 segmentation_type_id meanings
fn segmentation_type_name(t: u8) -> &'static str {
    match t {
        0x00 => "Not Indicated",
        0x01 => "Content Identification",
        0x02 => "Program Start",
        0x03 => "Program End",
        0x04 => "Program Early Termination",
        0x10 => "Break",
        0x11 => "Break End",
        0x12 => "Opening",
        0x13 => "Opening End",
        0x14 => "Closing",
        0x15 => "Closing End",
        0x16 => "Provider Advertisement Start",
        0x17 => "Provider Advertisement End",
        0x18 => "Distributor Advertisement Start",
        0x19 => "Distributor Advertisement End",
        0x20 => "Provider Placement Opportunity Start",
        0x21 => "Provider Placement Opportunity End",
        0x22 => "Provider Overlay Placement Opportunity Start",
        0x23 => "Provider Overlay Placement Opportunity End",
        0x24 => "Distributor Placement Opportunity Start",
        0x25 => "Distributor Placement Opportunity End",
        0x26 => "Distributor Overlay Placement Opportunity Start",
        0x27 => "Distributor Overlay Placement Opportunity End",
        0x30 => "Unscheduled Event Start",
        0x31 => "Unscheduled Event End",
        0x32 => "Network Start",
        0x33 => "Network End",
        _ => "Unknown",
    }
}

/// Parse a raw SCTE-35 splice_info_section buffer.
///
/// Detection: first byte is 0xFC (table_id for splice_info_section).
pub fn parse_scte35(fa: &mut FileAnalyze) -> bool {
    let magic = fa.peek_b4();
    if magic != 0xFC && (magic >> 24) as u8 != 0xFC {
        return false;
    }

    let owned = match fa.peek_raw(fa.remain().min(4096)) {
        Some(b) => b.to_vec(),
        None => return false,
    };

    if owned.is_empty() || owned[0] != 0xFC {
        return false;
    }
    if owned.len() < 5 {
        return false;
    }

    let section_length = ((owned[1] as usize & 0x0F) << 8) | owned[2] as usize;
    if !(4..=4091).contains(&section_length) {
        return false;
    }
    if owned.len() < section_length + 3 {
        return false;
    }

    let protocol_version = owned[3] >> 4;
    if protocol_version != 0 {
        return false;
    }

    let encrypted = (owned[3] >> 3) & 1;
    if encrypted != 0 {
        return false;
    }

    let pts_adjustment = ((owned[3] as u64 & 0x07) << 30)
        | (owned[4] as u64) << 22
        | (owned[5] as u64) << 14
        | (owned[6] as u64) << 6
        | ((owned[7] as u64) >> 2);

    let cw_index = owned[7] & 0x3F;
    let tier = ((owned[8] as u16) << 4) | ((owned[9] as u16) >> 4);
    let splice_command_length = ((owned[9] as u16 & 0x0F) << 8) | owned[10] as u16;
    let splice_command_type = owned[11];

    let mut pos: usize = 12;

    if splice_command_length as usize > 0 && pos < owned.len() {
        match splice_command_type {
            0x05 => parse_splice_insert(&owned, &mut pos),
            0x06 => parse_time_signal(&owned, &mut pos),
            _ => {}
        }
    }

    if pos + 2 < owned.len() {
        let descriptor_loop_length = ((owned[pos] as usize) << 8) | owned[pos + 1] as usize;
        pos += 2;
        let des_end = pos + descriptor_loop_length;
        while pos + 2 < des_end.min(owned.len()) {
            let tag = owned[pos];
            let dlen = owned[pos + 1] as usize;
            pos += 2;
            if pos + dlen > owned.len() {
                break;
            }
            if tag == 0x02 && dlen >= 8 {
                parse_segmentation_descriptor(&owned, pos, dlen, fa);
            }
            pos += dlen;
        }
    }

    fa.stream_prepare(StreamKind::Other);
    fa.set_field(StreamKind::Other, 0, "Format", "SCTE 35");

    let cmd_name = match splice_command_type {
        0x00 => "splice_null",
        0x04 => "splice_schedule",
        0x05 => "splice_insert",
        0x06 => "time_signal",
        0x07 => "bandwidth_reservation",
        _ => "reserved",
    };
    fa.set_field(StreamKind::Other, 0, "Format_Info", format!("SCTE 35 {} command", cmd_name));

    fa.set_field(StreamKind::Other, 0, "SpliceCommandType", splice_command_type.to_string());
    fa.set_field(StreamKind::Other, 0, "SpliceCommandName", cmd_name);
    fa.set_field(StreamKind::Other, 0, "Tier", tier.to_string());
    fa.set_field(StreamKind::Other, 0, "CWIndex", cw_index.to_string());

    let pts_secs = pts_adjustment as f64 / 90000.0;
    fa.set_field(StreamKind::Other, 0, "PTSAdjustment", format!("{:.3}", pts_secs));

    true
}

fn parse_splice_insert(buf: &[u8], pos: &mut usize) {
    if *pos + 4 > buf.len() {
        return;
    }
    let _splice_event_id =
        u32::from_be_bytes([buf[*pos], buf[*pos + 1], buf[*pos + 2], buf[*pos + 3]]);
    *pos += 4;
    if *pos >= buf.len() {
        return;
    }
    let cancel = (buf[*pos] >> 7) & 1;
    *pos += 1;
    if cancel == 1 {
        return;
    }
    if *pos >= buf.len() {
        return;
    }
    let _out_of_network = (buf[*pos] >> 7) & 1;
    let _program_splice = (buf[*pos] >> 6) & 1;
    let _duration_flag = (buf[*pos] >> 5) & 1;
    let _immediate = (buf[*pos] >> 4) & 1;
    *pos += 1;
    if *pos + 4 > buf.len() {
        return;
    }
    let _splice_time = (buf[*pos] as u64) << 25
        | (buf[*pos + 1] as u64) << 17
        | (buf[*pos + 2] as u64) << 9
        | (buf[*pos + 3] as u64) << 1
        | ((buf[*pos + 4] as u64) >> 7);
    *pos += 5;
}

fn parse_time_signal(_buf: &[u8], pos: &mut usize) {
    if *pos + 4 > _buf.len() {
        return;
    }
    *pos += 5; // 33-bit splice_time (PTS)
}

fn parse_segmentation_descriptor(buf: &[u8], offset: usize, dlen: usize, fa: &mut FileAnalyze) {
    if dlen < 12 {
        return;
    }
    // Skip 4-byte identifier (typically 0x43554549 = 'CUEI')
    let o = offset + 4;
    if o + 8 > buf.len() {
        return;
    }
    let _seg_event_id = u32::from_be_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
    let cancel = (buf[o + 4] >> 7) & 1;
    if cancel == 1 {
        return;
    }
    if o + 7 >= buf.len() {
        return;
    }

    let dur_flag = (buf[o + 5] >> 7) & 1;
    let _delivery_not_restricted = (buf[o + 5] >> 6) & 1;

    let mut dpos = o + 6;
    let seg_dur;
    if dur_flag == 1 {
        if dpos + 4 > buf.len() {
            return;
        }
        seg_dur = ((buf[dpos] as u32) << 25)
            | (buf[dpos + 1] as u32) << 17
            | (buf[dpos + 2] as u32) << 9
            | (buf[dpos + 3] as u32) << 1
            | ((buf[dpos + 4] as u32) >> 7);
        dpos += 5;
        if seg_dur > 0 {
            let dur_secs = seg_dur as f64 / 90000.0;
            fa.set_field(StreamKind::Other, 0, "SegmentationDuration", format!("{:.3}", dur_secs));
        }
    } else {
        seg_dur = 0;
    }
    let _ = seg_dur;

    if dpos + 2 > buf.len() {
        return;
    }
    let upid_len = buf[dpos + 1] as usize;
    let seg_type = buf[dpos];
    let seg_type_name = segmentation_type_name(seg_type);
    dpos += 2;
    fa.set_field(StreamKind::Other, 0, "SegmentationTypeID", seg_type.to_string());
    fa.set_field(StreamKind::Other, 0, "SegmentationTypeName", seg_type_name);

    if upid_len > 0 && dpos + upid_len <= buf.len() {
        let upid_type = buf[dpos];
        let _upid_name = upid_type_name(upid_type);
        dpos += 1;
        if upid_len > 1 && dpos < buf.len() {
            let upid_data = &buf[dpos..buf.len().min(dpos + upid_len - 1)];
            if !upid_data.is_empty()
                && let Ok(s) = std::str::from_utf8(upid_data)
                && !s.is_empty()
                && s.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
            {
                fa.set_field(StreamKind::Other, 0, "UPID", s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_core::FileAnalyze;

    #[test]
    fn scte35_splice_insert() {
        // Minimal SCTE-35 splice_insert() splice_info_section
        // Byte layout matching parser:
        //   [0] table_id, [1-2] section_length,
        //   [3] protocol, [4-7] pts_adjustment(33) + cw_index(6),
        //   [8-9] tier(12), [9-10] splice_command_length(12),
        //   [11] splice_command_type
        let mut buf = vec![
            0xFC, // [0]  table_id
            0x30, 0x00, // [1-2] section_length placeholder
            0x00, // [3]  protocol_version
            0x00, 0x00, 0x00, 0x00, // [4-7] pts_adjustment + cw_index
            0x00, 0x00, // [8-9] tier
            0x10, // [10] splice_command_length lo = 16
            0x05, // [11] splice_command_type = splice_insert
            // splice_insert():
            0x00, 0x00, 0x00, 0x01, // [12-15] splice_event_id=1
            0x00, // [16] cancel=0
            0xE0, // [17] out_of_network=1, program_splice=1, duration=1, immediate=0
            0x00, 0x00, 0x00, 0x00, 0x00, // [18-22] splice_time=0 (33-bit)
            0x00, 0x00, 0x00, 0x00, 0x00, // [23-27] duration=0 (33-bit)
            // descriptor_loop_length
            0x00, 0x00, // [28-29] no descriptors
            // CRC
            0x00, 0x00, 0x00, 0x00, // [30-33]
        ];
        let section_len = buf.len() - 3;
        buf[1] = 0x30 | ((section_len >> 8) as u8) & 0x0F;
        buf[2] = (section_len & 0xFF) as u8;

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_scte35(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Other, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("SCTE 35".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Other, 0, "SpliceCommandName").map(|z| z.as_str().to_owned()),
            Some("splice_insert".into())
        );
    }

    #[test]
    fn scte35_time_signal() {
        // Byte layout: same header as splice_insert, then time_signal command
        // with a segmentation_descriptor containing 4-byte 'CUEI' identifier.
        let mut buf = vec![
            0xFC, 0x30, 0x00, 0x00, // [0-3]: table_id, section_len, protocol
            0x00, 0x00, 0x00, 0x00, // [4-7]: pts_adjustment + cw_index
            0x00, 0x00, // [8-9]: tier=0
            0x05, // [10]: splice_command_length lo = 5
            0x06, // [11]: splice_command_type = time_signal
            0x00, 0x00, 0x00, 0x00, 0x00, // [12-16]: splice_time (33-bit)
            // descriptor_loop_length
            0x00, 0x13, // [17-18]: loop_len = 19
            // segmentation_descriptor (tag=0x02, len=17)
            0x02, 0x11, // [19-20]: tag=2, dlen=17
            0x43, 0x55, 0x45, 0x49, // [21-24]: identifier='CUEI'
            0x00, 0x00, 0x00, 0x01, // [25-28]: seg_event_id=1
            0x00, // [29]: cancel=0
            0x80, // [30]: dur_flag=1
            0x00, 0x00, 0x00, 0x00, 0x00, // [31-35]: seg_duration=0
            0x10, // [36]: seg_type=0x10 (Break)
            0x00, // [37]: upid_len=0
            // CRC
            0x00, 0x00, 0x00, 0x00, // [38-41]
        ];
        let section_len = buf.len() - 3;
        buf[1] = 0x30 | ((section_len >> 8) as u8) & 0x0F;
        buf[2] = (section_len & 0xFF) as u8;

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_scte35(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Other, 0, "SpliceCommandName").map(|z| z.as_str().to_owned()),
            Some("time_signal".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Other, 0, "SegmentationTypeName")
                .map(|z| z.as_str().to_owned()),
            Some("Break".into())
        );
    }

    #[test]
    fn scte35_rejects_invalid_table_id() {
        let buf = vec![0x00; 16];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_scte35(&mut fa));
    }
}
