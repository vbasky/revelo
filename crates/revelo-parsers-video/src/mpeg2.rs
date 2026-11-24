//! MPEG-2 Video (H.262) codec parser.
//!
//! MPEG-2 is used in DVDs, broadcast TV (DVB), and older digital video formats.
//! This parser handles the sequence header to extract profile, level,
//! dimensions, frame rate, aspect ratio, and bitrate.

use revelo_core::{FileAnalyze, StreamKind};

/// MPEG-2 profile identifiers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mpeg2Profile {
    Simple,
    Main,
    SNRScalable,
    SpatiallyScalable,
    High,
    Multiview,
}

impl Mpeg2Profile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mpeg2Profile::Simple => "Simple",
            Mpeg2Profile::Main => "Main",
            Mpeg2Profile::SNRScalable => "SNR Scalable",
            Mpeg2Profile::SpatiallyScalable => "Spatially Scalable",
            Mpeg2Profile::High => "High",
            Mpeg2Profile::Multiview => "Multiview",
        }
    }
}

/// MPEG-2 level identifiers.
#[derive(Debug, Clone, Copy)]
pub struct Mpeg2Level(pub u8);

impl Mpeg2Level {
    pub fn as_str(&self) -> Option<&'static str> {
        match self.0 {
            4 => Some("Low"),
            8 => Some("Main"),
            10 => Some("High 1440"),
            14 => Some("High"),
            _ => None,
        }
    }
}

/// Parsed MPEG-2 sequence header information.
#[derive(Debug, Default)]
pub struct Mpeg2Info {
    pub profile: Option<Mpeg2Profile>,
    pub level: Option<Mpeg2Level>,
    pub width: u32,
    pub height: u32,
    pub frame_rate_numerator: u32,
    pub frame_rate_denominator: u32,
    pub bit_rate: Option<u32>,
    pub vbv_buffer_size: Option<u32>,
    pub progressive: bool,
    pub chroma_format: u8,
    pub aspect_ratio: Option<(u16, u16)>,
    /// Sequence-header bitrate code is the VBR escape `0x3FFFF`.
    pub vbr: bool,
    /// 18-bit `bit_rate_value` from the sequence header, before the ×400
    /// conversion. `0x3FFFF` is the escape and is not a bitrate.
    pub bit_rate_code: u32,
}

impl Mpeg2Info {
    pub fn frame_rate(&self) -> Option<f64> {
        if self.frame_rate_denominator > 0 {
            Some(self.frame_rate_numerator as f64 / self.frame_rate_denominator as f64)
        } else {
            None
        }
    }

    pub fn chroma_format_str(&self) -> Option<&'static str> {
        match self.chroma_format {
            1 => Some("4:2:0"),
            2 => Some("4:2:2"),
            3 => Some("4:4:4"),
            _ => None,
        }
    }
}

pub fn parse_mpeg2_sequence_header(data: &[u8]) -> Option<Mpeg2Info> {
    if data.len() < 8 {
        return None;
    }

    let offset;
    if data.len() >= 4 && data[0..3] == [0x00, 0x00, 0x01] {
        if data[3] != 0xB3 {
            return None;
        }
        offset = 4;
    } else if data[0] == 0xB3 {
        offset = 1;
    } else {
        return None;
    }

    if offset + 7 > data.len() {
        return None;
    }

    let b0 = data[offset] as u32;
    let b1 = data[offset + 1] as u32;
    let b2 = data[offset + 2] as u32;
    let b3 = data[offset + 3] as u32;
    let b4 = data[offset + 4] as u32;
    let b5 = data[offset + 5] as u32;
    let b6 = data[offset + 6] as u32;

    let horizontal_size = ((b0 << 4) | (b1 >> 4)) & 0xFFF;
    let vertical_size = (((b1 & 0x0F) << 8) | b2) & 0xFFF;
    let aspect_ratio = (b3 >> 4) & 0x0F;
    let frame_rate_code = b3 & 0x0F;
    let bit_rate = ((b4 << 10) | (b5 << 2) | (b6 >> 6)) & 0x3FFFF;
    let marker_bit = (b6 >> 5) & 0x01;
    let vbv_buffer_size =
        ((b6 & 0x1F) << 5) | (data.get(offset + 7).copied().unwrap_or(0) as u32 >> 3);

    if marker_bit != 1 {
        return None;
    }

    let (frame_rate_num, frame_rate_den) =
        FRAME_RATE_TABLE.get(frame_rate_code as usize).copied().unwrap_or((0, 1));

    let aspect = ASPECT_RATIO_TABLE.get(aspect_ratio as usize).copied().flatten();

    let mut info = Mpeg2Info {
        width: horizontal_size,
        height: vertical_size,
        frame_rate_numerator: frame_rate_num,
        frame_rate_denominator: frame_rate_den,
        bit_rate: if bit_rate > 0 && bit_rate != 0x3FFFF { Some(bit_rate * 400) } else { None },
        vbv_buffer_size: Some(vbv_buffer_size),
        aspect_ratio: aspect,
        vbr: bit_rate == 0x3FFFF,
        bit_rate_code: bit_rate,
        ..Default::default()
    };

    if offset + 8 < data.len() {
        parse_extensions(&data[offset + 8..], &mut info);
    }

    Some(info)
}

