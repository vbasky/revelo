//! TIFF (Tag Image File Format) parser.
//!
//! Header (8 bytes):
//!   "II" (LE) or "MM" (BE)  — byte order
//!   2 bytes: magic 0x002A
//!   4 bytes: offset to first IFD
//!
//! IFD (Image File Directory):
//!   2 bytes: entry count N
//!   N × 12-byte entries (tag/type/count/value-or-offset)
//!   4 bytes: offset to next IFD (0 = none)
//!
//! Entry value layout: if `count × type_size ≤ 4`, the value lives in
//! the entry's value field directly. Otherwise the value field is a
//! file offset pointing to the data.

use revelio_core::{FileAnalyze, StreamKind};

const TAG_SUBFILE_TYPE: u16 = 254;
const TAG_IMAGE_WIDTH: u16 = 256;
const TAG_IMAGE_LENGTH: u16 = 257;
const TAG_BITS_PER_SAMPLE: u16 = 258;
const TAG_COMPRESSION: u16 = 259;
const TAG_PHOTOMETRIC: u16 = 262;
const TAG_MAKE: u16 = 271;
const TAG_MODEL: u16 = 272;
const TAG_SOFTWARE: u16 = 305;
const TAG_ARTIST: u16 = 315;
const TAG_COPYRIGHT: u16 = 33432;
const TAG_DATE_TIME: u16 = 306;
const TAG_IMAGE_DESCRIPTION: u16 = 270;
const TAG_X_RESOLUTION: u16 = 282;
const TAG_Y_RESOLUTION: u16 = 283;
const TAG_RESOLUTION_UNIT: u16 = 296;
const TAG_EXTRA_SAMPLES: u16 = 338;
const TAG_ORIENTATION: u16 = 274;
const TAG_SAMPLES_PER_PIXEL: u16 = 277;
const TAG_ROWS_PER_STRIP: u16 = 278;
const TAG_PLANAR_CONFIG: u16 = 284;
const TAG_SAMPLE_FORMAT: u16 = 339;

#[derive(Default)]
struct Ifd {
    width: u32,
    height: u32,
    bits_per_sample: u32,
    compression: u32,
    photometric: u32,
    extra_samples: u32,
    samples_per_pixel: u32,
    rows_per_strip: u32,
    planar_config: u32,
    sample_format: u32,
    orientation: u32,
    make: Option<String>,
    model: Option<String>,
    software: Option<String>,
    artist: Option<String>,
    copyright: Option<String>,
    date_time: Option<String>,
    image_description: Option<String>,
    x_resolution: Option<(u32, u32)>,
    y_resolution: Option<(u32, u32)>,
    resolution_unit: u32,
    is_thumbnail: bool,
}

