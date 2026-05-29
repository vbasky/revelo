//! APT-X100 (DTS theatrical) sidecar header parser.
//!
//! Mirrors `File_Aptx100.cpp::FileHeader_Parse`. The format has no magic
//! number — detection relies on the first 75 bytes being printable ASCII
//! (Title[60] + Language[8] + Studio[7]) plus structural sanity checks
//! on the trailing fixed-layout fields. Hence the strict signature
//! validation up front: any control byte (other than NUL string
//! terminators) or non-ASCII byte rejects the buffer.

use revelio_core::{FileAnalyze, StreamKind};

const HEADER_SIZE: usize = 0x5C;
const ASCII_REGION: usize = 60 + 8 + 7;

pub fn parse_aptx100(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < HEADER_SIZE {
        return false;
    }
    let head = match fa.peek_raw(HEADER_SIZE) {
        Some(h) => h,
        None => return false,
    };
    for i in 0..ASCII_REGION {
        let b = head[i];
        if (b != 0 && b < 0x20) || b >= 0x7F {
            return false;
        }
    }

    let title = read_nul_string(&head[0..60]);
    let language = read_nul_string(&head[60..68]);
    let mut studio = read_nul_string(&head[68..75]);

    let disc_number = head[75];
    let zero_4c = u16::from_le_bytes([head[76], head[77]]);
    let reel_number = u16::from_le_bytes([head[78], head[79]]);
    let serial = u16::from_le_bytes([head[80], head[81]]);
    let mut channel_count = u16::from_le_bytes([head[82], head[83]]);
    let start_ff_b = head[84];
    let start_ss_b = head[85];
    let start_mm_b = head[86];
    let start_hh_b = head[87];
    let end_ff_b = head[88];
    let end_ss_b = head[89];
    let end_mm_b = head[90];
    let end_hh_b = head[91];

    let start_ff = bcd_to_decimal(start_ff_b);
    let start_ss = bcd_to_decimal(if start_ss_b > 0x60 { start_ss_b - 0x60 } else { start_ss_b });
    let start_mm = bcd_to_decimal(if start_mm_b > 0x60 { start_mm_b - 0x60 } else { start_mm_b });
    let start_hh = bcd_to_decimal(start_hh_b);
    let end_ff = bcd_to_decimal(end_ff_b);
    let end_ss = bcd_to_decimal(if end_ss_b > 0x60 { end_ss_b - 0x60 } else { end_ss_b });
    let end_mm = bcd_to_decimal(if end_mm_b > 0x60 { end_mm_b - 0x60 } else { end_mm_b });
    let end_hh = bcd_to_decimal(end_hh_b);

    let mut is_nok = false;
    // 0x00, 0x01 (Disc A / DVD / USB), 0x81 (Disc B) are the only valid bytes
    // per the C++ switch; everything else rejects.
    match disc_number {
        0x00 | 0x01 | 0x81 => {}
        _ => is_nok = true,
    }
    match channel_count {
        2 | 3 | 4 | 5 | 6 | 8 => {}
        _ => is_nok = true,
    }
    if zero_4c != 0 {
        is_nok = true;
    }
    if start_ff > 99 || start_ss > 59 || start_mm > 59 || start_hh > 23 {
        is_nok = true;
    }
    if end_ff > 99 || end_ss > 59 || end_mm > 59 || end_hh > 23 {
        is_nok = true;
    }
    let start_ms = timecode_to_ms(start_hh, start_mm, start_ss, start_ff);
    let end_ms = timecode_to_ms(end_hh, end_mm, end_ss, end_ff);
    if start_ms >= end_ms {
        is_nok = true;
    }
    if is_nok {
        return false;
    }

    let duration_ms = (end_ms - start_ms) as i64;

    // Consume the header now that validation passed.
    fa.element_begin("APT-X100");
    fa.skip_hexa(HEADER_SIZE, "Header");
    let stream_size = fa.remain() as u64;
    fa.element_end();

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "aptX-100");
    if !title.is_empty() {
        fa.set_field(StreamKind::General, 0, "Movie", title.as_str());
        fa.set_field(StreamKind::General, 0, "Title", title.as_str());
    }
    if studio == "none" {
        studio.clear();
    }
    if !studio.is_empty() {
        fa.set_field(StreamKind::General, 0, "ProductionStudio", studio.as_str());
    }
    if disc_number != 0 {
        fa.set_field(StreamKind::General, 0, "Part_Position", ((disc_number >> 7) + 1).to_string());
    }
    if reel_number != 0 {
        fa.set_field(StreamKind::General, 0, "Reel_Position", reel_number.to_string());
    }
    if serial != 0 {
        fa.set_field(StreamKind::General, 0, "CatalogNumber", serial.to_string());
    }
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "aptX-100");
    // 44100 Hz × 4 bit/channel (16-bit @ 1:4 compression) × channels.
    // Computed before matrixed-channel additions below, per the C++.
    let bit_rate = 44100u32 * 4 * channel_count as u32;
    fa.set_field(StreamKind::Audio, 0, "BitRate", bit_rate.to_string());

    let mut channel_layout = String::new();
    let mut settings = String::new();
    let commercial: &str = match channel_count {
        2 => {
            channel_count += 2;
            channel_layout = "L C R S".to_string();
            settings = "4:2:4 Matrix".to_string();
            "DTS Stereo"
        }
        5 => {
            channel_count += 1;
            channel_layout = "L LS C RS R SW".to_string();
            if serial >= 60000 {
                channel_count += 1;
                channel_layout.push_str(" BS");
                settings = "ES Matrix".to_string();
                "DTS-ES 35 mm"
            } else if (serial == 1357 && language == "*ENG")
                || serial == 11131
                || serial == 12030
                || (serial > 12074 && serial < 21000)
            {
                "DTS 70 mm"
            } else {
                "DTS 35 mm"
            }
        }
        6 => {
            channel_layout = "L LC C RC R S".to_string();
            "DTS 70 mm Special Venue"
        }
        8 => {
            channel_layout = "L LS C RS R SW LC RC".to_string();
            "DTS 70 mm Special Venue"
        }
        _ => "DTS Special Venue",
    };
    fa.set_field(StreamKind::General, 0, "Format_Commercial_IfAny", commercial);
    fa.set_field(StreamKind::Audio, 0, "Format_Commercial_IfAny", commercial);
    if !settings.is_empty() {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings", settings.as_str());
    }
    fa.set_field(StreamKind::Audio, 0, "Channel(s)", channel_count.to_string());
    if !channel_layout.is_empty() {
        fa.set_field(StreamKind::Audio, 0, "ChannelLayout", channel_layout.as_str());
    }
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");
    fa.set_field(StreamKind::Audio, 0, "SamplingRate", "44100");
    fa.set_field(StreamKind::Audio, 0, "Duration", duration_ms.to_string());
    fa.set_field(
        StreamKind::Audio,
        0,
        "TimeCode_FirstFrame",
        format_timecode(start_hh, start_mm, start_ss, start_ff),
    );
    // C++ does `End--` before printing; subtract one frame (1/100 s, since 99fps).
    let (e_hh, e_mm, e_ss, e_ff) = decrement_frame(end_hh, end_mm, end_ss, end_ff);
    fa.set_field(
        StreamKind::Audio,
        0,
        "TimeCode_LastFrame",
        format_timecode(e_hh, e_mm, e_ss, e_ff),
    );

    let language_out = map_language(&language);
    if !language_out.is_empty() {
        fa.set_field(StreamKind::Audio, 0, "Language", language_out.as_str());
    }

    if stream_size > 0 {
        fa.set_field(StreamKind::Audio, 0, "StreamSize", stream_size.to_string());
    }

    true
}