fn parse_extensions(data: &[u8], info: &mut Mpeg2Info) {
    let mut pos = 0;

    while pos + 4 < data.len() {
        if data[pos..pos + 4] == [0x00, 0x00, 0x01, 0xB5] {
            if pos + 5 >= data.len() {
                break;
            }

            let ext_id = (data[pos + 4] >> 4) & 0x0F;

            match ext_id {
                // ISO/IEC 13818-2 extension ids: 1 sequence, 2 display, 5 scalable.
                1 if pos + 6 < data.len() => {
                    parse_sequence_extension(&data[pos + 4..], info);
                }
                2 if pos + 6 < data.len() => {
                    parse_sequence_display_extension(&data[pos + 4..], info);
                }
                5 if pos + 6 < data.len() => {
                    parse_sequence_scalable_extension(&data[pos + 4..], info);
                }
                _ => {}
            }

            pos += 4;
        } else {
            pos += 1;
        }
    }
}

fn parse_sequence_extension(data: &[u8], info: &mut Mpeg2Info) {
    let mut bits = ExtBits { data, bit: 0 };
    let Some(parsed) = (|| {
        let _ext_id = bits.take(4)?;
        let profile_and_level = bits.take(8)? as u8;
        let progressive = bits.take(1)? != 0;
        let chroma = bits.take(2)? as u8;
        let horizontal_ext = bits.take(2)?;
        let vertical_ext = bits.take(2)?;
        let bit_rate_ext = bits.take(12)?;
        let _marker = bits.take(1)?;
        let _vbv_ext = bits.take(8)?;
        Some((profile_and_level, progressive, chroma, horizontal_ext, vertical_ext, bit_rate_ext))
    })() else {
        return;
    };
    let (profile_and_level, progressive, chroma, horizontal_ext, vertical_ext, bit_rate_ext) =
        parsed;
    info.progressive = progressive;
    if (1..=3).contains(&chroma) {
        info.chroma_format = chroma;
    }
    info.width |= horizontal_ext << 12;
    info.height |= vertical_ext << 12;
    let profile_nibble = profile_and_level >> 4;
    let level_nibble = profile_and_level & 0x0F;
    if let Some(profile) = match profile_nibble {
        1 => Some(Mpeg2Profile::High),
        2 => Some(Mpeg2Profile::SpatiallyScalable),
        3 => Some(Mpeg2Profile::SNRScalable),
        4 => Some(Mpeg2Profile::Main),
        5 => Some(Mpeg2Profile::Simple),
        _ => None,
    } {
        info.profile = Some(profile);
    }
    if Mpeg2Level(level_nibble).as_str().is_some() {
        info.level = Some(Mpeg2Level(level_nibble));
    }
    if info.bit_rate_code != 0 && info.bit_rate_code != 0x3FFFF && bit_rate_ext != 0 {
        info.bit_rate = Some(((bit_rate_ext << 18) | info.bit_rate_code) * 400);
    }
}

struct ExtBits<'a> {
    data: &'a [u8],
    bit: usize,
}

impl ExtBits<'_> {
    fn take(&mut self, n: usize) -> Option<u32> {
        if n > 24 || self.bit + n > self.data.len() * 8 {
            return None;
        }
        let mut v = 0u32;
        for _ in 0..n {
            let byte = self.data[self.bit / 8];
            v = (v << 1) | u32::from((byte >> (7 - (self.bit % 8))) & 1);
            self.bit += 1;
        }
        Some(v)
    }
}

fn parse_sequence_display_extension(data: &[u8], info: &mut Mpeg2Info) {
    if data.len() < 3 {
        return;
    }

    let colour_description = data[0] & 0x01;

    let mut offset = 1;
    if colour_description != 0 && data.len() >= offset + 3 {
        offset += 3;
    }

    if data.len() >= offset + 3 {
        let display_horizontal_size = ((data[offset] as u32) << 9)
            | ((data[offset + 1] as u32) << 1)
            | ((data[offset + 2] >> 7) as u32);
        let display_vertical_size = ((data[offset + 2] & 0x7F) as u32) << 5;

        if display_horizontal_size > 0 && display_vertical_size > 0 && info.height > 0 {
            let dar_num = display_horizontal_size * info.height;
            let dar_den = display_vertical_size * info.width;
            let gcd_val = gcd(dar_num, dar_den);
            info.aspect_ratio = Some(((dar_num / gcd_val) as u16, (dar_den / gcd_val) as u16));
        }
    }
}

