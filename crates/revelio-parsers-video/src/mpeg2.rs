//! MPEG-2 Video (H.262) codec parser.
//!
//! MPEG-2 is used in DVDs, broadcast TV (DVB), and older digital video formats.
//! This parser handles the sequence header to extract profile, level,
//! dimensions, frame rate, aspect ratio, and bitrate.

use revelio_core::{FileAnalyze, StreamKind};

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
    if data.len() >= 4 && &data[0..3] == &[0x00, 0x00, 0x01] {
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
    let vbv_buffer_size = ((b6 & 0x1F) << 5) | (data.get(offset + 7).copied().unwrap_or(0) as u32 >> 3);
    
    if marker_bit != 1 {
        return None;
    }

    let (frame_rate_num, frame_rate_den) = FRAME_RATE_TABLE.get(frame_rate_code as usize)
        .copied()
        .unwrap_or((0, 1));

    let aspect = ASPECT_RATIO_TABLE.get(aspect_ratio as usize)
        .copied()
        .flatten();

    let mut info = Mpeg2Info {
        width: horizontal_size,
        height: vertical_size,
        frame_rate_numerator: frame_rate_num,
        frame_rate_denominator: frame_rate_den,
        bit_rate: if bit_rate > 0 { Some(bit_rate * 400) } else { None },
        vbv_buffer_size: Some(vbv_buffer_size),
        aspect_ratio: aspect,
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
        if &data[pos..pos + 4] == &[0x00, 0x00, 0x01, 0xB5] {
            if pos + 5 >= data.len() {
                break;
            }
            
            let ext_id = (data[pos + 4] >> 4) & 0x0F;
            
            match ext_id {
                1 => {
                    if pos + 6 < data.len() {
                        parse_sequence_display_extension(&data[pos + 4..], info);
                    }
                }
                2 => {
                    if pos + 6 < data.len() {
                        parse_sequence_scalable_extension(&data[pos + 4..], info);
                    }
                }
                _ => {}
            }
            
            pos += 4;
        } else {
            pos += 1;
        }
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
        let display_horizontal_size = ((data[offset] as u32) << 9) | 
                                      ((data[offset + 1] as u32) << 1) | 
                                      ((data[offset + 2] >> 7) as u32);
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
    
    let scalable_mode = (data[0] >> 5) & 0x03;
    
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
    fa.Stream_Prepare(StreamKind::Video);
    
    fa.Fill(StreamKind::Video, 0, "Format", "MPEG Video", false);
    fa.Fill(StreamKind::Video, 0, "Format_Version", "Version 2", false);
    
    if let Some(profile) = info.profile {
        fa.Fill(StreamKind::Video, 0, "Format_Profile", profile.as_str(), false);
    }
    
    if let Some(level) = info.level {
        if let Some(level_str) = level.as_str() {
            fa.Fill(StreamKind::Video, 0, "Format_Level", level_str, false);
        }
    }
    
    if info.width > 0 {
        fa.Fill(StreamKind::Video, 0, "Width", info.width.to_string(), false);
    }
    
    if info.height > 0 {
        fa.Fill(StreamKind::Video, 0, "Height", info.height.to_string(), false);
    }
    
    if let Some(fr) = info.frame_rate() {
        fa.Fill(StreamKind::Video, 0, "FrameRate", format!("{:.3}", fr), false);
    }
    
    if let Some(bitrate) = info.bit_rate {
        fa.Fill(StreamKind::Video, 0, "BitRate", bitrate.to_string(), false);
    }
    
    if let Some((num, den)) = info.aspect_ratio {
        let dar = num as f64 / den as f64;
        fa.Fill(StreamKind::Video, 0, "DisplayAspectRatio", format!("{:.3}", dar), false);
        fa.Fill(StreamKind::Video, 0, "DisplayAspectRatio/String", format!("{}:{}", num, den), false);
    }
    
    if let Some(chroma) = info.chroma_format_str() {
        fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", chroma, false);
    }
    
    fa.Fill(StreamKind::Video, 0, "ScanType", if info.progressive { "Progressive" } else { "Interlaced" }, false);
}

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
}
