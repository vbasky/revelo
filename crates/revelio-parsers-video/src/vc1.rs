//! VC-1 (SMPTE 421M) video codec parser.
//!
//! VC-1 is used in WMV (Windows Media Video) and some Blu-ray formats.
//! This parser handles the sequence header to extract profile, level,
//! dimensions, frame rate, and other metadata.

use revelio_core::{FileAnalyze, StreamKind};

/// VC-1 profile identifiers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Vc1Profile {
    Simple,
    Main,
    Complex, // Rarely used
    Advanced,
}

impl Vc1Profile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Vc1Profile::Simple => "Simple",
            Vc1Profile::Main => "Main",
            Vc1Profile::Complex => "Complex",
            Vc1Profile::Advanced => "Advanced",
        }
    }
}

/// VC-1 level identifiers.
#[derive(Debug, Clone, Copy)]
pub struct Vc1Level(pub u8);

impl Vc1Level {
    pub fn as_str(&self) -> Option<&'static str> {
        match self.0 {
            0 => Some("Low"),
            1 => Some("Medium"),
            2 => Some("High"),
            3 => Some("L0"),
            4 => Some("L1"),
            5 => Some("L2"),
            6 => Some("L3"),
            7 => Some("L4"),
            _ => None,
        }
    }
}

/// Parsed VC-1 sequence header information.
#[derive(Debug, Default)]
pub struct Vc1Info {
    pub profile: Option<Vc1Profile>,
    pub level: Option<Vc1Level>,
    pub width: u32,
    pub height: u32,
    pub frame_rate_numerator: u32,
    pub frame_rate_denominator: u32,
    pub bit_rate: Option<u32>,
    pub interlace: bool,
    pub max_b_frames: u8,
}

impl Vc1Info {
    /// Calculate frame rate as f64.
    pub fn frame_rate(&self) -> Option<f64> {
        if self.frame_rate_denominator > 0 {
            Some(self.frame_rate_numerator as f64 / self.frame_rate_denominator as f64)
        } else {
            None
        }
    }
}

/// Parse raw VC-1 sequence header bytes (from codec private or elementary stream).
/// The sequence header format varies by profile.
pub fn parse_vc1_sequence_header(data: &[u8]) -> Option<Vc1Info> {
    if data.len() < 5 {
        return None;
    }

    let mut offset = 0;

    // Check for start code prefix
    if data.len() >= 4 && data[0..3] == [0x00, 0x00, 0x01] {
        offset = 4;
    }

    if offset >= data.len() {
        return None;
    }

    let profile_byte = data[offset];
    let profile_idx = (profile_byte >> 6) & 0x3;
    let level_idx = (profile_byte >> 3) & 0x7;

    let profile = match profile_idx {
        0 => Vc1Profile::Simple,
        1 => Vc1Profile::Main,
        2 => Vc1Profile::Complex,
        3 => Vc1Profile::Advanced,
        _ => return None,
    };

    let mut info =
        Vc1Info { profile: Some(profile), level: Some(Vc1Level(level_idx)), ..Default::default() };

    match profile {
        Vc1Profile::Simple | Vc1Profile::Main => {
            parse_simple_main_header(&data[offset..], &mut info)?;
        }
        Vc1Profile::Advanced => {
            parse_advanced_header(&data[offset..], &mut info)?;
        }
        Vc1Profile::Complex => {
            parse_simple_main_header(&data[offset..], &mut info)?;
        }
    }

    Some(info)
}

fn parse_simple_main_header(data: &[u8], info: &mut Vc1Info) -> Option<()> {
    if data.len() < 4 {
        return None;
    }

    let byte1 = data[1];
    info.interlace = (byte1 & 0x80) != 0;
    info.max_b_frames = byte1 & 0x07;

    let wh = ((data[1] as u32) << 16) | ((data[2] as u32) << 8) | (data[3] as u32);
    let coded_width = ((wh >> 12) & 0xFFF) + 1;
    let coded_height = (wh & 0xFFF) + 1;

    info.width = coded_width * 2;
    info.height = coded_height * 2;

    if data.len() >= 8 {
        let fr_byte = data[4];
        let fr_num_idx = (fr_byte >> 4) & 0xF;
        let fr_den_idx = fr_byte & 0xF;

        info.frame_rate_numerator =
            FRAME_RATE_NUMERATORS.get(fr_num_idx as usize).copied().unwrap_or(0);
        info.frame_rate_denominator =
            FRAME_RATE_DENOMINATORS.get(fr_den_idx as usize).copied().unwrap_or(1);
    }

    Some(())
}

