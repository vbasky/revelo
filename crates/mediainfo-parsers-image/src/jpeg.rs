//! JPEG parser — segment-based image format. Mirrors the subset of
//! MediaInfoLib's `File_Jpeg.cpp` for plain JPEG/JFIF files.
//!
//! Layout: 0xFFD8 (SOI) + segments + 0xFFD9 (EOI).
//! Each non-fixed-length segment is `FF [marker:u8] [length:u16 BE]
//! [length-2 bytes payload]`.
//!
//! SOFn markers (Start Of Frame) carry the image geometry:
//!   FFC0 baseline DCT, FFC1 extended sequential, FFC2 progressive,
//!   FFC3 lossless, FFC5..FFC7 differential, FFC9..FFCB arithmetic,
//!   FFCD..FFCF differential arithmetic.
//! SOF payload: precision(1) + height(2 BE) + width(2 BE) + components(1)
//!   + per-component (id, h_v_sampling, qt_index) * components.

use mediainfo_core::{FileAnalyze, StreamKind};

#[derive(Default)]
struct ExifData {
    make: Option<String>,
    model: Option<String>,
    /// IFD0 ImageDescription (tag 0x010E).
    description: Option<String>,
    /// IFD0 DateTime (tag 0x0132).
    datetime: Option<String>,
    /// Exif sub-IFD DateTimeOriginal (tag 0x9003).
    datetime_original: Option<String>,
    /// IFD1 thumbnail block (JPEG-compressed; offsets relative to TIFF base).
    thumbnail_width: Option<u32>,
    thumbnail_height: Option<u32>,
    thumbnail_size: Option<u32>,
    /// JPEGInterchangeFormat (0x0201) — offset of the embedded thumbnail
    /// JPEG within the TIFF buffer. Used to read the thumbnail SOF when
    /// IFD1 doesn't carry explicit ImageWidth/Length tags.
    thumbnail_offset: Option<u32>,
    // Exif sub-IFD "extra" tags — bucketed into the `<extra>` block.
    /// ExposureTime (0x829A) as (numerator, denominator). Oracle emits
    /// `ShutterSpeed_Time` (decimal seconds) + `ShutterSpeed_Time_String` ("1/N s").
    exposure_time: Option<(u32, u32)>,
    /// FNumber (0x829D) as (numerator, denominator).
    f_number: Option<(u32, u32)>,
    /// ExposureProgram (0x8822) — small int with predefined meanings.
    exposure_program: Option<u16>,
    /// ISOSpeedRatings (0x8827).
    iso_speed: Option<u16>,
    /// ExifVersion (0x9000) — 4 ASCII bytes, e.g. "0220" → "2.20".
    exif_version: Option<[u8; 4]>,
    /// Flash (0x9209) — short with bit fields.
    flash: Option<u16>,
    /// FocalLength (0x920A) as (numerator, denominator).
    focal_length: Option<(u32, u32)>,
    /// FlashpixVersion (0xA000) — 4 ASCII bytes.
    flashpix_version: Option<[u8; 4]>,
    /// WhiteBalance (0xA403): 0=Auto, 1=Manual.
    white_balance: Option<u16>,
    /// FocalLengthIn35mmFilm (0xA405).
    focal_length_35mm: Option<u16>,
    /// LensModel (0xA434) - ASCII string.
    lens_model: Option<String>,
    /// ExposureBiasValue (0x9204) as (numerator, denominator).
    exposure_bias: Option<(i32, i32)>,
    /// MeteringMode (0x9207): 0=Unknown, 1=Average, 2=CenterWeighted, 3=Spot, 4=MultiSpot, 5=Pattern, 6=Partial.
    metering_mode: Option<u16>,
    /// LightSource (0x9208): 0=Unknown, 1=Daylight, 2=Fluorescent, 3=Tungsten, 4=Flash, etc.
    light_source: Option<u16>,
    /// SceneType (0xA301): 1=directly photographed.
    scene_type: Option<u8>,
    /// CustomRendered (0xA401): 0=Normal, 1=Custom.
    custom_rendered: Option<u16>,
    /// ExposureMode (0xA402): 0=Auto, 1=Manual, 2=Auto bracket.
    exposure_mode: Option<u16>,
    /// DigitalZoomRatio (0xA404) as (numerator, denominator).
    digital_zoom_ratio: Option<(u32, u32)>,
    /// SceneCaptureType (0xA406): 0=Standard, 1=Landscape, 2=Portrait, 3=Night scene.
    scene_capture_type: Option<u16>,
    /// Contrast (0xA408): 0=Normal, 1=Soft, 2=Hard.
    contrast: Option<u16>,
    /// Saturation (0xA409): 0=Normal, 1=Low, 2=High.
    saturation: Option<u16>,
    /// Sharpness (0xA40A): 0=Normal, 1=Soft, 2=Hard.
    sharpness: Option<u16>,
    // GPS IFD fields (tag 0x8825 points to GPS IFD)
    /// GPSLatitudeRef (0x0001): "N" or "S"
    gps_latitude_ref: Option<String>,
    /// GPSLatitude (0x0002): 3 rationals (degrees, minutes, seconds)
    gps_latitude: Option<[(u32, u32); 3]>,
    /// GPSLongitudeRef (0x0003): "E" or "W"
    gps_longitude_ref: Option<String>,
    /// GPSLongitude (0x0004): 3 rationals (degrees, minutes, seconds)
    gps_longitude: Option<[(u32, u32); 3]>,
    /// GPSAltitudeRef (0x0005): 0=above sea level, 1=below
    gps_altitude_ref: Option<u8>,
    /// GPSAltitude (0x0006): rational
    gps_altitude: Option<(u32, u32)>,
    /// GPSTimeStamp (0x0007): 3 rationals (hour, minute, second)
    gps_timestamp: Option<[(u32, u32); 3]>,
    /// GPSDateStamp (0x001D): ASCII string "YYYY:MM:DD"
    gps_datestamp: Option<String>,
}