fn read_nul_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn bcd_to_decimal(v: u8) -> u16 {
    let tens = (v >> 4) as u16;
    let units = (v & 0x0F) as u16;
    if tens >= 10 || units >= 10 {
        // Sentinel: any value > 99 will fail the validity check below.
        return u16::MAX;
    }
    tens * 10 + units
}

fn timecode_to_ms(hh: u16, mm: u16, ss: u16, ff: u16) -> i64 {
    // ff is hundredths of a second (99 fps timed).
    (hh as i64) * 3_600_000 + (mm as i64) * 60_000 + (ss as i64) * 1000 + (ff as i64) * 10
}

fn format_timecode(hh: u16, mm: u16, ss: u16, ff: u16) -> String {
    format!("{:02}:{:02}:{:02}:{:02}", hh, mm, ss, ff)
}

fn decrement_frame(hh: u16, mm: u16, ss: u16, ff: u16) -> (u16, u16, u16, u16) {
    if ff > 0 {
        return (hh, mm, ss, ff - 1);
    }
    if ss > 0 {
        return (hh, mm, ss - 1, 99);
    }
    if mm > 0 {
        return (hh, mm - 1, 59, 99);
    }
    if hh > 0 {
        return (hh - 1, 59, 59, 99);
    }
    (hh, mm, ss, ff)
}