fn parse_sequence_scalable_extension(data: &[u8], info: &mut Mpeg2Info) {
    if data.len() < 2 {
        return;
    }

    // scalable_mode is the 2 bits immediately after the 4-bit extension id.
    let scalable_mode = (data[0] >> 2) & 0x03;

    info.profile = match scalable_mode {
        0 => Some(Mpeg2Profile::SNRScalable),
        1 => Some(Mpeg2Profile::SpatiallyScalable),
        3 => Some(Mpeg2Profile::Multiview),
        _ => None,
    };
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

const FRAME_RATE_TABLE: [(u32, u32); 16] = [
    (0, 1),
    (24000, 1001),
    (24, 1),
    (25, 1),
    (30000, 1001),
    (30, 1),
    (50, 1),
    (60000, 1001),
    (60, 1),
    (0, 1),
    (0, 1),
    (0, 1),
    (0, 1),
    (0, 1),
    (0, 1),
    (0, 1),
];

const ASPECT_RATIO_TABLE: [Option<(u16, u16)>; 16] = [
    None,
    Some((1, 1)),
    Some((4, 3)),
    Some((16, 9)),
    Some((221, 100)),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
];

pub fn fill_mpeg2_streams(fa: &mut FileAnalyze, info: &Mpeg2Info) {
    fa.stream_prepare(StreamKind::Video);

    fa.set_field(StreamKind::Video, 0, "Format", "MPEG Video");
    fa.set_field(StreamKind::Video, 0, "Format_Version", "Version 2");

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

    if info.vbr {
        fa.set_field(StreamKind::Video, 0, "BitRate_Mode", "VBR");
    }

    if let Some(bitrate) = info.bit_rate {
        fa.set_field(StreamKind::Video, 0, "BitRate", bitrate.to_string());
    }

    if let Some((num, den)) = info.aspect_ratio {
        let dar = num as f64 / den as f64;
        fa.set_field(StreamKind::Video, 0, "DisplayAspectRatio", format!("{:.3}", dar));
        fa.set_field(StreamKind::Video, 0, "DisplayAspectRatio/String", format!("{}:{}", num, den));
    }

    if let Some(chroma) = info.chroma_format_str() {
        fa.set_field(StreamKind::Video, 0, "ChromaSubsampling", chroma);
    }

    fa.set_field(
        StreamKind::Video,
        0,
        "ScanType",
        if info.progressive { "Progressive" } else { "Interlaced" },
    );
}

/// Parse MPEG-2 Video (H.262) elementary stream.
///
/// Detection: Sequence header start code 0x000001B3.
/// Fills: Profile/level, dimensions, aspect ratio, frame rate code.
pub fn parse_mpeg2(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };

    let is_mpeg2 = h == [0x00, 0x00, 0x01, 0xB3];

    if !is_mpeg2 {
        return false;
    }

    let data = fa.peek_raw(256);
    let Some(data) = data else { return false };

    if let Some(info) = parse_mpeg2_sequence_header(data) {
        fill_mpeg2_streams(fa, &info);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sequence_header() {
        // MPEG-2 sequence header with start code 0x000001B3
        // Just verify the parser accepts valid data with marker bit set
        let data = vec![
            0x00, 0x00, 0x01, 0xB3, // Start code
            0x14, 0x00, // horizontal_size = 320 (0x140)
            0xF0, // vertical_size = 240 (0x0F0) - note: combined with bits from prev byte
            0x24, // aspect_ratio=2, frame_rate=4
            0x00, 0x00, // bit_rate = 0
            0x20, // marker_bit=1 (bit 5)
            0x00, // vbv + flags
        ];

        let info = parse_mpeg2_sequence_header(&data);
        assert!(info.is_some());
        let info = info.unwrap();
        // Verify we got some reasonable values
        assert!(info.width > 0);
        assert!(info.height > 0);
    }

    #[test]
    fn sequence_extension_is_progressive_main_and_vbr_escape_has_no_bitrate() {
        // 320×240 sequence header with bitrate code 0x3FFFF, then a sequence
        // extension whose profile_and_level is 0x48 (Main@Main), progressive,
        // chroma 4:2:0.
        let data = [
            0x00, 0x00, 0x01, 0xB3, 0x14, 0x00, 0xF0, 0x21, 0xFF, 0xFF, 0xE0, 0x18, 0x00, 0x00,
            0x01, 0xB5, 0x14, 0x8A, 0x00, 0x01, 0x00, 0x00,
        ];
        let info = parse_mpeg2_sequence_header(&data).expect("header");
        assert_eq!(info.width, 320);
        assert_eq!(info.height, 240);
        assert!(info.progressive);
        assert!(info.vbr);
        assert_eq!(info.bit_rate, None);
        assert_eq!(info.profile, Some(Mpeg2Profile::Main));
        assert_eq!(info.level.map(|l| l.0), Some(8));
        assert_eq!(info.chroma_format, 1);

        let mut fa = FileAnalyze::new(&data);
        fill_mpeg2_streams(&mut fa, &info);
        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(v("ScanType").as_deref(), Some("Progressive"));
        assert_eq!(v("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(v("BitRate"), None);
        assert_eq!(v("Format_Profile").as_deref(), Some("Main"));
        assert_eq!(v("Format_Level").as_deref(), Some("Main"));
        assert_eq!(v("ChromaSubsampling").as_deref(), Some("4:2:0"));
    }
}