pub fn parse_tiff(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(8);
    let Some(h) = head else { return false };
    let little_endian = match (h[0], h[1]) {
        (b'I', b'I') => true,
        (b'M', b'M') => false,
        _ => return false,
    };
    let magic = read_u16(h, 2, little_endian);
    if magic != 0x002A {
        return false;
    }
    let first_ifd_offset = read_u32(h, 4, little_endian) as usize;
    let total = fa.remain();
    let buf = match fa.peek_raw(total) {
        Some(b) => b,
        None => return false,
    };

    let ifd = match read_ifd(buf, first_ifd_offset, little_endian) {
        Some(i) => i,
        None => return false,
    };

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "TIFF", false);
    fa.fill(StreamKind::General, 0, "ImageCount", "1", false);
    if let Some(ref s) = ifd.software {
        // TIFF Software → both Encoded_Application_Name and the composed
        // Encoded_Application (oracle emits both, derived from the same
        // string when no version info is separately available).
        fa.fill(StreamKind::General, 0, "Encoded_Application", s.clone(), false);
        fa.fill(StreamKind::General, 0, "Encoded_Application_Name", s.clone(), false);
    }
    if let Some(ref s) = ifd.make {
        fa.fill(StreamKind::General, 0, "Make", s.clone(), false);
    }
    if let Some(ref s) = ifd.model {
        fa.fill(StreamKind::General, 0, "Model", s.clone(), false);
    }
    if let Some(ref s) = ifd.artist {
        fa.fill(StreamKind::General, 0, "Artist", s.clone(), false);
    }
    if let Some(ref s) = ifd.copyright {
        fa.fill(StreamKind::General, 0, "Copyright", s.clone(), false);
    }
    if let Some(ref s) = ifd.date_time {
        fa.fill(StreamKind::General, 0, "DateTime", s.clone(), false);
    }
    if let Some(ref s) = ifd.image_description {
        fa.fill(StreamKind::General, 0, "Description", s.clone(), false);
    }

    fa.stream_prepare(StreamKind::Image);
    let endianness = if little_endian { "Little" } else { "Big" };
    fa.fill(StreamKind::Image, 0, "Format", "TIFF", false);
    fa.fill(StreamKind::Image, 0, "Format_Settings_Endianness", endianness, false);
    if ifd.is_thumbnail {
        fa.fill(StreamKind::Image, 0, "Type", "Thumbnail", false);
    }
    if ifd.width > 0 {
        fa.fill(StreamKind::Image, 0, "Width", ifd.width.to_string(), false);
    }
    if ifd.height > 0 {
        fa.fill(StreamKind::Image, 0, "Height", ifd.height.to_string(), false);
    }
    if ifd.bits_per_sample > 0 {
        fa.fill(StreamKind::Image, 0, "BitDepth", ifd.bits_per_sample.to_string(), false);
    }
    if ifd.compression > 0 {
        let name = tiff_compression_name(ifd.compression);
        if !name.is_empty() {
            fa.fill(StreamKind::Image, 0, "Format", name, true);
        }
        let cm = tiff_compression_mode(ifd.compression);
        if !cm.is_empty() {
            fa.fill(StreamKind::Image, 0, "Compression_Mode", cm, false);
        }
    }
    if ifd.photometric > 0 {
        let mut cs = photometric_color_space(ifd.photometric).to_string();
        if ifd.extra_samples == 1 && !cs.ends_with('A') {
            cs.push('A');
        }
        if !cs.is_empty() {
            fa.fill(StreamKind::Image, 0, "ColorSpace", cs, false);
        }
    }

    // Density / resolution → <extra> block.
    let unit_str = match ifd.resolution_unit {
        1 => "dpcm",
        2 => "dpi",
        0 => "",
        _ if ifd.x_resolution.is_some() || ifd.y_resolution.is_some() => "dpi",
        _ => "",
    };
    let format_rat = |(n, d): (u32, u32)| -> String {
        if d == 0 { n.to_string() } else if n % d == 0 { (n / d).to_string() } else { format!("{:.3}", n as f64 / d as f64) }
    };
    if let Some(x) = ifd.x_resolution {
        fa.fill(StreamKind::Image, 0, "Density_X", format_rat(x), false);
    }
    if let Some(y) = ifd.y_resolution {
        fa.fill(StreamKind::Image, 0, "Density_Y", format_rat(y), false);
    }
    if !unit_str.is_empty() {
        fa.fill(StreamKind::Image, 0, "Density_Unit", unit_str, false);
    }
    if ifd.x_resolution.is_some() || ifd.y_resolution.is_some() {
        // Density_String mirrors the C++'s "Density/String" composed
        // value: "X[xY] unit", omitting Y when X==Y.
        let xs = ifd.x_resolution.map(format_rat).unwrap_or_else(|| "?".into());
        let ys = ifd.y_resolution.map(format_rat).unwrap_or_else(|| "?".into());
        let val_part = if xs == ys { xs.clone() } else { format!("{}x{}", xs, ys) };
        if !unit_str.is_empty() {
            fa.fill(
                StreamKind::Image,
                0,
                "Density_String",
                format!("{} {}", val_part, unit_str),
                false,
            );
        }
    }
    true
}