pub fn parse_jpeg(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(2);
    let Some(h) = head else { return false };
    if h != [0xFF, 0xD8] {
        return false;
    }

    let file_size = fa.Remain();
    fa.Skip_Hexa(2, "SOI");

    let mut width: u16 = 0;
    let mut height: u16 = 0;
    let mut precision: u8 = 0;
    let mut components: u8 = 0;
    let mut sampling: Vec<(u8, u8)> = Vec::new();
    let mut found_sof = false;
    let mut comment: Option<String> = None;
    let mut exif = ExifData::default();
    // Overhead = APP markers (0xE0..0xEF) + COM markers (0xFE).
    // Oracle treats SOI/EOI/DQT/DHT/SOF/SOS/entropy as image data.
    let mut overhead: usize = 0;

    while fa.Remain() >= 4 {
        let marker_bytes = fa.read_raw(2).to_vec();
        if marker_bytes.len() < 2 || marker_bytes[0] != 0xFF {
            // Past markers — into entropy-coded data.
            break;
        }
        let marker = marker_bytes[1];
        if marker == 0xD9 {
            // EOI
            break;
        }
        if marker == 0xDA {
            // SOS — entropy-coded data follows. Stop marker scanning.
            break;
        }
        // Markers without payload length: D0..D7 (RSTn), 01 (TEM)
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        // Read 2-byte BE length (includes the 2-byte length field itself).
        let len_bytes = fa.peek_raw(2);
        let Some(lb) = len_bytes else { break };
        let segment_len = u16::from_be_bytes([lb[0], lb[1]]) as usize;
        if segment_len < 2 || fa.Remain() < segment_len {
            break;
        }
        // APP markers + COM marker count toward General.StreamSize.
        if (0xE0..=0xEF).contains(&marker) || marker == 0xFE {
            overhead += 2 + segment_len; // marker(2) + length+payload
        }
        fa.Skip_Hexa(2, "segment_length");
        let payload_size = segment_len - 2;

        // SOFn markers (geometry). Excludes FFC4 (DHT), FFC8 (JPG), FFCC (DAC).
        let is_sof = matches!(marker,
            0xC0 | 0xC1 | 0xC2 | 0xC3
            | 0xC5 | 0xC6 | 0xC7
            | 0xC9 | 0xCA | 0xCB
            | 0xCD | 0xCE | 0xCF);
        if is_sof && !found_sof && payload_size >= 6 {
            let payload = fa.read_raw(payload_size).to_vec();
            precision = payload[0];
            height = u16::from_be_bytes([payload[1], payload[2]]);
            width = u16::from_be_bytes([payload[3], payload[4]]);
            components = payload[5];
            // Each component is 3 bytes: id, h_v_sampling, qt_idx.
            for c in 0..(components as usize) {
                let off = 6 + c * 3;
                if off + 1 < payload.len() {
                    let hv = payload[off + 1];
                    sampling.push(((hv >> 4) & 0xF, hv & 0xF));
                }
            }
            found_sof = true;
        } else if marker == 0xFE {
            // COM (Comment) segment — payload is UTF-8 text.
            let payload = fa.read_raw(payload_size).to_vec();
            comment = Some(
                String::from_utf8_lossy(&payload)
                    .trim_end_matches('\0')
                    .to_string(),
            );
        } else if marker == 0xE1 && payload_size >= 14 {
            // APP1: may be Exif ("Exif\0\0") or XMP (longer URI). Detect
            // by leading 6 bytes; only walk TIFF for Exif.
            let payload = fa.read_raw(payload_size).to_vec();
            if payload.len() >= 14 && &payload[..6] == b"Exif\0\0" {
                parse_exif_tiff(&payload[6..], &mut exif);
            } else if payload.len() >= 29 && &payload[..29] == b"http://ns.adobe.com/xap/1.0/\0" {
                // XMP packet follows the null-terminated URI
                if payload.len() > 29 {
                    let xmp_data = &payload[29..];
                    parse_xmp(xmp_data, &mut exif);
                }
            }
        } else {
            fa.Skip_Hexa(payload_size, "segment");
        }
    }

    if !found_sof {
        return false;
    }

    fill_streams(
        fa,
        file_size,
        overhead,
        comment,
        &exif,
        width,
        height,
        precision,
        components,
        &sampling,
    );
    true
}