fn parse_advanced_header(data: &[u8], info: &mut Vc1Info) -> Option<()> {
    if data.len() < 10 {
        return None;
    }

    info.width = ((data[1] as u32) << 8) | (data[2] as u32);
    info.height = ((data[3] as u32) << 8) | (data[4] as u32);

    info.interlace = (data[5] & 0x80) != 0;

    if data.len() >= 12 {
        info.frame_rate_numerator = ((data[6] as u32) << 8) | (data[7] as u32);
        info.frame_rate_denominator = ((data[8] as u32) << 8) | (data[9] as u32);
    }

    Some(())
}

const FRAME_RATE_NUMERATORS: [u32; 16] =
    [0, 24000, 24, 25, 30000, 30, 50, 60000, 60, 1, 1, 1, 1, 1, 1, 1];

const FRAME_RATE_DENOMINATORS: [u32; 16] =
    [1, 1001, 1, 1, 1001, 1, 1, 1001, 1, 1, 1, 1, 1, 1, 1, 1];

pub fn parse_vc1_codec_private(data: &[u8]) -> Option<Vc1Info> {
    if data.is_empty() {
        return None;
    }

    let header_len = data[0] as usize;
    if header_len > 0 && header_len < data.len() {
        parse_vc1_sequence_header(&data[1..=header_len])
    } else {
        parse_vc1_sequence_header(data)
    }
}

pub fn fill_vc1_streams(fa: &mut FileAnalyze, info: &Vc1Info) {
    fa.stream_prepare(StreamKind::Video);

    fa.set_field(StreamKind::Video, 0, "Format", "VC-1");

    if let Some(profile) = info.profile {
        fa.set_field(StreamKind::Video, 0, "Format_Profile", profile.as_str());
    }

    if let Some(level) = info.level
        && let Some(level_str) = level.as_str()
    {
        fa.set_field(StreamKind::Video, 0, "Format_Level", level_str);
    }

    if info.width > 0 {
        fa.set_field(StreamKind::Video, 0, "Width", info.width.to_string());
    }

    if info.height > 0 {
        fa.set_field(StreamKind::Video, 0, "Height", info.height.to_string());
    }

    if let Some(fr) = info.frame_rate() {
        fa.set_field(StreamKind::Video, 0, "FrameRate", format!("{:.3}", fr));
    }

    if let Some(bitrate) = info.bit_rate {
        fa.set_field(StreamKind::Video, 0, "BitRate", bitrate.to_string());
    }

    fa.set_field(
        StreamKind::Video,
        0,
        "ScanType",
        if info.interlace { "Interlaced" } else { "Progressive" },
    );
}

/// Parse VC-1 (SMPTE 421M) elementary stream.
///
/// Detection: Sequence header 0x0000010F or advanced entry point 0x0000010E.
/// Fills: Profile, level, dimensions, chroma format.
pub fn parse_vc1(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };

    let is_vc1 = (h == [0x00, 0x00, 0x01, 0x0F])
        || (h == [0x00, 0x00, 0x01, 0x0E])
        || (h == [0x00, 0x00, 0x01, 0x0D]);

    if !is_vc1 {
        return false;
    }

    let data = fa.peek_raw(32);
    let Some(data) = data else { return false };

    if let Some(info) = parse_vc1_sequence_header(data) {
        fill_vc1_streams(fa, &info);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_profile_header() {
        let data = vec![0x00, 0x00, 0x01, 0x0F, 0x00, 0x00, 0x01, 0x01];

        let info = parse_vc1_sequence_header(&data);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.profile, Some(Vc1Profile::Simple));
        assert!(!info.interlace);
    }

    #[test]
    fn test_parse_advanced_profile_header() {
        let data = vec![
            0x00, 0x00, 0x01, 0x0F, 0xC0, 0x02, 0x80, 0x01, 0xE0, 0x00, 0x00, 0x1E, 0x00, 0x01,
        ];

        let info = parse_vc1_sequence_header(&data);
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.profile, Some(Vc1Profile::Advanced));
        assert_eq!(info.width, 640);
        assert_eq!(info.height, 480);
    }
}