fn read_ifd(buf: &[u8], offset: usize, le: bool) -> Option<Ifd> {
    if offset + 2 > buf.len() {
        return None;
    }
    let entry_count = read_u16(buf, offset, le) as usize;
    let mut ifd = Ifd::default();
    for i in 0..entry_count {
        let entry_off = offset + 2 + i * 12;
        if entry_off + 12 > buf.len() {
            break;
        }
        let tag = read_u16(buf, entry_off, le);
        let entry_type = read_u16(buf, entry_off + 2, le);
        let count = read_u32(buf, entry_off + 4, le) as usize;
        let val_field = entry_off + 8;
        let type_size = match entry_type {
            1 | 2 | 7 => 1,
            3 => 2,
            4 | 11 => 4,
            5 | 10 | 12 => 8,
            6 | 8 => 2,
            9 => 4,
            _ => 1,
        };
        let total_bytes = count * type_size;
        let data_off = if total_bytes <= 4 {
            val_field
        } else {
            read_u32(buf, val_field, le) as usize
        };

        match tag {
            TAG_SUBFILE_TYPE => {
                let v = read_int(buf, data_off, entry_type, le);
                if (v & 1) != 0 {
                    ifd.is_thumbnail = true;
                }
            }
            TAG_IMAGE_WIDTH => ifd.width = read_int(buf, data_off, entry_type, le) as u32,
            TAG_IMAGE_LENGTH => ifd.height = read_int(buf, data_off, entry_type, le) as u32,
            TAG_BITS_PER_SAMPLE => {
                // May be one value (SHORT) or array. Use first element.
                ifd.bits_per_sample = read_int(buf, data_off, entry_type, le) as u32;
            }
            TAG_COMPRESSION => ifd.compression = read_int(buf, data_off, entry_type, le) as u32,
            TAG_PHOTOMETRIC => ifd.photometric = read_int(buf, data_off, entry_type, le) as u32,
            TAG_EXTRA_SAMPLES => ifd.extra_samples = read_int(buf, data_off, entry_type, le) as u32,
            TAG_SAMPLES_PER_PIXEL => ifd.samples_per_pixel = read_int(buf, data_off, entry_type, le) as u32,
            TAG_ROWS_PER_STRIP => ifd.rows_per_strip = read_int(buf, data_off, entry_type, le) as u32,
            TAG_PLANAR_CONFIG => ifd.planar_config = read_int(buf, data_off, entry_type, le) as u32,
            TAG_SAMPLE_FORMAT => ifd.sample_format = read_int(buf, data_off, entry_type, le) as u32,
            TAG_ORIENTATION => ifd.orientation = read_int(buf, data_off, entry_type, le) as u32,
            TAG_MAKE => {
                if entry_type == 2 {
                    let end = (data_off + count).min(buf.len());
                    if data_off < buf.len() {
                        let raw = &buf[data_off..end];
                        let s = String::from_utf8_lossy(raw)
                            .trim_end_matches('\0')
                            .to_string();
                        if !s.is_empty() { ifd.make = Some(s); }
                    }
                }
            }
            TAG_MODEL => {
                if entry_type == 2 {
                    let end = (data_off + count).min(buf.len());
                    if data_off < buf.len() {
                        let raw = &buf[data_off..end];
                        let s = String::from_utf8_lossy(raw)
                            .trim_end_matches('\0')
                            .to_string();
                        if !s.is_empty() { ifd.model = Some(s); }
                    }
                }
            }
            TAG_SOFTWARE => {
                if entry_type == 2 {
                    let end = (data_off + count).min(buf.len());
                    if data_off < buf.len() {
                        let raw = &buf[data_off..end];
                        let s = String::from_utf8_lossy(raw)
                            .trim_end_matches('\0')
                            .to_string();
                        ifd.software = Some(s);
                    }
                }
            }
            TAG_ARTIST => {
                if entry_type == 2 {
                    let end = (data_off + count).min(buf.len());
                    if data_off < buf.len() {
                        let raw = &buf[data_off..end];
                        let s = String::from_utf8_lossy(raw)
                            .trim_end_matches('\0')
                            .to_string();
                        if !s.is_empty() { ifd.artist = Some(s); }
                    }
                }
            }
            TAG_COPYRIGHT => {
                if entry_type == 2 {
                    let end = (data_off + count).min(buf.len());
                    if data_off < buf.len() {
                        let raw = &buf[data_off..end];
                        let s = String::from_utf8_lossy(raw)
                            .trim_end_matches('\0')
                            .to_string();
                        if !s.is_empty() { ifd.copyright = Some(s); }
                    }
                }
            }
            TAG_DATE_TIME => {
                if entry_type == 2 {
                    let end = (data_off + count).min(buf.len());
                    if data_off < buf.len() {
                        let raw = &buf[data_off..end];
                        let s = String::from_utf8_lossy(raw)
                            .trim_end_matches('\0')
                            .to_string();
                        if !s.is_empty() { ifd.date_time = Some(s); }
                    }
                }
            }
            TAG_IMAGE_DESCRIPTION => {
                if entry_type == 2 {
                    let end = (data_off + count).min(buf.len());
                    if data_off < buf.len() {
                        let raw = &buf[data_off..end];
                        let s = String::from_utf8_lossy(raw)
                            .trim_end_matches('\0')
                            .to_string();
                        if !s.is_empty() { ifd.image_description = Some(s); }
                    }
                }
            }
            TAG_X_RESOLUTION | TAG_Y_RESOLUTION => {
                if entry_type == 5 && data_off + 8 <= buf.len() {
                    let num = read_u32(buf, data_off, le);
                    let den = read_u32(buf, data_off + 4, le);
                    if tag == TAG_X_RESOLUTION {
                        ifd.x_resolution = Some((num, den));
                    } else {
                        ifd.y_resolution = Some((num, den));
                    }
                }
            }
            TAG_RESOLUTION_UNIT => ifd.resolution_unit = read_int(buf, data_off, entry_type, le) as u32,
            _ => {}
        }
    }
    Some(ifd)
}