/// Parse XMP (Extensible Metadata Platform) packet embedded in APP1.
/// Extracts common Dublin Core and XMP Basic properties.
fn parse_xmp(xmp_data: &[u8], out: &mut ExifData) {
    // XMP is XML/RDF - parse key fields with simple string search
    let xmp_str = String::from_utf8_lossy(xmp_data);
    
    // Extract dc:title
    if let Some(start) = xmp_str.find("<dc:title>") {
        if let Some(end) = xmp_str[start..].find("</dc:title>") {
            let title_section = &xmp_str[start..start+end+11];
            if let Some(rdf_start) = title_section.find("<rdf:Alt>") {
                if let Some(li_start) = title_section[rdf_start..].find("<rdf:li") {
                    let after_li = &title_section[rdf_start+li_start..];
                    if let Some(gt_pos) = after_li.find('>') {
                        if let Some(li_end) = after_li[gt_pos..].find("</rdf:li>") {
                            let title = &after_li[gt_pos+1..gt_pos+li_end];
                            if out.description.is_none() {
                                out.description = Some(title.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Extract dc:creator (artist)
    if let Some(start) = xmp_str.find("<dc:creator>") {
        if let Some(end) = xmp_str[start..].find("</dc:creator>") {
            let creator_section = &xmp_str[start..start+end+13];
            if let Some(seq_start) = creator_section.find("<rdf:Seq>") {
                if let Some(li_start) = creator_section[seq_start..].find("<rdf:li>") {
                    let after_li = &creator_section[seq_start+li_start+8..];
                    if let Some(li_end) = after_li.find("</rdf:li>") {
                        let creator = &after_li[..li_end];
                        // Store in make for now (can be moved to a dedicated field later)
                        if out.make.is_none() {
                            out.make = Some(creator.to_string());
                        }
                    }
                }
            }
        }
    }
    
    // Extract xmp:CreateDate (similar to datetime_original)
    if let Some(start) = xmp_str.find("xmp:CreateDate=\"") {
        let after_eq = &xmp_str[start+16..];
        if let Some(quote_end) = after_eq.find('\"') {
            let date_str = &after_eq[..quote_end];
            if out.datetime_original.is_none() {
                out.datetime_original = Some(date_str.to_string());
            }
        }
    }
    
    // Extract xmp:ModifyDate
    if let Some(start) = xmp_str.find("xmp:ModifyDate=\"") {
        let after_eq = &xmp_str[start+16..];
        if let Some(quote_end) = after_eq.find('\"') {
            let date_str = &after_eq[..quote_end];
            if out.datetime.is_none() {
                out.datetime = Some(date_str.to_string());
            }
        }
    }
}

/// Parse an EXIF TIFF block.
///
/// TIFF header (8 bytes):
///   2 bytes byte_order ("II" little, "MM" big)
///   2 bytes magic = 42
///   4 bytes offset to IFD0
///
/// Each IFD: 2-byte entry_count, N × 12-byte entries, 4-byte next-IFD offset.
/// Entry: 2-byte tag + 2-byte type + 4-byte count + 4-byte value/offset.
fn parse_exif_tiff(tiff: &[u8], out: &mut ExifData) {
    if tiff.len() < 8 {
        return;
    }
    let little = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return,
    };
    let r16 = |b: &[u8]| -> u16 {
        if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) }
    };
    let r32 = |b: &[u8]| -> u32 {
        if little {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        }
    };
    if r16(&tiff[2..4]) != 42 {
        return;
    }
    let ifd0_off = r32(&tiff[4..8]) as usize;
    let exif_ifd_off = walk_ifd0(tiff, ifd0_off, little, &r16, &r32, out);
    if let Some(off) = exif_ifd_off {
        walk_exif_ifd(tiff, off, little, &r16, &r32, out);
    }
    // IFD1 (thumbnail) starts at the next-IFD offset stored after IFD0.
    if let Some(ifd1_off) = next_ifd_offset(tiff, ifd0_off, little, &r16, &r32) {
        if ifd1_off != 0 {
            walk_ifd1(tiff, ifd1_off as usize, little, &r16, &r32, out);
        }
    }
}

fn next_ifd_offset(
    tiff: &[u8],
    ifd_off: usize,
    _little: bool,
    r16: &impl Fn(&[u8]) -> u16,
    r32: &impl Fn(&[u8]) -> u32,
) -> Option<u32> {
    if ifd_off + 2 > tiff.len() {
        return None;
    }
    let count = r16(&tiff[ifd_off..ifd_off + 2]) as usize;
    let next_pos = ifd_off + 2 + count * 12;
    if next_pos + 4 > tiff.len() {
        return None;
    }
    Some(r32(&tiff[next_pos..next_pos + 4]))
}

/// Read an ASCII string entry from a TIFF tag. Tag values of count ≤ 4 are
/// stored inline in the value field; larger values are offset-referenced.
fn read_ascii_entry(tiff: &[u8], value_field: &[u8], count: u32, r32: &impl Fn(&[u8]) -> u32) -> Option<String> {
    let n = count as usize;
    let bytes: &[u8] = if n <= 4 {
        &value_field[..n.min(4)]
    } else {
        let off = r32(value_field) as usize;
        if off + n > tiff.len() {
            return None;
        }
        &tiff[off..off + n]
    };
    Some(
        String::from_utf8_lossy(bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string(),
    )
}

fn walk_ifd0(
    tiff: &[u8],
    ifd_off: usize,
    little: bool,
    r16: &impl Fn(&[u8]) -> u16,
    r32: &impl Fn(&[u8]) -> u32,
    out: &mut ExifData,
) -> Option<usize> {
    if ifd_off + 2 > tiff.len() {
        return None;
    }
    let count = r16(&tiff[ifd_off..ifd_off + 2]) as usize;
    let mut exif_ifd_pointer: Option<usize> = None;
    for i in 0..count {
        let entry = ifd_off + 2 + i * 12;
        if entry + 12 > tiff.len() {
            break;
        }
        let tag = r16(&tiff[entry..entry + 2]);
        let typ = r16(&tiff[entry + 2..entry + 4]);
        let cnt = r32(&tiff[entry + 4..entry + 8]);
        let val = &tiff[entry + 8..entry + 12];
        match tag {
            0x010E => out.description = read_ascii_entry(tiff, val, cnt, r32),
            0x010F => out.make = read_ascii_entry(tiff, val, cnt, r32),
            0x0110 => out.model = read_ascii_entry(tiff, val, cnt, r32),
            0x0132 => out.datetime = read_ascii_entry(tiff, val, cnt, r32),
            0x8769 if typ == 4 => exif_ifd_pointer = Some(r32(val) as usize),
            _ => {}
        }
    }
    let _ = little;
    exif_ifd_pointer
}

fn walk_exif_ifd(
    tiff: &[u8],
    ifd_off: usize,
    _little: bool,
    r16: &impl Fn(&[u8]) -> u16,
    r32: &impl Fn(&[u8]) -> u32,
    out: &mut ExifData,
) {
    if ifd_off + 2 > tiff.len() {
        return;
    }
    let count = r16(&tiff[ifd_off..ifd_off + 2]) as usize;
    for i in 0..count {
        let entry = ifd_off + 2 + i * 12;
        if entry + 12 > tiff.len() {
            break;
        }
        let tag = r16(&tiff[entry..entry + 2]);
        let typ = r16(&tiff[entry + 2..entry + 4]);
        let cnt = r32(&tiff[entry + 4..entry + 8]);
        let val = &tiff[entry + 8..entry + 12];
        match tag {
            0x9003 => out.datetime_original = read_ascii_entry(tiff, val, cnt, r32),
            // RATIONAL (type 5) tags — 8-byte payload always pointed to
            // by the value field's offset.
            0x829A if typ == 5 && cnt == 1 => out.exposure_time = read_rational(tiff, val, r32),
            0x829D if typ == 5 && cnt == 1 => out.f_number = read_rational(tiff, val, r32),
            0x920A if typ == 5 && cnt == 1 => out.focal_length = read_rational(tiff, val, r32),
            // SHORT (type 3) tags — value sits inline in low 2 bytes of val.
            0x8822 if typ == 3 && cnt == 1 => out.exposure_program = Some(r16(&val[..2])),
            0x8827 if typ == 3 && cnt == 1 => out.iso_speed = Some(r16(&val[..2])),
            0x9209 if typ == 3 && cnt == 1 => out.flash = Some(r16(&val[..2])),
            0xA403 if typ == 3 && cnt == 1 => out.white_balance = Some(r16(&val[..2])),
            0xA405 if typ == 3 && cnt == 1 => out.focal_length_35mm = Some(r16(&val[..2])),
            // UNDEFINED (type 7) — 4-byte ASCII version strings stored inline.
            0x9000 if cnt == 4 => out.exif_version = Some([val[0], val[1], val[2], val[3]]),
            0xA000 if cnt == 4 => out.flashpix_version = Some([val[0], val[1], val[2], val[3]]),
            // Lens and exposure info
            0xA434 => out.lens_model = read_ascii_entry(tiff, val, cnt, r32),
            0x9204 if typ == 5 && cnt == 1 => out.exposure_bias = read_signed_rational(tiff, val, r32),
            0x9207 if typ == 3 && cnt == 1 => out.metering_mode = Some(r16(&val[..2])),
            0x9208 if typ == 3 && cnt == 1 => out.light_source = Some(r16(&val[..2])),
            0xA301 if typ == 7 && cnt == 1 => out.scene_type = Some(val[0]),
            0xA401 if typ == 3 && cnt == 1 => out.custom_rendered = Some(r16(&val[..2])),
            0xA402 if typ == 3 && cnt == 1 => out.exposure_mode = Some(r16(&val[..2])),
            0xA404 if typ == 5 && cnt == 1 => out.digital_zoom_ratio = read_rational(tiff, val, r32),
            0xA406 if typ == 3 && cnt == 1 => out.scene_capture_type = Some(r16(&val[..2])),
            0xA408 if typ == 3 && cnt == 1 => out.contrast = Some(r16(&val[..2])),
            0xA409 if typ == 3 && cnt == 1 => out.saturation = Some(r16(&val[..2])),
            0xA40A if typ == 3 && cnt == 1 => out.sharpness = Some(r16(&val[..2])),
            _ => {}
        }
    }
}

/// Read a signed 8-byte RATIONAL (numerator + denominator, each i32) from the
/// TIFF buffer. The 4-byte `val` field is the offset to the 8 bytes.
fn read_signed_rational(
    tiff: &[u8],
    val: &[u8],
    r32: &impl Fn(&[u8]) -> u32,
) -> Option<(i32, i32)> {
    let off = r32(val) as usize;
    if off + 8 > tiff.len() {
        return None;
    }
    let num = r32(&tiff[off..off + 4]) as i32;
    let den = r32(&tiff[off + 4..off + 8]) as i32;
    Some((num, den))
}

/// Read an 8-byte RATIONAL (numerator + denominator, each u32) from the
/// TIFF buffer. The 4-byte `val` field is the offset to the 8 bytes.
fn read_rational(
    tiff: &[u8],
    val: &[u8],
    r32: &impl Fn(&[u8]) -> u32,
) -> Option<(u32, u32)> {
    let off = r32(val) as usize;
    if off + 8 > tiff.len() {
        return None;
    }
    let num = r32(&tiff[off..off + 4]);
    let den = r32(&tiff[off + 4..off + 8]);
    Some((num, den))
}

fn walk_ifd1(
    tiff: &[u8],
    ifd_off: usize,
    _little: bool,
    r16: &impl Fn(&[u8]) -> u16,
    r32: &impl Fn(&[u8]) -> u32,
    out: &mut ExifData,
) {
    if ifd_off + 2 > tiff.len() {
        return;
    }
    let count = r16(&tiff[ifd_off..ifd_off + 2]) as usize;
    for i in 0..count {
        let entry = ifd_off + 2 + i * 12;
        if entry + 12 > tiff.len() {
            break;
        }
        let tag = r16(&tiff[entry..entry + 2]);
        let cnt = r32(&tiff[entry + 4..entry + 8]);
        let val = &tiff[entry + 8..entry + 12];
        // For SHORT (type 3) tags the value sits in the low 2 bytes of
        // `val` (count == 1); for LONG (type 4) it's the full 4 bytes.
        match tag {
            // ImageWidth / ImageLength when present in IFD1 describe the
            // thumbnail geometry.
            0x0100 if cnt == 1 => out.thumbnail_width = Some(r32(val)),
            0x0101 if cnt == 1 => out.thumbnail_height = Some(r32(val)),
            // JPEGInterchangeFormat = thumbnail data offset.
            0x0201 if cnt == 1 => out.thumbnail_offset = Some(r32(val)),
            // JPEGInterchangeFormatLength = thumbnail byte size.
            0x0202 if cnt == 1 => out.thumbnail_size = Some(r32(val)),
            _ => {}
        }
    }
    // If IFD1 only carries the JPEG offset/length but no ImageWidth/Length,
    // parse the embedded JPEG's SOF for geometry. Common in compact-camera
    // thumbnails (e.g. Acer C01).
    if out.thumbnail_width.is_none() {
        if let (Some(off), Some(sz)) = (out.thumbnail_offset, out.thumbnail_size) {
            let lo = off as usize;
            let hi = (off + sz) as usize;
            if hi <= tiff.len() {
                if let Some((w, h)) = scan_jpeg_sof(&tiff[lo..hi]) {
                    out.thumbnail_width = Some(w as u32);
                    out.thumbnail_height = Some(h as u32);
                }
            }
        }
    }
}

/// Scan a JPEG byte stream for the first SOF marker and return its
/// (width, height). Returns None if no SOF found.
fn scan_jpeg_sof(buf: &[u8]) -> Option<(u16, u16)> {
    if buf.len() < 4 || buf[0] != 0xFF || buf[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 4 <= buf.len() {
        if buf[i] != 0xFF {
            return None;
        }
        let m = buf[i + 1];
        if m == 0xD9 || m == 0xDA {
            return None;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        if i + 4 > buf.len() {
            return None;
        }
        let seg = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        let is_sof = matches!(
            m,
            0xC0 | 0xC1 | 0xC2 | 0xC3 | 0xC5 | 0xC6 | 0xC7 |
            0xC9 | 0xCA | 0xCB | 0xCD | 0xCE | 0xCF
        );
        if is_sof && i + 4 + 5 <= buf.len() {
            // SOF payload = precision(1) + height(2) + width(2) + ...
            let h = u16::from_be_bytes([buf[i + 5], buf[i + 6]]);
            let w = u16::from_be_bytes([buf[i + 7], buf[i + 8]]);
            return Some((w, h));
        }
        i += 2 + seg;
    }
    None
}

/// "2018:12:10 15:44:06" (EXIF) → "2018-12-10 15:44:06" (oracle).
fn exif_datetime_to_oracle(s: &str) -> String {
    let mut out: Vec<u8> = s.bytes().collect();
    if out.len() >= 10 && out[4] == b':' && out[7] == b':' {
        out[4] = b'-';
        out[7] = b'-';
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn fill_streams(
    fa: &mut FileAnalyze,
    file_size: usize,
    overhead: usize,
    comment: Option<String>,
    exif: &ExifData,
    width: u16,
    height: u16,
    precision: u8,
    components: u8,
    sampling: &[(u8, u8)],
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "JPEG", false);
    let has_thumbnail = exif.thumbnail_width.is_some() && exif.thumbnail_height.is_some();
    let image_count = if has_thumbnail { 2 } else { 1 };
    fa.Fill(StreamKind::General, 0, "ImageCount", image_count.to_string(), false);
    // General.StreamSize = file overhead. When a thumbnail is embedded
    // its bytes are part of the EXIF APP1 segment but oracle deducts
    // them from General (and attributes them to the thumbnail Image
    // stream instead).
    let thumb_bytes = exif.thumbnail_size.unwrap_or(0) as usize;
    let general_overhead = overhead.saturating_sub(thumb_bytes);
    fa.Fill(StreamKind::General, 0, "StreamSize", general_overhead.to_string(), true);
    if let Some(d) = exif.description.as_deref() {
        if !d.is_empty() {
            fa.Fill(StreamKind::General, 0, "Description", d.to_string(), false);
        }
    }
    if let Some(dt) = exif.datetime_original.as_deref() {
        fa.Fill(StreamKind::General, 0, "Recorded_Date", exif_datetime_to_oracle(dt), false);
    }
    if let Some(dt) = exif.datetime.as_deref() {
        fa.Fill(StreamKind::General, 0, "Mastered_Date", exif_datetime_to_oracle(dt), false);
    }
    if let Some(m) = exif.make.as_deref() {
        if !m.is_empty() {
            // Oracle normalizes common manufacturer-name suffixes (" Inc.",
            // " Corporation", " CO.,LTD") — strip the most common one so
            // "Acer Inc." matches the oracle's "Acer".
            let normalized = m
                .trim_end_matches(" Inc.")
                .trim_end_matches(" Corporation")
                .trim_end_matches(" CORPORATION")
                .trim()
                .to_string();
            fa.Fill(StreamKind::General, 0, "Encoded_Hardware_CompanyName", normalized, false);
        }
    }
    if let Some(m) = exif.model.as_deref() {
        if !m.is_empty() {
            fa.Fill(StreamKind::General, 0, "Encoded_Hardware_Model", m.to_string(), false);
        }
    }
    // Exif sub-IFD extras (oracle wraps these in <extra>...</extra>).
    // Emit in oracle's display order so the diff matches sequentially.
    if let Some((n, d)) = exif.exposure_time {
        if d > 0 {
            let secs = n as f64 / d as f64;
            fa.Fill_Extra(StreamKind::General, 0, "ShutterSpeed_Time", format!("{secs:.6}"), false);
            // String form: "1/N s" if numerator is 1 and denominator > 0.
            if n == 1 {
                fa.Fill_Extra(StreamKind::General, 0, "ShutterSpeed_Time_String", format!("1/{d} s"), false);
            }
        }
    }
    if let Some((n, d)) = exif.f_number {
        if d > 0 {
            let v = n as f64 / d as f64;
            fa.Fill_Extra(StreamKind::General, 0, "IrisFNumber", format!("{v:.1}"), false);
        }
    }
    if let Some(prog) = exif.exposure_program {
        let s = match prog {
            0 => "Not Defined",
            1 => "Manual",
            2 => "Normal",
            3 => "Aperture priority",
            4 => "Shutter priority",
            5 => "Creative",
            6 => "Action",
            7 => "Portrait",
            8 => "Landscape",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "AutoExposureMode", s, false);
        }
    }
    if let Some(iso) = exif.iso_speed {
        fa.Fill_Extra(StreamKind::General, 0, "ISOSensitivity", iso.to_string(), false);
    }
    if let Some(v) = exif.exif_version {
        // "0220" → "2.20"
        let s = String::from_utf8_lossy(&v);
        if s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()) {
            let formatted = format!("{}.{}{}", &s[..1], &s[2..3], &s[3..4]);
            // Simpler: split as "2.20" — first digit then ".XX" tail.
            let _ = formatted;
            let pretty = format!("{}.{}{}",
                s[1..2].chars().next().unwrap_or('0'),
                s[2..3].chars().next().unwrap_or('0'),
                s[3..4].chars().next().unwrap_or('0'));
            fa.Fill_Extra(StreamKind::General, 0, "ExifVersion", pretty, false);
        }
    }
    if let Some(f) = exif.flash {
        // Bits: 0=fired, 1-2=return, 3-4=mode, 5=function, 6=red-eye.
        // Oracle string: "Off, Did not fire" when flag=0x10 (mode=Off,
        // not fired), "Fired" when bit 0 set, etc. Subset for the common
        // cases.
        let s = match f {
            0x0000 => "Off, Did not fire",
            0x0001 => "Fired",
            0x0010 => "Off, Did not fire",
            0x0018 => "Auto, Did not fire",
            0x0019 => "Auto, Fired",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "Flash", s, false);
        }
    }
    if let Some((n, d)) = exif.focal_length {
        if d > 0 {
            let mm = n as f64 / d as f64;
            // Oracle drops decimals when the value is an integer mm.
            let int_mm = mm.round() as u32;
            if (mm - int_mm as f64).abs() < 0.01 {
                fa.Fill_Extra(StreamKind::General, 0, "LensZoomActualFocalLength", int_mm.to_string(), false);
                fa.Fill_Extra(StreamKind::General, 0, "LensZoomActualFocalLength_String", format!("{int_mm} mm"), false);
            } else {
                fa.Fill_Extra(StreamKind::General, 0, "LensZoomActualFocalLength", format!("{mm:.1}"), false);
                fa.Fill_Extra(StreamKind::General, 0, "LensZoomActualFocalLength_String", format!("{mm:.1} mm"), false);
            }
        }
    }
    if let Some(v) = exif.flashpix_version {
        let s = String::from_utf8_lossy(&v);
        if s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()) {
            let pretty = format!("{}.{}{}",
                s[1..2].chars().next().unwrap_or('0'),
                s[2..3].chars().next().unwrap_or('0'),
                s[3..4].chars().next().unwrap_or('0'));
            fa.Fill_Extra(StreamKind::General, 0, "FlashpixVersion", pretty, false);
        }
    }
    if let Some(wb) = exif.white_balance {
        let s = match wb {
            0 => "Auto",
            1 => "Manual",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "AutoWhiteBalanceMode", s, false);
        }
    }
    if let Some(fl35) = exif.focal_length_35mm {
        fa.Fill_Extra(StreamKind::General, 0, "LensZoom35mmStillCameraEquivalent", fl35.to_string(), false);
        fa.Fill_Extra(StreamKind::General, 0, "LensZoom35mmStillCameraEquivalent_String", format!("{fl35} mm"), false);
    }
    // New EXIF fields
    if let Some(ref lens) = exif.lens_model {
        fa.Fill_Extra(StreamKind::General, 0, "LensModel", lens.clone(), false);
    }
    if let Some((n, d)) = exif.exposure_bias {
        if d > 0 {
            let ev = n as f64 / d as f64;
            let sign = if ev >= 0.0 { "+" } else { "" };
            fa.Fill_Extra(StreamKind::General, 0, "ExposureBias", format!("{sign}{ev:.2}"), false);
            fa.Fill_Extra(StreamKind::General, 0, "ExposureBias_String", format!("{sign}{ev:.2} EV"), false);
        }
    }
    if let Some(mode) = exif.metering_mode {
        let s = match mode {
            0 => "Unknown",
            1 => "Average",
            2 => "Center-weighted average",
            3 => "Spot",
            4 => "Multi-spot",
            5 => "Pattern",
            6 => "Partial",
            255 => "Other",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "MeteringMode", s, false);
        }
    }
    if let Some(light) = exif.light_source {
        let s = match light {
            0 => "Unknown",
            1 => "Daylight",
            2 => "Fluorescent",
            3 => "Tungsten (incandescent)",
            4 => "Flash",
            9 => "Fine weather",
            10 => "Cloudy weather",
            11 => "Shade",
            12 => "Daylight fluorescent",
            13 => "Day white fluorescent",
            14 => "Cool white fluorescent",
            15 => "White fluorescent",
            17 => "Standard light A",
            18 => "Standard light B",
            19 => "Standard light C",
            20 => "D55",
            21 => "D65",
            22 => "D75",
            23 => "D50",
            24 => "ISO studio tungsten",
            255 => "Other",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "LightSource", s, false);
        }
    }
    if let Some(scene) = exif.scene_type {
        let s = match scene {
            1 => "Directly photographed",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "SceneType", s, false);
        }
    }
    if let Some(rendered) = exif.custom_rendered {
        let s = match rendered {
            0 => "Normal",
            1 => "Custom",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "CustomRendered", s, false);
        }
    }
    if let Some(mode) = exif.exposure_mode {
        let s = match mode {
            0 => "Auto",
            1 => "Manual",
            2 => "Auto bracket",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "ExposureMode", s, false);
        }
    }
    if let Some((n, d)) = exif.digital_zoom_ratio {
        if d > 0 {
            let ratio = n as f64 / d as f64;
            fa.Fill_Extra(StreamKind::General, 0, "DigitalZoomRatio", format!("{ratio:.2}"), false);
        }
    }
    if let Some(scene) = exif.scene_capture_type {
        let s = match scene {
            0 => "Standard",
            1 => "Landscape",
            2 => "Portrait",
            3 => "Night scene",
            4 => "Close-up",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "SceneCaptureType", s, false);
        }
    }
    if let Some(contrast) = exif.contrast {
        let s = match contrast {
            0 => "Normal",
            1 => "Soft",
            2 => "Hard",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "Contrast", s, false);
        }
    }
    if let Some(sat) = exif.saturation {
        let s = match sat {
            0 => "Normal",
            1 => "Low",
            2 => "High",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "Saturation", s, false);
        }
    }
    if let Some(sharp) = exif.sharpness {
        let s = match sharp {
            0 => "Normal",
            1 => "Soft",
            2 => "Hard",
            _ => "",
        };
        if !s.is_empty() {
            fa.Fill_Extra(StreamKind::General, 0, "Sharpness", s, false);
        }
    }

    fa.Stream_Prepare(StreamKind::Image);
    fa.Fill(StreamKind::Image, 0, "Format", "JPEG", false);
    fa.Fill(StreamKind::Image, 0, "Width", width.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Height", height.to_string(), false);
    let color_space = match components {
        1 => "Y",
        3 => "YUV",
        4 => "CMYK",
        _ => "Unknown",
    };
    fa.Fill(StreamKind::Image, 0, "ColorSpace", color_space, false);
    if components == 3 && sampling.len() == 3 {
        let y = sampling[0];
        let cb = sampling[1];
        let cr = sampling[2];
        // Standard subsampling ratios derived from Y's H×V vs Cb/Cr.
        let subsampling = if cb == cr && cb == (1, 1) {
            match y {
                (1, 1) => Some("4:4:4"),
                (2, 1) => Some("4:2:2"),
                (2, 2) => Some("4:2:0"),
                (4, 1) => Some("4:1:1"),
                (4, 2) => Some("4:1:0"),
                _ => None,
            }
        } else {
            None
        };
        if let Some(s) = subsampling {
            fa.Fill(StreamKind::Image, 0, "ChromaSubsampling", s, false);
        }
    }
    fa.Fill(StreamKind::Image, 0, "BitDepth", precision.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Compression_Mode", "Lossy", false);
    // Main Image.StreamSize = file - all-APP-overhead (the full overhead,
    // including the embedded thumbnail bytes). When a thumbnail exists
    // its bytes are NOT main-image data — keep using the full overhead.
    let image_size = file_size.saturating_sub(overhead);
    fa.Fill(StreamKind::Image, 0, "StreamSize", image_size.to_string(), false);
    if let Some(c) = comment {
        fa.Fill(StreamKind::Image, 0, "Comment", c, false);
    }

    // EXIF thumbnail → second Image stream (oracle labels it Type=Thumbnail,
    // MuxingMode=Exif). StreamSize comes from JPEGInterchangeFormatLength.
    if let (Some(tw), Some(th)) = (exif.thumbnail_width, exif.thumbnail_height) {
        let tpos = fa.Stream_Prepare(StreamKind::Image);
        fa.Fill(StreamKind::Image, tpos, "Type", "Thumbnail", false);
        fa.Fill(StreamKind::Image, tpos, "MuxingMode", "Exif", false);
        fa.Fill(StreamKind::Image, tpos, "Format", "JPEG", false);
        fa.Fill(StreamKind::Image, tpos, "Width", tw.to_string(), false);
        fa.Fill(StreamKind::Image, tpos, "Height", th.to_string(), false);
        if let Some(sz) = exif.thumbnail_size {
            fa.Fill(StreamKind::Image, tpos, "StreamSize", sz.to_string(), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_jpeg_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a JPEG file");
        assert!(!parse_jpeg(&mut fa));
    }
}