fn map_language(language: &str) -> String {
    // C++ only remaps strings starting with '*'; bare strings pass through.
    if language.is_empty() || !language.starts_with('*') {
        return language.to_string();
    }
    let lang_test = &language[1..];
    // Must mirror the sorted Aptx100_Languages table in File_Aptx100.cpp.
    static LANGS: &[(&str, &str)] = &[
        ("ARB", "ar"),
        ("CAN", "yue"),
        ("CRO", "hr"),
        ("CZC", "cs"),
        ("ENGL", "en"),
        ("FLM", "nl-BE"),
        ("FRN", "fr"),
        ("FRNC", "fr-CA"),
        ("GERS", "de-CH"),
        ("GRK", "el"),
        ("ITL", "it"),
        ("JAP", "ja"),
        ("PORB", "pt-BR"),
        ("SLO", "sl"),
        ("SLV", "sk"),
        ("SPNC", "es"),
        ("SPNL", "es-419"),
        ("SWD", "sv"),
        ("TRK", "tr"),
        ("VTN", "vi"),
        ("YUG", "-YU"),
    ];
    for (from, to) in LANGS {
        if *from == lang_test {
            return (*to).to_string();
        }
    }
    // Unmapped: drop the leading '*'.
    lang_test.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)] // fixture builder mirrors the binary header layout
    fn build_header(
        title: &str,
        language: &str,
        studio: &str,
        disc: u8,
        reel: u16,
        serial: u16,
        channels: u16,
        start: (u8, u8, u8, u8),
        end: (u8, u8, u8, u8),
        audio_size: usize,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; HEADER_SIZE];
        let t = title.as_bytes();
        buf[..t.len().min(60)].copy_from_slice(&t[..t.len().min(60)]);
        let l = language.as_bytes();
        buf[60..60 + l.len().min(8)].copy_from_slice(&l[..l.len().min(8)]);
        let s = studio.as_bytes();
        buf[68..68 + s.len().min(7)].copy_from_slice(&s[..s.len().min(7)]);
        buf[75] = disc;
        buf[76..78].copy_from_slice(&0u16.to_le_bytes());
        buf[78..80].copy_from_slice(&reel.to_le_bytes());
        buf[80..82].copy_from_slice(&serial.to_le_bytes());
        buf[82..84].copy_from_slice(&channels.to_le_bytes());
        buf[84] = to_bcd(start.3); // FF
        buf[85] = to_bcd(start.2); // SS
        buf[86] = to_bcd(start.1); // MM
        buf[87] = to_bcd(start.0); // HH
        buf[88] = to_bcd(end.3);
        buf[89] = to_bcd(end.2);
        buf[90] = to_bcd(end.1);
        buf[91] = to_bcd(end.0);
        buf.resize(HEADER_SIZE + audio_size, 0);
        buf
    }

    fn to_bcd(v: u8) -> u8 {
        ((v / 10) << 4) | (v % 10)
    }

    #[test]
    fn parses_stereo_header() {
        let buf = build_header(
            "TestMovie",
            "*ENG",
            "Studio1",
            0x01,
            1,
            42,
            2,
            (0, 0, 0, 0),
            (0, 1, 0, 0),
            1000,
        );
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aptx100(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("aptX-100"));
        assert_eq!(g("Title").as_deref(), Some("TestMovie"));
        assert_eq!(g("ProductionStudio").as_deref(), Some("Studio1"));
        assert_eq!(g("CatalogNumber").as_deref(), Some("42"));
        assert_eq!(a("Format").as_deref(), Some("aptX-100"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        // Stereo: ChannelCount becomes 4 (2 + matrixed C S).
        assert_eq!(a("Channel(s)").as_deref(), Some("4"));
        assert_eq!(a("ChannelLayout").as_deref(), Some("L C R S"));
        assert_eq!(a("Format_Settings").as_deref(), Some("4:2:4 Matrix"));
        // BitRate uses the *original* (pre-matrix) channel count: 44100*4*2.
        assert_eq!(a("BitRate").as_deref(), Some("352800"));
        // 1 minute duration.
        assert_eq!(a("Duration").as_deref(), Some("60000"));
        // *ENG → unmapped, leading '*' stripped.
        assert_eq!(a("Language").as_deref(), Some("ENG"));
        assert_eq!(a("StreamSize").as_deref(), Some("1000"));
    }

    #[test]
    fn parses_5ch_dts_70mm_variant() {
        // serial=11131 triggers DTS 70 mm classification at 5 channels.
        let buf = build_header(
            "Movie",
            "ENG",
            "none",
            0x81,
            2,
            11131,
            5,
            (1, 0, 0, 0),
            (1, 30, 0, 0),
            500,
        );
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aptx100(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channel(s)").as_deref(), Some("6"));
        assert_eq!(a("ChannelLayout").as_deref(), Some("L LS C RS R SW"));
        assert_eq!(a("Format_Commercial_IfAny").as_deref(), Some("DTS 70 mm"));
        // Disc 0x81 → Part_Position = (0x81>>7)+1 = 2.
        assert_eq!(g("Part_Position").as_deref(), Some("2"));
        assert_eq!(g("Reel_Position").as_deref(), Some("2"));
        // "none" studio is suppressed.
        assert!(g("ProductionStudio").is_none());
    }

    #[test]
    fn rejects_invalid_header() {
        // All-zero buffer → start_ms == end_ms == 0 → rejected.
        let buf = vec![0u8; HEADER_SIZE + 10];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_aptx100(&mut fa));

        // Short buffer → rejected.
        let mut fa2 = FileAnalyze::new(&[0u8; 10]);
        assert!(!parse_aptx100(&mut fa2));

        // Non-ASCII byte in title region → rejected.
        let mut buf3 =
            build_header("Movie", "ENG", "Studio", 0x01, 1, 1, 2, (0, 0, 0, 0), (0, 1, 0, 0), 100);
        buf3[5] = 0xFF;
        let mut fa3 = FileAnalyze::new(&buf3);
        assert!(!parse_aptx100(&mut fa3));
    }
}