fn read_int(buf: &[u8], off: usize, t: u16, le: bool) -> u64 {
    match t {
        1 | 2 | 7 => *buf.get(off).unwrap_or(&0) as u64,
        3 | 8 => read_u16(buf, off, le) as u64,
        4 | 9 | 11 => read_u32(buf, off, le) as u64,
        _ => read_u32(buf, off, le) as u64,
    }
}

fn read_u16(buf: &[u8], off: usize, le: bool) -> u16 {
    if off + 2 > buf.len() {
        return 0;
    }
    if le {
        u16::from_le_bytes([buf[off], buf[off + 1]])
    } else {
        u16::from_be_bytes([buf[off], buf[off + 1]])
    }
}

fn read_u32(buf: &[u8], off: usize, le: bool) -> u32 {
    if off + 4 > buf.len() {
        return 0;
    }
    if le {
        u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
    } else {
        u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
    }
}

fn tiff_compression_name(c: u32) -> &'static str {
    match c {
        1 => "Raw",
        2 => "CCITT T.4/Group 3 1D Fax",
        3 => "CCITT T.4/Group 3 Fax",
        4 => "CCITT T.6/Group 4 Fax",
        5 => "LZW",
        6 | 7 | 99 => "JPEG",
        8 => "Adobe Deflate",
        32773 => "PackBits",
        32946 => "Deflate",
        34712 => "JPEG 2000",
        34925 => "LZMA2",
        34926 | 50000 => "Zstd",
        34927 | 50001 => "WebP",
        34933 => "PNG",
        34934 => "JPEG XR",
        50002 | 52546 => "JPEG XL",
        _ => "",
    }
}

fn tiff_compression_mode(c: u32) -> &'static str {
    match c {
        1 | 2 | 3 | 5 | 8 | 32773 => "Lossless",
        _ => "",
    }
}

fn photometric_color_space(p: u32) -> &'static str {
    match p {
        0 | 1 => "Y",
        2 | 3 => "RGB",
        4 => "A",
        5 => "CMYK",
        6 => "YUV",
        8 => "CIELAB",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_le_tiff_minimal() -> Vec<u8> {
        // Header (8 bytes): II 2A 00 + offset to IFD (8)
        // IFD at offset 8: entry count(2) + entries(12 each) + next(4)
        // Entries: ImageWidth=256(SHORT,1,100), ImageLength=257(SHORT,1,50),
        //   BitsPerSample=258(SHORT,1,8), Compression=259(SHORT,1,1),
        //   PhotometricInterpretation=262(SHORT,1,2 RGB)
        let mut buf = Vec::new();
        buf.extend_from_slice(&[b'I', b'I', 0x2A, 0x00]);
        buf.extend_from_slice(&8u32.to_le_bytes());
        // IFD
        let entries: [(u16, u16, u32, u32); 5] = [
            (256, 3, 1, 100), // ImageWidth
            (257, 3, 1, 50),  // ImageLength
            (258, 3, 1, 8),   // BitsPerSample
            (259, 3, 1, 1),   // Compression (raw)
            (262, 3, 1, 2),   // Photometric (RGB)
        ];
        buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, ty, count, val) in entries {
            buf.extend_from_slice(&tag.to_le_bytes());
            buf.extend_from_slice(&ty.to_le_bytes());
            buf.extend_from_slice(&count.to_le_bytes());
            buf.extend_from_slice(&val.to_le_bytes());
        }
        buf.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
        buf
    }

    #[test]
    fn rejects_non_tiff() {
        let mut fa = FileAnalyze::new(b"NOPE TIFF MAGIC HERE");
        assert!(!parse_tiff(&mut fa));
    }

    #[test]
    fn parses_minimal_le_tiff() {
        let buf = build_le_tiff_minimal();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_tiff(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Width").as_deref(), Some("100"));
        assert_eq!(i("Height").as_deref(), Some("50"));
        assert_eq!(i("BitDepth").as_deref(), Some("8"));
        assert_eq!(i("Format").as_deref(), Some("Raw"));
        assert_eq!(i("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(i("ColorSpace").as_deref(), Some("RGB"));
        assert_eq!(i("Format_Settings_Endianness").as_deref(), Some("Little"));
    }

    #[test]
    fn parses_big_endian_tiff() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[b'M', b'M', 0x00, 0x2A]);
        buf.extend_from_slice(&8u32.to_be_bytes());
        let entries: [(u16, u16, u32, u32); 4] = [
            (256, 3, 1, 0), // width tag, type SHORT, count 1, value 0 (placeholder; actual)
            (257, 3, 1, 0),
            (258, 3, 1, 0),
            (259, 3, 1, 0),
        ];
        // For SHORT (type 3, size 2) with count 1 → total 2 bytes ≤ 4,
        // so value lives in first 2 bytes of the 4-byte value field. In
        // big-endian, that means the value occupies the high 2 bytes.
        buf.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        // Manually emit entries with the value packed at the start of the
        // value field, matching BE SHORT layout.
        let values: [u16; 4] = [200, 150, 16, 5]; // width, height, bits, compression=LZW
        for (i, (tag, ty, count, _)) in entries.iter().enumerate() {
            buf.extend_from_slice(&tag.to_be_bytes());
            buf.extend_from_slice(&ty.to_be_bytes());
            buf.extend_from_slice(&count.to_be_bytes());
            buf.extend_from_slice(&values[i].to_be_bytes());
            buf.extend_from_slice(&[0u8, 0]); // padding to 4-byte value field
        }
        buf.extend_from_slice(&0u32.to_be_bytes());
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_tiff(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Width").as_deref(), Some("200"));
        assert_eq!(i("Height").as_deref(), Some("150"));
        assert_eq!(i("BitDepth").as_deref(), Some("16"));
        assert_eq!(i("Format").as_deref(), Some("LZW"));
        assert_eq!(i("Format_Settings_Endianness").as_deref(), Some("Big"));
    }
}
