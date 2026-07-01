use revelo_core::{FileAnalyze, StreamKind};
use std::io::Read;

type TagEntry = (&'static str, String);
const TAIL_TAG_SCAN_LIMIT: usize = 1024 * 1024;
const EMBEDDED_METADATA_SCAN_LIMIT: usize = 4 * 1024 * 1024;

fn metadata_prefix<'fa, 'src>(fa: &'fa FileAnalyze<'src>) -> Option<&'fa [u8]> {
    fa.peek_raw_at(0, fa.element_size().min(EMBEDDED_METADATA_SCAN_LIMIT))
}

fn fill_tags(fa: &mut FileAnalyze, tags: &[TagEntry], kind: StreamKind) {
    if tags.is_empty() {
        return;
    }
    // Embedded-metadata streams are singletons per file: a JPEG has one
    // EXIF block, one IPTC block, etc. The container parser may have
    // already created stream 0 of this kind (e.g. jpeg.rs fills curated
    // Exif fields); merge into it rather than appending a duplicate
    // stream. set_field is first-write-wins, so pre-existing values win.
    let pos = if fa.streams().stream_count(kind) > 0 { 0 } else { fa.stream_prepare(kind) };
    for (key, value) in tags {
        fa.set_field(kind, pos, key, value.as_str());
    }
}

// ---------- ID3v1 ----------

pub fn parse_id3v1(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.remain();
    if remain < 128 {
        return None;
    }
    let tag = fa.peek_raw_at(remain - 128, 128)?;
    if &tag[0..3] != b"TAG" {
        return None;
    }

    let title = trim_str(&tag[3..33]);
    let artist = trim_str(&tag[33..63]);
    let album = trim_str(&tag[63..93]);
    let year = trim_str(&tag[93..97]);
    let mut comment = trim_str(&tag[97..127]);

    let mut track: Option<u8> = None;
    if comment.len() == 30 && tag[125] == 0 {
        track = Some(tag[126]);
        comment = trim_str(&tag[97..125]);
    }

    let mut tags: Vec<TagEntry> = Vec::new();
    if !title.is_empty() {
        tags.push(("Title", title));
    }
    if !artist.is_empty() {
        tags.push(("Performer", artist));
    }
    if !album.is_empty() {
        tags.push(("Album", album));
    }
    if !year.is_empty() {
        tags.push(("Recorded_Date", year));
    }
    if !comment.is_empty() {
        if comment.contains("ExactAudioCopy") {
            tags.push(("Encoded_Application", comment));
        } else {
            tags.push(("Comment", comment));
        }
    }
    if let Some(t) = track {
        tags.push(("Track_Position", t.to_string()));
    }

    let genre = tag[127];
    if genre < 80 {
        tags.push(("Genre", id3v1_genre(genre).to_string()));
    }

    fill_tags(fa, &tags, StreamKind::General);
    Some(128)
}

fn trim_str(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).trim_end().to_string()
}

fn id3v1_genre(idx: u8) -> &'static str {
    match idx {
        0 => "Blues",
        1 => "Classic Rock",
        2 => "Country",
        3 => "Dance",
        4 => "Disco",
        5 => "Funk",
        6 => "Grunge",
        7 => "Hip-Hop",
        8 => "Jazz",
        9 => "Metal",
        10 => "New Age",
        11 => "Oldies",
        12 => "Other",
        13 => "Pop",
        14 => "R&B",
        15 => "Rap",
        16 => "Reggae",
        17 => "Rock",
        18 => "Techno",
        19 => "Industrial",
        20 => "Alternative",
        21 => "Ska",
        22 => "Death Metal",
        23 => "Pranks",
        24 => "Soundtrack",
        25 => "Euro-Techno",
        26 => "Ambient",
        27 => "Trip-Hop",
        28 => "Vocal",
        29 => "Jazz+Funk",
        30 => "Fusion",
        31 => "Trance",
        32 => "Classical",
        33 => "Instrumental",
        34 => "Acid",
        35 => "House",
        36 => "Game",
        37 => "Sound Clip",
        38 => "Gospel",
        39 => "Noise",
        40 => "Alternative Rock",
        41 => "Bass",
        42 => "Soul",
        43 => "Punk",
        44 => "Space",
        45 => "Meditative",
        46 => "Instrumental Pop",
        47 => "Instrumental Rock",
        48 => "Ethnic",
        49 => "Gothic",
        50 => "Darkwave",
        51 => "Techno-Industrial",
        52 => "Electronic",
        53 => "Pop-Folk",
        54 => "Eurodance",
        55 => "Dream",
        56 => "Southern Rock",
        57 => "Comedy",
        58 => "Cult",
        59 => "Gangsta",
        60 => "Top 40",
        61 => "Christian Rap",
        62 => "Pop/Funk",
        63 => "Jungle",
        64 => "Native American",
        65 => "Cabaret",
        66 => "New Wave",
        67 => "Psychadelic",
        68 => "Rave",
        69 => "Showtunes",
        70 => "Trailer",
        71 => "Lo-Fi",
        72 => "Tribal",
        73 => "Acid Punk",
        74 => "Acid Jazz",
        75 => "Polka",
        76 => "Retro",
        77 => "Musical",
        78 => "Rock & Roll",
        79 => "Hard Rock",
        _ => "Unknown",
    }
}

// ---------- ID3v2 ----------

pub fn parse_id3v2(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.remain();
    if remain < 10 {
        return None;
    }
    let header = fa.peek_raw(10)?;
    if &header[0..3] != b"ID3" {
        return None;
    }

    let version_major = header[3];
    let flags = header[5];
    let size = synch_safe_int(&header[6..10]);
    if size == 0 {
        return None;
    }

    let has_footer = (flags & 0x10) != 0;
    let total_size = size as usize + 10 + if has_footer { 10 } else { 0 };
    let buf = fa.peek_raw(total_size)?;

    let mut offset = 10;
    let end = size as usize + 10;
    let mut tags: Vec<TagEntry> = Vec::new();

    while offset + 10 <= end {
        let frame_id_len = if version_major >= 3 { 4 } else { 3 };
        let frame_id = String::from_utf8_lossy(&buf[offset..offset + frame_id_len]).to_string();
        offset += frame_id_len;
        if offset + 4 > end {
            break;
        }

        let frame_size = if version_major >= 4 {
            synch_safe_int(&buf[offset..offset + 4]) as usize
        } else {
            u32::from_be_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]])
                as usize
        };
        offset += 4;
        if version_major >= 3 {
            offset += 2;
        }

        if frame_id.as_bytes() == b"\0\0\0\0" || frame_id.is_empty() {
            break;
        }
        if frame_size == 0 || offset + frame_size > end {
            break;
        }

        let frame_data = &buf[offset..offset + frame_size];
        parse_id3v2_frame(&mut tags, &frame_id, frame_data);
        offset += frame_size;
    }

    fill_tags(fa, &tags, StreamKind::General);
    Some(total_size as u32)
}

fn synch_safe_int(bytes: &[u8]) -> u32 {
    let mut val = 0u32;
    for &b in bytes {
        val = (val << 7) | (b as u32 & 0x7F);
    }
    val
}

fn parse_id3v2_frame(tags: &mut Vec<TagEntry>, id: &str, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let encoding = data[0];
    let text_start = 1;
    let text = if encoding == 1 || encoding == 2 {
        read_utf16(&data[text_start..], encoding == 2)
    } else {
        String::from_utf8_lossy(&data[text_start..]).trim_end_matches('\0').to_string()
    };
    if text.is_empty() {
        return;
    }

    match id {
        "TIT2" | "TT2" => tags.push(("Title", text)),
        "TPE1" | "TP1" => tags.push(("Performer", text)),
        "TPE2" | "TP2" => tags.push(("Accompaniment", text)),
        "TALB" | "TAL" => tags.push(("Album", text)),
        "TRCK" | "TRK" => tags.push(("Track_Position", text)),
        "TPOS" | "TPA" => tags.push(("Part_Position", text)),
        "TDRC" | "TYER" | "TDA" => tags.push(("Recorded_Date", text)),
        "TCON" | "TCO" => tags.push(("Genre", text)),
        "TCOM" | "TCM" => tags.push(("Composer", text)),
        "TPUB" | "TPB" => tags.push(("Publisher", text)),
        "TLAN" | "TLA" => tags.push(("Language", text)),
        "TCOP" | "TCR" => tags.push(("Copyright", text)),
        _ => {}
    }
}

fn read_utf16(data: &[u8], big_endian: bool) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let b1 = data[i];
        let b2 = data[i + 1];
        let cp =
            if big_endian { u16::from_be_bytes([b1, b2]) } else { u16::from_le_bytes([b1, b2]) };
        if cp == 0 {
            break;
        }
        if let Some(c) = char::from_u32(cp as u32) {
            result.push(c);
        }
        i += 2;
    }
    result
}

// ---------- APE tag ----------

pub fn parse_ape_tag(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.remain();
    if remain < 32 {
        return None;
    }
    let tail_start = remain.saturating_sub(TAIL_TAG_SCAN_LIMIT);
    let buf = fa.peek_raw_at(tail_start, remain - tail_start)?;

    let footer_start = buf.windows(8).rposition(|w| w == b"APETAGEX")?;
    if footer_start + 32 > buf.len() {
        return None;
    }

    let footer = &buf[footer_start..footer_start + 32];
    let item_count = u32::from_le_bytes([footer[12], footer[13], footer[14], footer[15]]) as usize;
    let flags = u32::from_le_bytes([footer[16], footer[17], footer[18], footer[19]]);
    let has_header = (flags & 0x80000000) != 0;
    let tag_start = if has_header && footer_start >= 32 { footer_start - 32 } else { 0 };
    let tag_end = footer_start + 32;
    if tag_start >= tag_end || tag_end > buf.len() {
        return None;
    }

    let mut tags: Vec<TagEntry> = Vec::new();
    let mut offset = if has_header { tag_start + 32 } else { tag_start };

    for _ in 0..item_count.min(100) {
        if offset + 8 > tag_end {
            break;
        }
        let value_size =
            u32::from_le_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]])
                as usize;
        offset += 8;
        if offset >= tag_end {
            break;
        }
        let key_end = buf[offset..].iter().position(|&b| b == 0).unwrap_or(tag_end - offset);
        let key = String::from_utf8_lossy(&buf[offset..offset + key_end]).to_uppercase();
        offset += key_end + 1;
        if offset + value_size > tag_end {
            break;
        }
        let value = String::from_utf8_lossy(&buf[offset..offset + value_size])
            .trim_end_matches('\0')
            .to_string();
        offset += value_size;

        match key.as_str() {
            "TITLE" => tags.push(("Title", value)),
            "ARTIST" => tags.push(("Performer", value)),
            "ALBUM" => tags.push(("Album", value)),
            "YEAR" => tags.push(("Recorded_Date", value)),
            "TRACK" => tags.push(("Track_Position", value)),
            "GENRE" => tags.push(("Genre", value)),
            "COMMENT" => tags.push(("Comment", value)),
            "COMPOSER" => tags.push(("Composer", value)),
            _ => {}
        }
    }

    fill_tags(fa, &tags, StreamKind::General);
    Some((tag_end - tag_start) as u32)
}

// ---------- Vorbis Comment ----------

pub fn parse_vorbis_comment_from_buf(buf: &[u8], offset: &mut usize) -> Vec<TagEntry> {
    let mut tags = Vec::new();
    if *offset + 4 > buf.len() {
        return tags;
    }
    let vendor_len =
        u32::from_le_bytes([buf[*offset], buf[*offset + 1], buf[*offset + 2], buf[*offset + 3]])
            as usize;
    *offset += 4 + vendor_len;
    if *offset + 4 > buf.len() {
        return tags;
    }
    let count =
        u32::from_le_bytes([buf[*offset], buf[*offset + 1], buf[*offset + 2], buf[*offset + 3]])
            as usize;
    *offset += 4;

    for _ in 0..count.min(200) {
        if *offset + 4 > buf.len() {
            break;
        }
        let len = u32::from_le_bytes([
            buf[*offset],
            buf[*offset + 1],
            buf[*offset + 2],
            buf[*offset + 3],
        ]) as usize;
        *offset += 4;
        if *offset + len > buf.len() {
            break;
        }
        let entry = String::from_utf8_lossy(&buf[*offset..*offset + len]).to_string();
        *offset += len;
        if let Some(eq) = entry.find('=') {
            let key = entry[..eq].to_uppercase();
            let v = entry[eq + 1..].to_string();
            match key.as_str() {
                "TITLE" => tags.push(("Title", v)),
                "ARTIST" => tags.push(("Performer", v)),
                "ALBUM" => tags.push(("Album", v)),
                "DATE" => tags.push(("Recorded_Date", v)),
                "TRACKNUMBER" => tags.push(("Track_Position", v)),
                "GENRE" => tags.push(("Genre", v)),
                "DESCRIPTION" => tags.push(("Comment", v)),
                "COMPOSER" => tags.push(("Composer", v)),
                "PUBLISHER" => tags.push(("Publisher", v)),
                "COPYRIGHT" => tags.push(("Copyright", v)),
                "ENCODEDBY" => tags.push(("Encoded_Application", v)),
                _ => {}
            }
        }
    }
    tags
}

pub fn parse_vorbis_comment(fa: &mut FileAnalyze, offset: &mut usize, buf: &[u8]) {
    let tags = parse_vorbis_comment_from_buf(buf, offset);
    fill_tags(fa, &tags, StreamKind::General);
}

// ---------- Lyrics3 ----------

pub fn parse_lyrics3(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.remain();
    if remain < 20 {
        return None;
    }
    let tail_start = remain.saturating_sub(TAIL_TAG_SCAN_LIMIT);
    let buf = fa.peek_raw_at(tail_start, remain - tail_start)?;

    let start = buf.windows(11).position(|w| w == b"LYRICSBEGIN")?;
    if start + 20 > buf.len() {
        return None;
    }

    let mut offset = start + 11;
    let mut end = offset;
    let mut tags: Vec<TagEntry> = Vec::new();

    while offset + 8 < buf.len() {
        if &buf[offset..offset + 5] == b"LYR20" {
            end = offset;
            break;
        }
        let size_str = std::str::from_utf8(&buf[offset + 3..offset + 8]).ok().unwrap_or("0");
        let size: usize = size_str.trim().parse().unwrap_or(0);
        offset += 8;
        if offset + size > buf.len() {
            break;
        }
        let value = String::from_utf8_lossy(&buf[offset..offset + size]).trim().to_string();
        if offset >= 11 {
            match &buf[offset - 8..offset - 5] {
                b"EAL" => tags.push(("Album", value)),
                b"EAR" => tags.push(("Performer", value)),
                b"ETT" => tags.push(("Title", value)),
                b"IMG" => tags.push(("Cover_Description", value)),
                b"INF" => tags.push(("Comment", value)),
                _ => {}
            }
        }
        offset += size;
    }

    fill_tags(fa, &tags, StreamKind::General);
    Some((end - start) as u32)
}

// ---------- Generic ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exif_float_honors_byte_order() {
        // 1.5_f32 == 0x3FC0_0000. Big-endian bytes vs little-endian bytes.
        let be = 1.5_f32.to_be_bytes();
        let le = 1.5_f32.to_le_bytes();
        assert_eq!(read_exif_val(&be, 0, 11, 1, "BE"), "1.5");
        assert_eq!(read_exif_val(&le, 0, 11, 1, "LE"), "1.5");
        // Same for DOUBLE (type 12).
        let bed = 2.25_f64.to_be_bytes();
        assert_eq!(read_exif_val(&bed, 0, 12, 1, "BE"), "2.25");
        // Out-of-range offset must not panic.
        assert_eq!(read_exif_val(&[0u8; 2], 0, 11, 1, "BE"), "0");
    }

    #[test]
    fn id3v1_parses_basic_tag() {
        let mut buf = vec![0u8; 256];
        let start = 256 - 128;
        buf[start..start + 3].copy_from_slice(b"TAG");
        buf[start + 3..start + 33]
            .copy_from_slice(b"Test Song\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
        buf[start + 127] = 17; // Rock
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_id3v1(&mut fa).is_some());
    }

    #[test]
    fn id3v2_parses_header() {
        let mut buf = vec![0u8; 256];
        buf[0..3].copy_from_slice(b"ID3");
        buf[3] = 3;
        let size = 20u32;
        buf[6] = ((size >> 21) & 0x7F) as u8;
        buf[7] = ((size >> 14) & 0x7F) as u8;
        buf[8] = ((size >> 7) & 0x7F) as u8;
        buf[9] = (size & 0x7F) as u8;
        buf[10..14].copy_from_slice(b"TIT2");
        buf[14..18].copy_from_slice(&(7u32.to_be_bytes()));
        buf[20] = 0;
        buf[21..28].copy_from_slice(b"MySong\x00");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_id3v2(&mut fa).is_some());
    }

    #[test]
    fn ape_tag_detects_footer() {
        let mut buf = vec![0u8; 256];
        let start = 256 - 32;
        buf[start..start + 8].copy_from_slice(b"APETAGEX");
        buf[start + 12] = 1; // 1 item
        buf[0..4].copy_from_slice(&[4u32.to_le_bytes()[0], 0, 0, 0]);
        buf[8..15].copy_from_slice(b"Artist\0");
        buf[15..19].copy_from_slice(b"Test");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ape_tag(&mut fa).is_some());
    }

    #[test]
    fn parse_tags_uses_bounded_metadata_windows_on_large_files() {
        let buf = vec![0u8; EMBEDDED_METADATA_SCAN_LIMIT + 1024];
        let mut fa = FileAnalyze::new(&buf);

        assert!(parse_tags(&mut fa));

        let stats = fa.access_stats();
        assert!(stats.max_request_len <= EMBEDDED_METADATA_SCAN_LIMIT, "{stats:?}");
        assert!(stats.bytes_returned < 64 * 1024 * 1024, "{stats:?}");
    }

    #[test]
    fn parse_tags_skips_image_metadata_scans_for_known_containers() {
        let buf = vec![0u8; EMBEDDED_METADATA_SCAN_LIMIT + 1024];
        let mut fa = FileAnalyze::new(&buf);
        fa.stream_prepare(StreamKind::General);
        fa.set_field(StreamKind::General, 0, "Format", "MPEG-4");

        assert!(parse_tags(&mut fa));

        let stats = fa.access_stats();
        assert!(stats.bytes_returned < 1024, "{stats:?}");
        assert!(stats.max_request_len <= 128, "{stats:?}");
    }

    #[test]
    fn parse_tags_keeps_embedded_metadata_for_image_formats() {
        let buf =
            br#"prefix xmpmeta <rdf:RDF><xmp:CreatorTool>Camera</CreatorTool></rdf:RDF> suffix"#;
        let mut fa = FileAnalyze::new(buf);
        fa.stream_prepare(StreamKind::General);
        fa.set_field(StreamKind::General, 0, "Format", "JPEG");

        assert!(parse_tags(&mut fa));

        assert_eq!(
            fa.retrieve(StreamKind::Xmp, 0, "Encoded_Application").map(|z| z.as_str()),
            Some("Camera")
        );
    }
}

// ---------- EXIF ----------

/// Parse a TIFF/EXIF IFD from raw bytes at the given byte offset.
/// Returns the list of tag entries and the offset to the next IFD.
const EXIF_MAGIC: &[u8; 6] = b"Exif\0\0";

fn find_exif_header(buf: &[u8]) -> Option<usize> {
    buf.windows(EXIF_MAGIC.len())
        .position(|w| w == EXIF_MAGIC.as_slice())
        .map(|p| p + EXIF_MAGIC.len())
}

pub fn parse_exif(fa: &mut FileAnalyze) -> bool {
    let head = match fa.peek_raw_at(0, 12) {
        Some(b) => b,
        None => return false,
    };
    let raw_tiff = &head[0..2] == b"II" || &head[0..2] == b"MM";
    let scan_len = if raw_tiff {
        fa.element_size()
    } else {
        fa.element_size().min(EMBEDDED_METADATA_SCAN_LIMIT)
    };
    let buf = match fa.peek_raw_at(0, scan_len) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 12 {
        return false;
    }

    // Find TIFF data: either raw at offset 0 or embedded in JPEG APP1.
    let tiff_buf: &[u8] = if &buf[0..2] == b"II" || &buf[0..2] == b"MM" {
        &buf[..]
    } else if let Some(pos) = find_exif_header(&buf) {
        &buf[pos..]
    } else {
        return false;
    };
    let bo = if &tiff_buf[0..2] == b"II" { "LE" } else { "BE" };

    let ifd0_off = read_tiff_u32(tiff_buf, 4, bo) as usize;
    let mut tags: Vec<TagEntry> = Vec::new();
    let mut iptc_tags: Vec<TagEntry> = Vec::new();
    let mut exif_ptr = None;
    let mut gps_ptr = None;
    let mut interop_ptr = None;

    let next_ifd = parse_ifd(
        tiff_buf,
        ifd0_off,
        bo,
        &mut tags,
        &mut iptc_tags,
        &mut exif_ptr,
        &mut gps_ptr,
        &mut interop_ptr,
        &mut None,
        IfdKind::Tiff,
    );

    if let Some(ptr) = exif_ptr {
        let mut t = Vec::new();
        let mut sub_interop = None;
        let mut sub_makernote = None;
        parse_ifd(
            tiff_buf,
            ptr as usize,
            bo,
            &mut t,
            &mut iptc_tags,
            &mut None,
            &mut None,
            &mut sub_interop,
            &mut sub_makernote,
            IfdKind::Tiff,
        );
        tags.extend(t);

        if let Some((mn_off, mn_size)) = sub_makernote {
            let make =
                tags.iter().find(|(k, _)| *k == "Make").map(|(_, v)| v.clone()).unwrap_or_default();
            let mn_data = &tiff_buf[mn_off..mn_off + mn_size];
            parse_makernote(&make, mn_data, mn_off, &mut tags);
        }

        if let Some(ip) = sub_interop {
            let mut it = Vec::new();
            parse_ifd(
                tiff_buf,
                ip as usize,
                bo,
                &mut it,
                &mut Vec::new(),
                &mut None,
                &mut None,
                &mut None,
                &mut None,
                IfdKind::Interop,
            );
            tags.extend(it);
        }
    }
    if let Some(ptr) = gps_ptr {
        let mut t = Vec::new();
        parse_ifd(
            tiff_buf,
            ptr as usize,
            bo,
            &mut t,
            &mut Vec::new(),
            &mut None,
            &mut None,
            &mut None,
            &mut None,
            IfdKind::Gps,
        );
        tags.extend(t);
    }
    if let Some(n) = next_ifd {
        if n != 0 {
            parse_ifd(
                tiff_buf,
                n as usize,
                bo,
                &mut tags,
                &mut Vec::new(),
                &mut None,
                &mut None,
                &mut None,
                &mut None,
                IfdKind::Tiff,
            );
        }
    }

    compute_gps_decimal(&mut tags);
    compute_composites(&mut tags);
    format_apple_makernote(&mut tags);
    if let Some(entry) = tags.iter_mut().find(|(k, _)| *k == "LensSpecification") {
        entry.1 = format_lens_spec(&entry.1);
    }
    if let Some(pos) = tags.iter().position(|(k, _)| *k == "CanonLensType") {
        let val = tags[pos].1.clone();
        tags[pos].1 = format_lens_type("CanonLensType", &val);
    }
    // For JPEG, derive the MediaInfo-vocabulary EXIF fields (Recorded_Date,
    // Encoded_Hardware_*, the <extra> photographic block) from the parsed
    // ExifTool tags. This replaces the duplicate EXIF walk the JPEG container
    // parser used to carry; other formats are unaffected.
    let is_jpeg = fa
        .streams()
        .stream(StreamKind::General, 0)
        .and_then(|s| s.get("Format"))
        .map(|f| f.as_str() == "JPEG")
        .unwrap_or(false);
    if is_jpeg {
        let (regular, extras) = derive_jpeg_mediainfo(&tags);
        // Prepend the MediaInfo regular fields so JSON field order matches the
        // historical layout (these were emitted before the ExifTool tags).
        let mut merged = regular;
        merged.extend(tags);
        fill_tags(fa, &merged, StreamKind::Exif);
        for (k, v) in extras {
            fa.set_extra_field(StreamKind::Exif, 0, k, v);
        }
    } else {
        fill_tags(fa, &tags, StreamKind::Exif);
    }
    fill_tags(fa, &iptc_tags, StreamKind::Iptc);
    true
}

enum IfdKind {
    Tiff,
    Gps,
    Interop,
}

fn parse_ifd(
    data: &[u8],
    offset: usize,
    bo: &str,
    tags: &mut Vec<TagEntry>,
    iptc_tags: &mut Vec<TagEntry>,
    exif_ptr: &mut Option<u32>,
    gps_ptr: &mut Option<u32>,
    interop_ptr: &mut Option<u32>,
    makernote: &mut Option<(usize, usize)>,
    kind: IfdKind,
) -> Option<u32> {
    if offset + 2 > data.len() {
        return None;
    }
    let count = read_tiff_u16(data, offset, bo) as usize;
    let mut pos = offset + 2;

    for _ in 0..count.min(200) {
        if pos + 12 > data.len() {
            break;
        }
        let tag_id = read_tiff_u16(data, pos, bo);
        let tag_type = read_tiff_u16(data, pos + 2, bo);
        let tag_count = read_tiff_u32(data, pos + 4, bo) as usize;
        let tag_size = exif_type_size(tag_type) * tag_count;
        pos += 8;

        if tag_id == 0x927C {
            let mn_off = if tag_size <= 4 { pos } else { read_tiff_u32(data, pos, bo) as usize };
            if mn_off + tag_size <= data.len() {
                *makernote = Some((mn_off, tag_size));
            }
        }

        match tag_id {
            0x8769 => {
                let v = read_tiff_u32(data, pos, bo);
                *exif_ptr = Some(v);
                pos += 4;
                continue;
            }
            0x8825 => {
                let v = read_tiff_u32(data, pos, bo);
                *gps_ptr = Some(v);
                pos += 4;
                continue;
            }
            0xA005 => {
                let v = read_tiff_u32(data, pos, bo);
                *interop_ptr = Some(v);
                pos += 4;
                continue;
            }
            0x927C => {
                pos += 4;
                continue;
            }
            0x9286 => {
                let val_off =
                    if tag_size <= 4 { pos } else { read_tiff_u32(data, pos, bo) as usize };
                if val_off + tag_size <= data.len() {
                    let raw = &data[val_off..val_off + tag_count.min(tag_size)];
                    let text = if raw.len() > 8 {
                        String::from_utf8_lossy(&raw[8..]).trim_matches('\0').trim().to_string()
                    } else {
                        String::new()
                    };
                    tags.push(("UserComment", text));
                }
                pos += 4;
                continue;
            }
            // PrintIM (Print Image Matching): "PrintIM\0" + 4-char version.
            0xC4A5 => {
                let val_off =
                    if tag_size <= 4 { pos } else { read_tiff_u32(data, pos, bo) as usize };
                if val_off + 12 <= data.len() && &data[val_off..val_off + 8] == b"PrintIM\0" {
                    let version = String::from_utf8_lossy(&data[val_off + 8..val_off + 12])
                        .trim_matches('\0')
                        .to_string();
                    tags.push(("PrintIMVersion", version));
                }
                pos += 4;
                continue;
            }
            // Windows XP tags (UCS-2 / UTF-16LE byte arrays).
            0x9C9B | 0x9C9C | 0x9C9D | 0x9C9E | 0x9C9F => {
                let val_off =
                    if tag_size <= 4 { pos } else { read_tiff_u32(data, pos, bo) as usize };
                if val_off + tag_size <= data.len() {
                    let u16s: Vec<u16> = data[val_off..val_off + tag_size]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .collect();
                    let text = String::from_utf16_lossy(&u16s).trim_end_matches('\0').to_string();
                    let name = match tag_id {
                        0x9C9B => "XPTitle",
                        0x9C9C => "XPComment",
                        0x9C9D => "XPAuthor",
                        0x9C9E => "XPKeywords",
                        _ => "XPSubject",
                    };
                    if !text.is_empty() {
                        tags.push((name, text));
                    }
                }
                pos += 4;
                continue;
            }
            0x83BB => {
                let iim_data = if tag_size <= 4 {
                    let end = pos + tag_size.min(data.len() - pos);
                    data[pos..end].to_vec()
                } else {
                    let off = read_tiff_u32(data, pos, bo) as usize;
                    let end = off + tag_size.min(data.len() - off);
                    data[off..end].to_vec()
                };
                parse_iim_buf(&iim_data, iptc_tags);
                pos += 4;
                continue;
            }
            _ => {}
        }

        let value = if tag_size <= 4 {
            if tag_type == 2 {
                let end = data[pos..pos + tag_count.min(4)]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count.min(4));
                String::from_utf8_lossy(&data[pos..pos + end]).to_string()
            } else {
                read_exif_val(data, pos, tag_type, 1, bo)
            }
        } else {
            let val_off = read_tiff_u32(data, pos, bo) as usize;
            if val_off + tag_size > data.len() {
                pos += 4;
                continue;
            }
            if tag_type == 2 {
                let end = data[val_off..val_off + tag_count]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count);
                String::from_utf8_lossy(&data[val_off..val_off + end]).to_string()
            } else {
                read_exif_val(data, val_off, tag_type, tag_count, bo)
            }
        };
        pos += 4;

        let value_trimmed = if tag_id == 0x010E { value.trim_end().to_string() } else { value };

        let n = match tag_id {
            // === IFD0 / IFD1 (TIFF Rev 6.0 attributes) ===
            0x0100 => "ImageWidth",
            0x0101 => "ImageHeight",
            0x0102 => "BitsPerSample",
            0x0103 => "Compression",
            0x0106 => "PhotometricInterpretation",
            0x010E => "ImageDescription",
            0x010F => "Make",
            0x0110 => "Model",
            0x0111 => "StripOffsets",
            0x0112 => "Orientation",
            0x0115 => "SamplesPerPixel",
            0x0116 => "RowsPerStrip",
            0x0117 => "StripByteCounts",
            0x011A => "XResolution",
            0x011B => "YResolution",
            0x011C => "PlanarConfiguration",
            0x0128 => "ResolutionUnit",
            0x012D => "TransferFunction",
            0x0131 => "Software",
            0x0132 => "DateTime",
            0x013B => "Artist",
            0x013E => "WhitePoint",
            0x013F => "PrimaryChromaticities",
            0x0144 => "TileOffsets",
            0x0145 => "TileByteCounts",
            0x0201 => "ThumbnailOffset",
            0x0202 => "ThumbnailLength",
            0x0211 => "YCbCrCoefficients",
            0x0212 => "YCbCrSubSampling",
            0x0213 => "YCbCrPositioning",
            0x0214 => "ReferenceBlackWhite",
            0x8298 => "Copyright",

            // === SubIFD (Exif IFD — camera & image metadata) ===
            0x829A => "ExposureTime",
            0x829D => "FNumber",
            0x8822 => "ExposureProgram",
            0x8824 => "SpectralSensitivity",
            0x8827 => "PhotographicSensitivity",
            0x8828 => "OECF",
            0x8830 => "SensitivityType",
            0x8831 => "StandardOutputSensitivity",
            0x8832 => "RecommendedExposureIndex",
            0x8833 => "ISOSpeed",
            0x8834 => "ISOSpeedLatitudeyyy",
            0x8835 => "ISOSpeedLatitudezzz",
            0x9000 => "ExifVersion",
            0x9003 => "DateTimeOriginal",
            0x9004 => "DateTimeDigitized",
            0x9010 => "OffsetTime",
            0x9011 => "OffsetTimeOriginal",
            0x9012 => "OffsetTimeDigitized",
            0x9101 => "ComponentsConfiguration",
            0x9102 => "CompressedBitsPerPixel",
            0x9201 => "ShutterSpeedValue",
            0x9202 => "ApertureValue",
            0x9203 => "BrightnessValue",
            0x9204 => "ExposureBiasValue",
            0x9205 => "MaxApertureValue",
            0x9206 => "SubjectDistance",
            0x9207 => "MeteringMode",
            0x9208 => "LightSource",
            0x9209 => "Flash",
            0x920A => "FocalLength",
            0x9214 => "SubjectArea",
            0x927C => "MakerNote",
            0x9286 => "UserComment",
            0x9290 => "SubSecTime",
            0x9291 => "SubSecTimeOriginal",
            0x9292 => "SubSecTimeDigitized",
            0x9400 => "Temperature",
            0x9401 => "Humidity",
            0x9402 => "Pressure",
            0x9403 => "WaterDepth",
            0x9404 => "Acceleration",
            0x9405 => "CameraElevationAngle",
            0xA000 => "FlashpixVersion",
            0xA001 => "ColorSpace",
            0xA002 => "PixelXDimension",
            0xA003 => "PixelYDimension",
            0xA004 => "RelatedSoundFile",
            0xA20B => "FlashEnergy",
            0xA20C => "SpatialFrequencyResponse",
            0xA20E => "FocalPlaneXResolution",
            0xA20F => "FocalPlaneYResolution",
            0xA210 => "FocalPlaneResolutionUnit",
            0xA214 => "SubjectLocation",
            0xA215 => "ExposureIndex",
            0xA217 => "SensingMethod",
            0xA300 => "FileSource",
            0xA301 => "SceneType",
            0xA302 => "CFAPattern",
            0xA401 => "CustomRendered",
            0xA402 => "ExposureMode",
            0xA403 => "WhiteBalance",
            0xA404 => "DigitalZoomRatio",
            0xA405 => "FocalLengthIn35mmFilm",
            0xA406 => "SceneCaptureType",
            0xA407 => "GainControl",
            0xA408 => "Contrast",
            0xA409 => "Saturation",
            0xA40A => "Sharpness",
            0xA40B => "DeviceSettingDescription",
            0xA40C => "SubjectDistanceRange",
            0xA420 => "ImageUniqueID",
            0xA430 => "CameraOwnerName",
            0xA431 => "BodySerialNumber",
            0xA432 => "LensSpecification",
            0xA433 => "LensMake",
            0xA434 => "LensModel",
            0xA435 => "LensSerialNumber",
            0xA460 => "CompositeImage",
            0xA461 => "SourceImageNumberOfCompositeImage",
            0xA462 => "SourceExposureTimesOfCompositeImage",
            0xA500 => "Gamma",

            // === GPS & Interoperability attributes ===
            // (context-sensitive — tag IDs overlap in the 0x00xx range)
            0x0000 => match kind {
                IfdKind::Interop => "",
                _ => "GPSVersionID",
            },
            0x0001 => match kind {
                IfdKind::Interop => "InteroperabilityIndex",
                _ => "GPSLatitudeRef",
            },
            0x0002 => match kind {
                IfdKind::Interop => "InteroperabilityVersion",
                _ => "GPSLatitude",
            },
            0x0003 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSLongitudeRef"
                }
            }
            0x0004 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSLongitude"
                }
            }
            0x0005 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSAltitudeRef"
                }
            }
            0x0006 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSAltitude"
                }
            }
            0x0007 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSTimestamp"
                }
            }
            0x0008 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSSatellites"
                }
            }
            0x0009 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSStatus"
                }
            }
            0x000A => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSMeasureMode"
                }
            }
            0x000B => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDOP"
                }
            }
            0x000C => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSSpeedRef"
                }
            }
            0x000D => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSSpeed"
                }
            }
            0x000E => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSTrackRef"
                }
            }
            0x000F => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSTrack"
                }
            }
            0x0010 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSImgDirectionRef"
                }
            }
            0x0011 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSImgDirection"
                }
            }
            0x0012 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSMapDatum"
                }
            }
            0x0013 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestLatitudeRef"
                }
            }
            0x0014 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestLatitude"
                }
            }
            0x0015 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestLongitudeRef"
                }
            }
            0x0016 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestLongitude"
                }
            }
            0x0017 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestBearingRef"
                }
            }
            0x0018 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestBearing"
                }
            }
            0x0019 => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestDistanceRef"
                }
            }
            0x001A => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDestDistance"
                }
            }
            0x001B => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSProcessingMethod"
                }
            }
            0x001C => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSAreaInformation"
                }
            }
            0x001D => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDateStamp"
                }
            }
            0x001E => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSDifferential"
                }
            }
            0x001F => {
                if let IfdKind::Interop = kind {
                    ""
                } else {
                    "GPSHPositioningError"
                }
            }

            // Interoperability-only tags (no ID overlap)
            0x1000 => {
                if let IfdKind::Interop = kind {
                    "RelatedImageFileFormat"
                } else {
                    ""
                }
            }
            0x1001 => {
                if let IfdKind::Interop = kind {
                    "RelatedImageWidth"
                } else {
                    ""
                }
            }
            0x1002 => {
                if let IfdKind::Interop = kind {
                    "RelatedImageHeight"
                } else {
                    ""
                }
            }

            _ => "",
        };
        if !n.is_empty() {
            // Align EXIF/Interop tag names with exiftool where they diverge
            // (e.g. DateTime -> ModifyDate, InteroperabilityIndex ->
            // InteropIndex). Keyed by name, which is unambiguous per IFD, so
            // GPS names (none of which are aliased) are unaffected.
            #[cfg(feature = "exiftool-tables")]
            let n = if matches!(kind, IfdKind::Tiff | IfdKind::Interop) {
                revelo_exiftool_tables::exif_name_alias(n).unwrap_or(n)
            } else {
                n
            };
            tags.push((n, value_trimmed));
        }
    }

    if pos + 4 <= data.len() { Some(read_tiff_u32(data, pos, bo)) } else { None }
}

fn read_exif_val(data: &[u8], off: usize, t: u16, count: usize, bo: &str) -> String {
    if count == 1 {
        return match t {
            1 | 6 => {
                if off >= data.len() {
                    return "0".into();
                }
                if t == 1 { data[off].to_string() } else { (data[off] as i8).to_string() }
            }
            3 => read_tiff_u16(data, off, bo).to_string(),
            8 => (read_tiff_u16(data, off, bo) as i16).to_string(),
            4 | 13 => read_tiff_u32(data, off, bo).to_string(),
            9 => (read_tiff_u32(data, off, bo) as i32).to_string(),
            5 | 10 => {
                let num = read_tiff_u32(data, off, bo);
                let den = read_tiff_u32(data, off + 4, bo);
                if den == 0 {
                    "inf".into()
                } else if num % den == 0 {
                    (num / den).to_string()
                } else {
                    format!("{num}/{den}")
                }
            }
            11 => read_tiff_f32(data, off, bo).to_string(),
            12 => read_tiff_f64(data, off, bo).to_string(),
            7 => {
                let end = data[off..].iter().take(4).position(|&b| b == 0).unwrap_or(4);
                String::from_utf8_lossy(&data[off..off + end]).to_string()
            }
            _ => read_tiff_u32(data, off, bo).to_string(),
        };
    }

    // UNDEFINED (type 7) is a single byte blob (serial numbers, version
    // strings), not an array of per-byte strings. Render it as one trimmed
    // string rather than repeating it from every offset.
    if t == 7 {
        if off + count > data.len() {
            return String::new();
        }
        return String::from_utf8_lossy(&data[off..off + count])
            .trim_matches('\0')
            .trim()
            .to_string();
    }

    let elem = exif_type_size(t);
    let mut vals = Vec::with_capacity(count);
    let mut o = off;
    for _ in 0..count {
        let v = match t {
            1 => data[o].to_string(),
            3 => read_tiff_u16(data, o, bo).to_string(),
            4 => read_tiff_u32(data, o, bo).to_string(),
            5 | 10 => {
                let num = read_tiff_u32(data, o, bo);
                let den = read_tiff_u32(data, o + 4, bo);
                o += 8;
                if den == 0 {
                    "inf".into()
                } else if num % den == 0 {
                    (num / den).to_string()
                } else {
                    format!("{num}/{den}")
                }
            }
            9 => (read_tiff_u32(data, o, bo) as i32).to_string(),
            11 => read_tiff_f32(data, o, bo).to_string(),
            12 => read_tiff_f64(data, o, bo).to_string(),
            7 => {
                let end = data[o..].iter().take(count).position(|&b| b == 0).unwrap_or(count);
                String::from_utf8_lossy(&data[o..o + end.min(count)]).to_string()
            }
            _ => read_tiff_u32(data, o, bo).to_string(),
        };
        if t != 5 && t != 10 {
            o += elem;
        }
        vals.push(v);
    }
    vals.join(", ")
}

fn exif_type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 | 13 => 4,
        5 | 10 | 12 => 8,
        _ => 1,
    }
}

fn read_tiff_u16(data: &[u8], off: usize, bo: &str) -> u16 {
    if bo == "BE" {
        u16::from_be_bytes([data[off], data[off + 1]])
    } else {
        u16::from_le_bytes([data[off], data[off + 1]])
    }
}

fn read_tiff_u32(data: &[u8], off: usize, bo: &str) -> u32 {
    if bo == "BE" {
        u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
    } else {
        u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
    }
}

/// Reads a 32-bit IEEE-754 float (EXIF type 11) honoring the TIFF byte order
/// (`bo == "BE"` → big-endian). Returns `0.0` if the 4-byte value would run
/// past the end of `data`.
fn read_tiff_f32(data: &[u8], off: usize, bo: &str) -> f32 {
    if off + 4 > data.len() {
        return 0.0;
    }
    let b = [data[off], data[off + 1], data[off + 2], data[off + 3]];
    if bo == "BE" { f32::from_be_bytes(b) } else { f32::from_le_bytes(b) }
}

/// Reads a 64-bit IEEE-754 double (EXIF type 12) honoring the TIFF byte order.
/// Returns `0.0` if the 8-byte value would run past the end of `data`.
fn read_tiff_f64(data: &[u8], off: usize, bo: &str) -> f64 {
    if off + 8 > data.len() {
        return 0.0;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(&data[off..off + 8]);
    if bo == "BE" { f64::from_be_bytes(b) } else { f64::from_le_bytes(b) }
}

// ---------- GPS Computed Geotagging ----------

fn parse_gps_rational(val: &str) -> Option<f64> {
    let val = val.trim();
    if let Some(slash) = val.find('/') {
        let num: f64 = val[..slash].trim().parse().ok()?;
        let den: f64 = val[slash + 1..].trim().parse().ok()?;
        if den == 0.0 {
            return None;
        }
        Some(num / den)
    } else {
        val.parse::<f64>().ok()
    }
}

fn compute_gps_decimal(tags: &mut Vec<TagEntry>) {
    let lat_ref = tags
        .iter()
        .find(|(k, _)| *k == "GPSLatitudeRef")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let lat_str = tags.iter().find(|(k, _)| *k == "GPSLatitude").map(|(_, v)| v.clone());
    let lon_ref = tags
        .iter()
        .find(|(k, _)| *k == "GPSLongitudeRef")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let lon_str = tags.iter().find(|(k, _)| *k == "GPSLongitude").map(|(_, v)| v.clone());

    if let Some(lat) = lat_str {
        let parts: Vec<&str> = lat.split(',').collect();
        if parts.len() == 3 {
            let d = parse_gps_rational(parts[0]).unwrap_or(0.0);
            let m = parse_gps_rational(parts[1]).unwrap_or(0.0);
            let s = parse_gps_rational(parts[2]).unwrap_or(0.0);
            let mut decimal = d + m / 60.0 + s / 3600.0;
            if lat_ref == "S" {
                decimal = -decimal;
            }
            tags.push(("GPSLatitudeDecimal", format!("{:.6}", decimal)));
        }
    }

    if let Some(lon) = lon_str {
        let parts: Vec<&str> = lon.split(',').collect();
        if parts.len() == 3 {
            let d = parse_gps_rational(parts[0]).unwrap_or(0.0);
            let m = parse_gps_rational(parts[1]).unwrap_or(0.0);
            let s = parse_gps_rational(parts[2]).unwrap_or(0.0);
            let mut decimal = d + m / 60.0 + s / 3600.0;
            if lon_ref == "W" {
                decimal = -decimal;
            }
            tags.push(("GPSLongitudeDecimal", format!("{:.6}", decimal)));
        }
    }
}

/// Derive photographic composite values (ExifTool "Composite" group) from the
/// already-parsed EXIF tags: scale factor to 35 mm, circle of confusion, field
/// of view, hyperfocal distance, and light value. Each is emitted only when its
/// inputs are present, so files lacking the source tags are unaffected.
fn compute_composites(tags: &mut Vec<TagEntry>) {
    let get = |name: &str| -> Option<f64> {
        tags.iter().find(|(k, _)| *k == name).and_then(|(_, v)| parse_gps_rational(v))
    };
    let focal = get("FocalLength");
    let focal35 = get("FocalLengthIn35mmFilm");
    let fnumber = get("FNumber");
    let exposure = get("ExposureTime");
    let iso = get("PhotographicSensitivity").or_else(|| get("ISOSpeed"));

    // Scale factor to 35 mm equivalent = 35 mm-equiv focal / actual focal.
    let scale = match (focal35, focal) {
        (Some(f35), Some(f)) if f > 0.0 => Some(f35 / f),
        _ => None,
    };
    if let Some(s) = scale {
        tags.push(("ScaleFactor35efl", format!("{s:.1}")));
    }

    // Circle of confusion = full-frame diagonal CoC / crop factor.
    let coc =
        scale.filter(|s| *s > 0.0).map(|s| (24.0_f64 * 24.0 + 36.0 * 36.0).sqrt() / 1440.0 / s);
    if let Some(c) = coc {
        tags.push(("CircleOfConfusion", format!("{c:.3} mm")));
    }

    // Field of view (diagonal-equivalent against full-frame 36 mm width).
    let efl = focal35.or(match (focal, scale) {
        (Some(f), Some(s)) => Some(f * s),
        _ => None,
    });
    if let Some(e) = efl.filter(|e| *e > 0.0) {
        let fov = 2.0 * (36.0 / (2.0 * e)).atan().to_degrees();
        tags.push(("FieldOfView", format!("{fov:.1} deg")));
    }

    // Hyperfocal distance (m) = focal² / (N · CoC · 1000).
    if let (Some(f), Some(n), Some(c)) = (focal, fnumber, coc)
        && n > 0.0
        && c > 0.0
    {
        let h = f * f / (n * c * 1000.0);
        tags.push(("HyperfocalDistance", format!("{h:.2} m")));
    }

    // Light value (EV at ISO 100) = log2(N²/t) − log2(ISO/100).
    if let (Some(n), Some(t), Some(i)) = (fnumber, exposure, iso)
        && n > 0.0
        && t > 0.0
        && i > 0.0
    {
        let lv = (n * n / t).log2() - (i / 100.0).log2();
        tags.push(("LightValue", format!("{lv:.1}")));
    }
}

/// Parse an "n/d" rational string into (num, den) unsigned.
fn parse_ratio_pair(s: &str) -> Option<(u32, u32)> {
    let s = s.trim();
    if let Some((n, d)) = s.split_once('/') {
        Some((n.trim().parse().ok()?, d.trim().parse().ok()?))
    } else {
        Some((s.parse().ok()?, 1))
    }
}

/// Parse an "n/d" rational, reinterpreting each field as signed (SRATIONAL).
fn parse_sratio_pair(s: &str) -> Option<(i64, i64)> {
    let s = s.trim();
    let sign = |x: i64| if x > i32::MAX as i64 { x - (1i64 << 32) } else { x };
    if let Some((n, d)) = s.split_once('/') {
        Some((sign(n.trim().parse().ok()?), sign(d.trim().parse().ok()?)))
    } else {
        Some((sign(s.parse().ok()?), 1))
    }
}

/// "0232" / "0100" EXIF version bytes → "2.32" / "1.00".
fn exif_version_str(s: &str) -> Option<String> {
    if s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()) {
        Some(format!("{}.{}{}", &s[1..2], &s[2..3], &s[3..4]))
    } else {
        None
    }
}

/// "MM:DD" date separators (`exif_datetime_to_oracle` analogue): EXIF
/// "2018:12:10 15:44:06" → MediaInfo "2018-12-10 15:44:06".
fn exif_datetime_to_oracle(s: &str) -> String {
    let mut out: Vec<u8> = s.bytes().collect();
    if out.len() >= 10 && out[4] == b':' && out[7] == b':' {
        out[4] = b'-';
        out[7] = b'-';
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

/// Derive the MediaInfo-vocabulary EXIF fields from parsed ExifTool `tags`,
/// reproducing the JPEG container parser's historical output. Returns the
/// regular fields (merged into the Exif stream ahead of the ExifTool tags) and
/// the `<extra>` photographic block, in their original display order.
fn derive_jpeg_mediainfo(tags: &[TagEntry]) -> (Vec<TagEntry>, Vec<TagEntry>) {
    let get = |name: &str| tags.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str());
    let mut reg: Vec<TagEntry> = Vec::new();
    let mut ex: Vec<TagEntry> = Vec::new();

    if let Some(d) = get("ImageDescription").filter(|s| !s.is_empty()) {
        reg.push(("Description", d.to_string()));
    }
    if let Some(dt) = get("DateTimeOriginal") {
        reg.push(("Recorded_Date", exif_datetime_to_oracle(dt)));
    }
    if let Some(dt) = get("DateTime") {
        reg.push(("Mastered_Date", exif_datetime_to_oracle(dt)));
    }
    if let Some(m) = get("Make").map(str::trim).filter(|s| !s.is_empty()) {
        let n = m
            .trim_end_matches(" Inc.")
            .trim_end_matches(" Corporation")
            .trim_end_matches(" CORPORATION")
            .trim()
            .to_string();
        reg.push(("Encoded_Hardware_CompanyName", n));
    }
    if let Some(m) = get("Model").map(str::trim).filter(|s| !s.is_empty()) {
        reg.push(("Encoded_Hardware_Model", m.to_string()));
    }

    if let Some((n, d)) = get("ExposureTime").and_then(parse_ratio_pair).filter(|(_, d)| *d > 0) {
        ex.push(("ShutterSpeed_Time", format!("{:.6}", n as f64 / d as f64)));
        if n == 1 {
            ex.push(("ShutterSpeed_Time_String", format!("1/{d} s")));
        }
    }
    if let Some((n, d)) = get("FNumber").and_then(parse_ratio_pair).filter(|(_, d)| *d > 0) {
        ex.push(("IrisFNumber", format!("{:.1}", n as f64 / d as f64)));
    }
    if let Some(s) = get("ExposureProgram").and_then(|v| {
        Some(match v {
            "0" => "Not Defined",
            "1" => "Manual",
            "2" => "Normal",
            "3" => "Aperture priority",
            "4" => "Shutter priority",
            "5" => "Creative",
            "6" => "Action",
            "7" => "Portrait",
            "8" => "Landscape",
            _ => return None,
        })
    }) {
        ex.push(("AutoExposureMode", s.to_string()));
    }
    if let Some(iso) = get("PhotographicSensitivity").or_else(|| get("ISOSpeed")) {
        ex.push(("ISOSensitivity", iso.to_string()));
    }
    if let Some(s) = get("ExifVersion").and_then(exif_version_str) {
        ex.push(("ExifVersion", s));
    }
    if let Some(s) = get("Flash").and_then(|v| v.parse::<u16>().ok()).and_then(|f| {
        Some(match f {
            0x0000 | 0x0010 => "Off, Did not fire",
            0x0001 => "Fired",
            0x0018 => "Auto, Did not fire",
            0x0019 => "Auto, Fired",
            _ => return None,
        })
    }) {
        ex.push(("Flash", s.to_string()));
    }
    if let Some((n, d)) = get("FocalLength").and_then(parse_ratio_pair).filter(|(_, d)| *d > 0) {
        let mm = n as f64 / d as f64;
        let i = mm.round() as u32;
        if (mm - i as f64).abs() < 0.01 {
            ex.push(("LensZoomActualFocalLength", i.to_string()));
            ex.push(("LensZoomActualFocalLength_String", format!("{i} mm")));
        } else {
            ex.push(("LensZoomActualFocalLength", format!("{mm:.1}")));
            ex.push(("LensZoomActualFocalLength_String", format!("{mm:.1} mm")));
        }
    }
    if let Some(s) = get("FlashpixVersion").and_then(exif_version_str) {
        ex.push(("FlashpixVersion", s));
    }
    if let Some(s) = get("WhiteBalance").and_then(|v| match v {
        "0" => Some("Auto"),
        "1" => Some("Manual"),
        _ => None,
    }) {
        ex.push(("AutoWhiteBalanceMode", s.to_string()));
    }
    if let Some(fl35) = get("FocalLengthIn35mmFilm") {
        ex.push(("LensZoom35mmStillCameraEquivalent", fl35.to_string()));
        ex.push(("LensZoom35mmStillCameraEquivalent_String", format!("{fl35} mm")));
    }
    if let Some(l) = get("LensModel") {
        ex.push(("LensModel", l.to_string()));
    }
    if let Some((n, d)) =
        get("ExposureBiasValue").and_then(parse_sratio_pair).filter(|(_, d)| *d != 0)
    {
        let ev = n as f64 / d as f64;
        let sign = if ev >= 0.0 { "+" } else { "" };
        ex.push(("ExposureBias", format!("{sign}{ev:.2}")));
        ex.push(("ExposureBias_String", format!("{sign}{ev:.2} EV")));
    }
    if let Some(s) = get("MeteringMode").and_then(|v| {
        Some(match v {
            "0" => "Unknown",
            "1" => "Average",
            "2" => "Center-weighted average",
            "3" => "Spot",
            "4" => "Multi-spot",
            "5" => "Pattern",
            "6" => "Partial",
            "255" => "Other",
            _ => return None,
        })
    }) {
        ex.push(("MeteringMode", s.to_string()));
    }
    if let Some(s) = get("LightSource").and_then(|v| {
        Some(match v {
            "0" => "Unknown",
            "1" => "Daylight",
            "2" => "Fluorescent",
            "3" => "Tungsten (incandescent)",
            "4" => "Flash",
            "9" => "Fine weather",
            "10" => "Cloudy weather",
            "11" => "Shade",
            "12" => "Daylight fluorescent",
            "13" => "Day white fluorescent",
            "14" => "Cool white fluorescent",
            "15" => "White fluorescent",
            "17" => "Standard light A",
            "18" => "Standard light B",
            "19" => "Standard light C",
            "20" => "D55",
            "21" => "D65",
            "22" => "D75",
            "23" => "D50",
            "24" => "ISO studio tungsten",
            "255" => "Other",
            _ => return None,
        })
    }) {
        ex.push(("LightSource", s.to_string()));
    }
    // SceneType is a single UNDEFINED byte; only the first byte matters (some
    // files carry trailing junk). 1 = directly photographed.
    if let Some(v) = get("SceneType")
        && (v.as_bytes().first() == Some(&1) || v == "1")
    {
        ex.push(("SceneType", "Directly photographed".to_string()));
    }
    if let Some(s) = get("CustomRendered").and_then(|v| match v {
        "0" => Some("Normal"),
        "1" => Some("Custom"),
        _ => None,
    }) {
        ex.push(("CustomRendered", s.to_string()));
    }
    if let Some(s) = get("ExposureMode").and_then(|v| match v {
        "0" => Some("Auto"),
        "1" => Some("Manual"),
        "2" => Some("Auto bracket"),
        _ => None,
    }) {
        ex.push(("ExposureMode", s.to_string()));
    }
    if let Some((n, d)) = get("DigitalZoomRatio").and_then(parse_ratio_pair).filter(|(_, d)| *d > 0)
    {
        ex.push(("DigitalZoomRatio", format!("{:.2}", n as f64 / d as f64)));
    }
    if let Some(s) = get("SceneCaptureType").and_then(|v| {
        Some(match v {
            "0" => "Standard",
            "1" => "Landscape",
            "2" => "Portrait",
            "3" => "Night scene",
            "4" => "Close-up",
            _ => return None,
        })
    }) {
        ex.push(("SceneCaptureType", s.to_string()));
    }
    let norm_soft_hard = |v: &str| match v {
        "0" => Some("Normal"),
        "1" => Some("Soft"),
        "2" => Some("Hard"),
        _ => None,
    };
    if let Some(s) = get("Contrast").and_then(norm_soft_hard) {
        ex.push(("Contrast", s.to_string()));
    }
    if let Some(s) = get("Saturation").and_then(|v| match v {
        "0" => Some("Normal"),
        "1" => Some("Low"),
        "2" => Some("High"),
        _ => None,
    }) {
        ex.push(("Saturation", s.to_string()));
    }
    if let Some(s) = get("Sharpness").and_then(norm_soft_hard) {
        ex.push(("Sharpness", s.to_string()));
    }

    (reg, ex)
}

// ---------- XMP ----------

pub fn parse_xmp(fa: &mut FileAnalyze) -> bool {
    // Read a bounded prefix cursor-independently: in a JPEG the XMP packet
    // normally sits in an early APP1 segment, behind the cursor by the time
    // this pass runs. Lossy decoding keeps the ASCII XMP region intact while
    // tolerating surrounding binary image bytes.
    let buf = match metadata_prefix(fa) {
        Some(b) => b,
        None => return false,
    };
    if find_subslice(buf, b"xmpmeta").is_none() || find_subslice(buf, b"rdf:RDF").is_none() {
        return false;
    }
    let text_owned = String::from_utf8_lossy(buf).into_owned();
    let text: &str = &text_owned;

    let mut tags: Vec<TagEntry> = Vec::new();
    extract_xmp_fields(text, &mut tags);
    fill_tags(fa, &tags, StreamKind::Xmp);

    // For JPEG, surface XMP title/creator/dates as the MediaInfo EXIF fields
    // the container parser used to derive. set_field is first-write-wins, so
    // any value already provided by EXIF takes precedence.
    let is_jpeg = fa
        .streams()
        .stream(StreamKind::General, 0)
        .and_then(|s| s.get("Format"))
        .map(|f| f.as_str() == "JPEG")
        .unwrap_or(false);
    if is_jpeg {
        if let Some(t) = xmp_alt_li(text, "dc:title").filter(|s| !s.is_empty()) {
            fa.set_field(StreamKind::Exif, 0, "Description", t);
        }
        if let Some(c) = xmp_seq_li(text, "dc:creator").filter(|s| !s.is_empty()) {
            let n = c
                .trim_end_matches(" Inc.")
                .trim_end_matches(" Corporation")
                .trim_end_matches(" CORPORATION")
                .trim()
                .to_string();
            fa.set_field(StreamKind::Exif, 0, "Encoded_Hardware_CompanyName", n);
        }
        if let Some(d) = xmp_attr(text, "xmp:CreateDate") {
            fa.set_field(StreamKind::Exif, 0, "Recorded_Date", exif_datetime_to_oracle(&d));
        }
        if let Some(d) = xmp_attr(text, "xmp:ModifyDate") {
            fa.set_field(StreamKind::Exif, 0, "Mastered_Date", exif_datetime_to_oracle(&d));
        }
    }
    true
}

/// Value of `attr="..."` in the XMP packet (attribute form).
fn xmp_attr(xml: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = xml.find(&needle)? + needle.len();
    let end = xml[start..].find('"')?;
    Some(xml[start..start + end].to_string())
}

/// First `<rdf:li ...>value</rdf:li>` inside `<tag><rdf:Alt>…` (dc:title).
fn xmp_alt_li(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = xml.find(&open)?;
    let e = xml[s..].find(&close)? + s;
    let section = &xml[s..e];
    let li = section.find("<rdf:li")?;
    let gt = section[li..].find('>')? + li;
    let end = section[gt..].find("</rdf:li>")? + gt;
    Some(section[gt + 1..end].to_string())
}

/// First `<rdf:li>value</rdf:li>` inside `<tag><rdf:Seq>…` (dc:creator).
fn xmp_seq_li(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = xml.find(&open)?;
    let e = xml[s..].find(&close)? + s;
    let section = &xml[s..e];
    let li = section.find("<rdf:li>")? + "<rdf:li>".len();
    let end = section[li..].find("</rdf:li>")? + li;
    Some(section[li..end].to_string())
}

fn extract_xmp_fields(xml: &str, tags: &mut Vec<TagEntry>) {
    let field_map = [
        ("dc:title", "Title"),
        ("dc:creator", "Creator"),
        ("dc:subject", "Subject"),
        ("dc:description", "Description"),
        ("dc:publisher", "Publisher"),
        ("dc:date", "Encoded_Date"),
        ("dc:format", "Format"),
        ("dc:language", "Language"),
        ("dc:rights", "Copyright"),
        ("xmp:CreateDate", "Encoded_Date"),
        ("xmp:ModifyDate", "Encoded_Date"),
        ("xmp:CreatorTool", "Encoded_Application"),
        ("xmp:Rating", "Rating"),
        ("tiff:Make", "Make"),
        ("tiff:Model", "Model"),
        ("tiff:ImageWidth", "Width"),
        ("tiff:ImageLength", "Height"),
        ("exif:ExposureTime", "ExposureTime"),
        ("exif:FNumber", "FNumber"),
        ("exif:ISOSpeedRatings", "ISOSpeed"),
        ("exif:FocalLength", "FocalLength"),
    ];

    for (xmp_key, mi_key) in &field_map {
        if let Some(val) = extract_xml_element(xml, xmp_key) {
            tags.push((mi_key, val));
        }
    }
}

fn extract_xml_element(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}", tag);
    let pos = xml.find(&open)?;
    let rest = &xml[pos + open.len()..];
    let close = rest.find('>')?;
    let inner = rest[close + 1..].trim();
    let end_tag = format!("</{}>", tag.split(':').next_back().unwrap_or(tag));
    let end = inner.find(&end_tag)?;
    Some(inner[..end].trim().to_string())
}

// ---------- ICC profile ----------

pub fn parse_icc(fa: &mut FileAnalyze) -> bool {
    // Read a bounded prefix cursor-independently (like `parse_exif`): the ICC
    // profile may be embedded (JPEG APP2 "ICC_PROFILE\0", HEIC `colr`, TIFF
    // tag 0x8773) rather than sitting at offset 0 of a standalone `.icc`.
    let buf = match metadata_prefix(fa) {
        Some(b) => b,
        None => return false,
    };
    // Locate the profile by its `acsp` file signature, which always sits at
    // byte 36 of an ICC header. Try offset 0 first (raw `.icc`), then scan.
    let base = match icc_profile_offset(&buf) {
        Some(b) => b,
        None => return false,
    };
    let p = &buf[base..];
    if p.len() < 132 {
        return false;
    }

    let profile_size = read_icc_u32(p, 0) as usize;
    let cmm = icc_fourcc(&p[4..8]);
    let version = read_icc_u32(p, 8);
    let device_class = icc_fourcc(&p[12..16]);
    let color_space = icc_fourcc(&p[16..20]);
    let pcs = icc_fourcc(&p[20..24]);
    let platform = icc_fourcc(&p[40..44]);
    let manufacturer = icc_fourcc(&p[48..52]);
    let model = icc_fourcc(&p[52..56]);
    let rendering_intent = read_icc_u32(p, 64);
    let creator = icc_fourcc(&p[80..84]);

    let mut tags: Vec<TagEntry> = Vec::new();
    // Summary line revelo has always emitted (kept for back-compat), now with
    // real values rather than the empty "()" from reading the wrong offset.
    tags.push(("ICC_Profile", format!("{} ({})", icc_trim(&color_space), icc_trim(&device_class))));

    // Fixed-offset header fields (ExifTool "ICC-header" group names).
    tags.push(("ProfileCMMType", icc_company(&cmm)));
    tags.push(("ProfileVersion", icc_version(version)));
    tags.push(("ProfileClass", icc_profile_class(&device_class)));
    tags.push(("ColorSpaceData", icc_trim(&color_space)));
    tags.push(("ProfileConnectionSpace", icc_trim(&pcs)));
    if let Some(dt) = icc_datetime(p) {
        tags.push(("ProfileDateTime", dt));
    }
    tags.push(("ProfileFileSignature", icc_trim(&icc_fourcc(&p[36..40]))));
    tags.push(("PrimaryPlatform", icc_company(&platform)));
    if !icc_trim(&manufacturer).is_empty() {
        tags.push(("DeviceManufacturer", icc_company(&manufacturer)));
    }
    if !icc_trim(&model).is_empty() {
        tags.push(("DeviceModel", icc_trim(&model)));
    }
    tags.push(("RenderingIntent", icc_rendering_intent(rendering_intent)));
    if !icc_trim(&creator).is_empty() {
        tags.push(("ProfileCreator", icc_company(&creator)));
    }
    let pid = &p[84..100];
    if pid.iter().any(|&b| b != 0) {
        let hex: String = pid.iter().map(|b| format!("{b:02x}")).collect();
        tags.push(("ProfileID", hex));
    }

    // Tag table — pull human-readable text/XYZ tags.
    let cap = profile_size.min(p.len());
    let tag_count = read_icc_u32(p, 128) as usize;
    let mut pos = 132;
    for _ in 0..tag_count.min(100) {
        if pos + 12 > cap {
            break;
        }
        let sig = [p[pos], p[pos + 1], p[pos + 2], p[pos + 3]];
        let off = read_icc_u32(p, pos + 4) as usize;
        let size = read_icc_u32(p, pos + 8) as usize;
        pos += 12;
        if off + size > cap || size < 8 {
            continue;
        }
        let data = &p[off..off + size];
        match &sig {
            b"desc" => {
                if let Some(s) = icc_text(data) {
                    tags.push(("ProfileDescription", s));
                }
            }
            b"cprt" => {
                if let Some(s) = icc_text(data) {
                    tags.push(("ProfileCopyright", s));
                }
            }
            b"wtpt" => {
                if let Some(s) = icc_xyz(data) {
                    tags.push(("MediaWhitePoint", s));
                }
            }
            b"rXYZ" => {
                if let Some(s) = icc_xyz(data) {
                    tags.push(("RedMatrixColumn", s));
                }
            }
            b"gXYZ" => {
                if let Some(s) = icc_xyz(data) {
                    tags.push(("GreenMatrixColumn", s));
                }
            }
            b"bXYZ" => {
                if let Some(s) = icc_xyz(data) {
                    tags.push(("BlueMatrixColumn", s));
                }
            }
            _ => {}
        }
    }

    fill_tags(fa, &tags, StreamKind::Icc);
    true
}

/// Locate the start of an ICC profile within `buf`. The 4-byte `acsp` file
/// signature always sits at byte 36 of the 128-byte ICC header, so we scan
/// for it and validate the surrounding header. Returns the byte offset of the
/// profile header (0 for a standalone `.icc`).
fn icc_profile_offset(buf: &[u8]) -> Option<usize> {
    let valid = |base: usize| -> bool {
        if base + 132 > buf.len() {
            return false;
        }
        if &buf[base + 36..base + 40] != b"acsp" {
            return false;
        }
        let size = read_icc_u32(&buf[base..], 0) as usize;
        // Header (128) + tag count word (4) minimum; cap to remaining bytes.
        size >= 132 && base + size <= buf.len() && (read_icc_u32(&buf[base..], 8) >> 24) <= 5
    };
    if valid(0) {
        return Some(0);
    }
    let mut from = 0;
    while let Some(rel) = buf[from..].windows(4).position(|w| w == b"acsp") {
        let sig_pos = from + rel;
        if sig_pos >= 36 && valid(sig_pos - 36) {
            return Some(sig_pos - 36);
        }
        from = sig_pos + 4;
    }
    None
}

/// Bytes 4..8 etc. as a 4-character code string (lossy).
fn icc_fourcc(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_string()
}

/// Trim ICC 4cc padding (trailing spaces / nulls).
fn icc_trim(s: &str) -> String {
    s.trim_end_matches([' ', '\0']).to_string()
}

/// ICC version word (BBGGRRSS where BB=major, high nibble of GG=minor) → "4.0.0".
fn icc_version(v: u32) -> String {
    let major = (v >> 24) & 0xFF;
    let minor = (v >> 20) & 0x0F;
    let bugfix = (v >> 16) & 0x0F;
    format!("{major}.{minor}.{bugfix}")
}

/// ICC profile/creation date: 6 × u16 BE at offset 24 (Y M D h m s).
fn icc_datetime(p: &[u8]) -> Option<String> {
    if p.len() < 36 {
        return None;
    }
    let r = |o: usize| u16::from_be_bytes([p[o], p[o + 1]]);
    let (y, mo, d, h, mi, s) = (r(24), r(26), r(28), r(30), r(32), r(34));
    if y == 0 && mo == 0 && d == 0 {
        return None;
    }
    Some(format!("{y:04}:{mo:02}:{d:02} {h:02}:{mi:02}:{s:02}"))
}

/// Map common ICC signature 4ccs to their company / platform names.
fn icc_company(code: &str) -> String {
    match icc_trim(code).as_str() {
        "APPL" | "appl" => "Apple Computer Inc.".to_string(),
        "MSFT" => "Microsoft Corporation".to_string(),
        "SUNW" => "Sun Microsystems".to_string(),
        "SGI" => "Silicon Graphics Inc.".to_string(),
        "TGNT" => "Taligent Inc.".to_string(),
        "ADBE" => "Adobe Systems Inc.".to_string(),
        "HP" => "Hewlett-Packard".to_string(),
        "Lino" | "LINO" => "Linotype-Hell AG".to_string(),
        "IEC" => "IEC".to_string(),
        "KCMS" => "Kodak".to_string(),
        "UCCM" => "Unicolor".to_string(),
        other if other.is_empty() => String::new(),
        other => other.to_string(),
    }
}

fn icc_profile_class(code: &str) -> String {
    match icc_trim(code).as_str() {
        "scnr" => "Input Device Profile".to_string(),
        "mntr" => "Display Device Profile".to_string(),
        "prtr" => "Output Device Profile".to_string(),
        "link" => "DeviceLink Profile".to_string(),
        "spac" => "ColorSpace Conversion Profile".to_string(),
        "abst" => "Abstract Profile".to_string(),
        "nmcl" => "NamedColor Profile".to_string(),
        other => other.to_string(),
    }
}

fn icc_rendering_intent(v: u32) -> String {
    match v {
        0 => "Perceptual".to_string(),
        1 => "Media-Relative Colorimetric".to_string(),
        2 => "Saturation".to_string(),
        3 => "ICC-Absolute Colorimetric".to_string(),
        other => other.to_string(),
    }
}

/// Extract text from an ICC `desc`/`text`/`mluc` tag.
fn icc_text(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    let s = match &data[0..4] {
        b"mluc" => {
            // multiLocalizedUnicode: count(4) recsize(4) then records of
            // lang(2) country(2) len(4) offset(4); strings are UTF-16BE.
            if data.len() < 28 {
                return None;
            }
            let len = read_icc_u32(data, 20) as usize;
            let off = read_icc_u32(data, 24) as usize;
            if off + len > data.len() || len < 2 {
                return None;
            }
            let u16s: Vec<u16> = data[off..off + len]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16s)
        }
        b"desc" => {
            // v2 textDescription: count(4) at offset 8, then ASCII.
            let n = read_icc_u32(data, 8) as usize;
            let start = 12;
            if start + n > data.len() {
                return None;
            }
            String::from_utf8_lossy(&data[start..start + n]).to_string()
        }
        b"text" => String::from_utf8_lossy(&data[8..]).to_string(),
        _ => return None,
    };
    let s = s.trim_end_matches('\0').trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Format an ICC `XYZ ` tag (one or more s15Fixed16 triples) as space-joined
/// decimals, e.g. "0.95045 1 1.08905".
fn icc_xyz(data: &[u8]) -> Option<String> {
    if data.len() < 8 + 12 || &data[0..4] != b"XYZ " {
        return None;
    }
    let v = |o: usize| -> f64 { (read_icc_u32(data, o) as i32) as f64 / 65536.0 };
    let parts: Vec<String> = (0..3).map(|i| icc_num(v(8 + i * 4))).collect();
    Some(parts.join(" "))
}

/// Trim trailing zeros from an ICC fixed-point decimal for compact display.
fn icc_num(x: f64) -> String {
    let s = format!("{x:.5}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() { "0".to_string() } else { s.to_string() }
}

fn read_icc_u32(data: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

// ---------- C2PA ----------

pub fn parse_c2pa(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = metadata_prefix(fa) else {
        return false;
    };
    if buf.len() < 16 {
        return false;
    }

    // Search for C2PA JUMBF box: "jumb" at any position
    let jumb_pos = buf.windows(4).position(|w| w == b"jumb");
    if jumb_pos.is_none() {
        return false;
    }

    let mut tags: Vec<TagEntry> = Vec::new();

    // Look for "c2pa" or "c2ma" after the jumb box
    for win in buf.windows(4) {
        if win == b"c2pa" || win == b"c2ma" || win == b"c2as" || win == b"c2cl" || win == b"c2cs" {
            let label = match win {
                b"c2pa" => "C2PA Manifest",
                b"c2ma" => "C2PA Assertion Manifest",
                b"c2as" => "C2PA Assertion Store",
                b"c2cl" => "C2PA Claim",
                b"c2cs" => "C2PA Claim Signature",
                _ => "C2PA",
            };
            tags.push(("C2PA_Format", label.to_string()));
        }
    }

    if !tags.is_empty() {
        tags.push(("C2PA_Present", "Yes".to_string()));
        fill_tags(fa, &tags, StreamKind::C2pa);
        true
    } else {
        false
    }
}

// ---------- IIM / IPTC ----------

pub fn parse_iim(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = metadata_prefix(fa) else {
        return false;
    };
    if buf.len() < 4 {
        return false;
    }

    // IPTC IIM is only well-defined inside its container — the Photoshop
    // Image Resource Block (8BIM) resource 0x0404, introduced by the
    // APP13 "Photoshop 3.0" identifier. We must NOT scan the whole file
    // for the 0x1C tag marker: compressed image data contains stray 0x1C
    // bytes that parse as bogus datasets (e.g. a garbage "Headline").
    // Anchor to the IRB so only genuine IPTC is parsed.
    let Some(iptc) = find_iptc_resource(&buf) else {
        return false;
    };
    let mut tags: Vec<TagEntry> = Vec::new();
    parse_iim_buf(iptc, &mut tags);
    if tags.is_empty() {
        return false;
    }
    fill_tags(fa, &tags, StreamKind::Iptc);
    true
}

/// Locate the IPTC-NAA payload (8BIM resource id 0x0404) inside a
/// Photoshop Image Resource Block. Returns the bytes of that resource
/// only, or `None` when no Photoshop IRB / IPTC resource is present.
fn find_iptc_resource(data: &[u8]) -> Option<&[u8]> {
    // The IRB is introduced by the APP13 identifier "Photoshop 3.0\0".
    // Requiring it keeps us from treating arbitrary "8BIM" byte runs in
    // compressed data as resource blocks.
    let anchor = b"Photoshop 3.0\0";
    let start = find_subslice(data, anchor)? + anchor.len();

    let mut pos = start;
    while pos + 4 <= data.len() {
        // Resync to the next "8BIM" signature (resource blocks are
        // contiguous, but be tolerant of a leading pad byte).
        if &data[pos..pos + 4] != b"8BIM" {
            let rel = find_subslice(&data[pos..], b"8BIM")?;
            pos += rel;
            if pos + 4 > data.len() {
                return None;
            }
        }
        pos += 4;
        if pos + 2 > data.len() {
            return None;
        }
        let res_id = u16::from_be_bytes([data[pos], data[pos + 1]]);
        pos += 2;

        // Pascal name string: 1 length byte + name, padded so the whole
        // (length + name) field is even.
        if pos >= data.len() {
            return None;
        }
        let name_len = data[pos] as usize;
        let name_field = 1 + name_len;
        pos += name_field + (name_field & 1);
        if pos + 4 > data.len() {
            return None;
        }
        let size =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + size > data.len() {
            return None;
        }
        if res_id == 0x0404 {
            return Some(&data[pos..pos + size]);
        }
        // Resource data is itself padded to an even length.
        pos += size + (size & 1);
    }
    None
}

/// First index of `needle` in `haystack`, or `None`.
fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub fn parse_iim_buf(data: &[u8], tags: &mut Vec<TagEntry>) {
    if !data.contains(&0x1C) {
        return;
    }
    let mut pos = 0;

    while pos + 2 < data.len() {
        if data[pos] != 0x1C {
            pos += 1;
            continue;
        }
        if pos + 3 > data.len() {
            break;
        }
        let record = data[pos + 1];
        let dataset = data[pos + 2];
        pos += 3;

        if pos >= data.len() {
            break;
        }
        let size = if data[pos] & 0x80 != 0 {
            if pos + 2 > data.len() {
                break;
            }
            let s = ((data[pos] as u16 & 0x7F) << 8) | data[pos + 1] as u16;
            pos += 2;
            s as usize
        } else {
            let s = data[pos] as usize;
            pos += 1;
            s
        };
        if pos + size > data.len() {
            break;
        }
        let value = String::from_utf8_lossy(&data[pos..pos + size]).trim_end().to_string();
        pos += size;

        let key = iim_tag_key(record, dataset);
        let Some(key) = key else { continue };

        if key == "Keyword" {
            if let Some(existing) = tags.iter_mut().find(|(k, _)| *k == key) {
                existing.1.push_str(" / ");
                existing.1.push_str(&value);
                continue;
            }
        }
        tags.push((key, value));
    }
}

fn iim_tag_key(record: u8, dataset: u8) -> Option<&'static str> {
    Some(match (record, dataset) {
        // Record 1: Envelope
        (1, 0) => "IIMModelVersion",
        (1, 20) => "Destination",
        (1, 30) => "IIMFileFormat",
        (1, 40) => "IIMFileVersion",
        (1, 50) => "ServiceIdentifier",
        (1, 70) => "EnvelopeNumber",
        (1, 80) => "ProductID",
        (1, 90) => "EnvelopePriority",
        (1, 100) => "DateSent",
        (1, 120) => "TimeSent",
        (1, 130) => "CodedCharacterSet",
        (1, 150) => "UniqueObjectName",
        (1, 160) => "ARMIdentifier",
        (1, 170) => "ARMVersion",

        // Record 2: Application
        (2, 3) => "ObjectTypeReference",
        (2, 4) => "ObjectAttributeReference",
        (2, 5) => "ObjectName",
        (2, 7) => "EditStatus",
        (2, 8) => "EditorialUpdate",
        (2, 10) => "Urgency",
        (2, 12) => "SubjectReference",
        (2, 15) => "Category",
        (2, 20) => "SupplementalCategories",
        (2, 22) => "FixtureIdentifier",
        (2, 25) => "Keyword",
        (2, 26) => "ContentLocationCode",
        (2, 27) => "ContentLocationName",
        (2, 30) => "ReleaseDate",
        (2, 35) => "ReleaseTime",
        (2, 37) => "ExpirationDate",
        (2, 38) => "ExpirationTime",
        (2, 40) => "SpecialInstructions",
        (2, 42) => "ActionAdvised",
        (2, 45) => "ReferenceService",
        (2, 47) => "ReferenceDate",
        (2, 50) => "ReferenceNumber",
        (2, 55) => "DateCreated",
        (2, 60) => "TimeCreated",
        (2, 62) => "DigitalCreationDate",
        (2, 63) => "DigitalCreationTime",
        (2, 65) => "OriginatingProgram",
        (2, 70) => "ProgramVersion",
        (2, 75) => "ObjectCycle",
        (2, 80) => "Byline",
        (2, 85) => "BylineTitle",
        (2, 90) => "City",
        (2, 92) => "Sublocation",
        (2, 95) => "ProvinceState",
        (2, 100) => "CountryCode",
        (2, 101) => "Country",
        (2, 103) => "OriginalTransmissionReference",
        (2, 105) => "Headline",
        (2, 110) => "Credit",
        (2, 115) => "Source",
        (2, 116) => "Copyright",
        (2, 118) => "Contact",
        (2, 120) => "Description",
        (2, 122) => "DescriptionWriter",

        _ => return None,
    })
}

// ---------- PropertyList (Apple plist) ----------

pub fn parse_property_list(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = metadata_prefix(fa) else {
        return false;
    };
    if find_subslice(buf, b"<!DOCTYPE plist").is_none() && find_subslice(buf, b"<plist").is_none() {
        return false;
    }
    let text = std::str::from_utf8(&buf).unwrap_or("");

    let mut tags: Vec<TagEntry> = Vec::new();
    extract_plist_fields(text, &mut tags);
    fill_tags(fa, &tags, StreamKind::General);
    true
}

fn extract_plist_fields(xml: &str, tags: &mut Vec<TagEntry>) {
    let keys = [
        ("director", "Director"),
        ("producer", "Producer"),
        ("screenwriter", "ScreenplayBy"),
        ("studio", "ProductionStudio"),
        ("cast", "Actor"),
        ("genre", "Genre"),
        ("copyright", "Copyright"),
        ("title", "Title"),
        ("artist", "Performer"),
        ("album", "Album"),
    ];

    for (plist_key, mi_key) in &keys {
        if let Some(val) = extract_plist_value(xml, plist_key) {
            tags.push((mi_key, val));
        }
    }
}

fn extract_plist_value(xml: &str, key: &str) -> Option<String> {
    let key_tag = format!("<key>{}</key>", key);
    let pos = xml.find(&key_tag)?;
    let rest = &xml[pos + key_tag.len()..];
    let after_key = rest.trim_start();

    if let Some(inner) = after_key.strip_prefix("<string>") {
        let end = inner.find("</string>")?;
        Some(inner[..end].to_string())
    } else if let Some(inner) = after_key.strip_prefix("<integer>") {
        let end = inner.find("</integer>")?;
        Some(inner[..end].to_string())
    } else if let Some(inner) = after_key.strip_prefix("<array>") {
        // Collect array elements as comma-separated
        let arr_end = inner.find("</array>")?;
        let arr = &inner[..arr_end];
        let mut items = Vec::new();
        let mut search_pos = 0;
        while let Some(s_start) = arr[search_pos..].find("<string>") {
            let s = &arr[search_pos + s_start + 8..];
            if let Some(s_end) = s.find("</string>") {
                items.push(s[..s_end].to_string());
                search_pos += s_start + 8 + s_end + 9;
            } else {
                break;
            }
        }
        if !items.is_empty() { Some(items.join(", ")) } else { None }
    } else {
        None
    }
}

// ---------- SphericalVideo ----------

pub fn parse_spherical_video(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = metadata_prefix(fa) else {
        return false;
    };
    if find_subslice(buf, b"SphericalVideo").is_none()
        && find_subslice(buf, b"ProjectionType").is_none()
    {
        return false;
    }
    let text = std::str::from_utf8(&buf).unwrap_or("");

    let mut tags: Vec<TagEntry> = Vec::new();

    if let Some(pt) = extract_xml_element(text, "ProjectionType") {
        tags.push(("Projection", pt));
    }
    if let Some(sm) = extract_xml_element(text, "StereoMode") {
        tags.push(("StereoMode", sm));
    }
    if let Some(cw) = extract_xml_element(text, "CroppedAreaImageWidthPixels") {
        tags.push(("CroppedAreaImageWidthPixels", cw));
    }
    if let Some(ch) = extract_xml_element(text, "CroppedAreaImageHeightPixels") {
        tags.push(("CroppedAreaImageHeightPixels", ch));
    }
    if let Some(fw) = extract_xml_element(text, "FullPanoWidthPixels") {
        tags.push(("FullPanoWidthPixels", fw));
    }
    if let Some(fh) = extract_xml_element(text, "FullPanoHeightPixels") {
        tags.push(("FullPanoHeightPixels", fh));
    }

    if !tags.is_empty() {
        tags.push(("SphericalVideo", "Yes".to_string()));
        fill_tags(fa, &tags, StreamKind::General);
        true
    } else {
        false
    }
}

// ---------- PNG text chunks ----------

fn png_key_to_field(key: &str) -> Option<&'static str> {
    Some(match key {
        "Title" => "Title",
        "Author" => "Performer",
        "Description" => "Description",
        "Copyright" => "Copyright",
        "Creation Time" => "Recorded_Date",
        "Comment" => "Comment",
        "Software" => "Encoded_Application",
        "Disclaimer" => "Disclaimer",
        "Warning" => "Warning",
        "Source" => "Source",
        _ => return None,
    })
}

pub fn parse_png_text(fa: &mut FileAnalyze) -> bool {
    let buf = match metadata_prefix(fa) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 8 || &buf[0..4] != b"\x89PNG" {
        return false;
    }
    let mut i = 8usize;
    let mut tags: Vec<TagEntry> = Vec::new();
    while i + 12 <= buf.len() {
        let len = u32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]) as usize;
        let ty = &buf[i + 4..i + 8];
        let payload_off = i + 8;
        i = payload_off + len + 4;
        if i > buf.len() {
            break;
        }
        if ty == b"IEND" {
            break;
        }
        if ty != b"tEXt" && ty != b"iTXt" && ty != b"zTXt" {
            continue;
        }
        if len == 0 {
            continue;
        }
        let payload = &buf[payload_off..payload_off + len];
        let nul = match payload.iter().position(|&b| b == 0) {
            Some(n) => n,
            None => continue,
        };
        let key = String::from_utf8_lossy(&payload[..nul]).into_owned();
        if key.is_empty() {
            continue;
        }
        let field = match png_key_to_field(&key) {
            Some(f) => f,
            None => continue,
        };

        if ty == b"tEXt" {
            let val =
                String::from_utf8_lossy(&payload[nul + 1..]).trim_matches('\0').trim().to_string();
            if !val.is_empty() {
                tags.push((field, val));
            }
        } else if ty == b"iTXt" {
            let rest = &payload[nul + 1..];
            let mut pos = 0;
            for _ in 0..3 {
                if let Some(n) = rest[pos..].iter().position(|&b| b == 0) {
                    pos += n + 1;
                } else {
                    break;
                }
            }
            if let Some(n) = rest[pos..].iter().position(|&b| b == 0) {
                pos += n + 1;
                let val =
                    String::from_utf8_lossy(&rest[pos..]).trim_matches('\0').trim().to_string();
                if !val.is_empty() {
                    tags.push((field, val));
                }
            }
        } else if ty == b"zTXt" {
            let rest = &payload[nul + 1..];
            if rest.len() < 2 {
                continue;
            }
            let comp_method = rest[0];
            if comp_method != 0 {
                continue;
            }
            let compressed = &rest[1..];
            if compressed.is_empty() {
                continue;
            }
            let mut decoder = flate2::read::DeflateDecoder::new(compressed);
            let mut val = String::new();
            if decoder.read_to_string(&mut val).is_ok() {
                let val = val.trim_matches('\0').trim().to_string();
                if !val.is_empty() {
                    tags.push((field, val));
                }
            }
        }
    }
    if !tags.is_empty() {
        fill_tags(fa, &tags, StreamKind::General);
        true
    } else {
        false
    }
}

// ---------- JPEG COM ----------

pub fn parse_jpeg_com(fa: &mut FileAnalyze) -> bool {
    let buf = match metadata_prefix(fa) {
        Some(b) if b.len() >= 4 => b,
        None => return false,
        Some(_) => return false,
    };
    if buf.len() < 4 || buf[0] != 0xFF || buf[1] != 0xD8 {
        return false;
    }

    let mut tags: Vec<TagEntry> = Vec::new();
    let mut i = 2;
    while i + 3 < buf.len() {
        if buf[i] != 0xFF {
            break;
        }
        let marker = buf[i + 1];
        if marker == 0xDA {
            break;
        } // SOS — no more metadata
        if marker == 0xD9 {
            break;
        } // EOI
        let seg_len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        if seg_len < 2 {
            break;
        }
        let data_start = i + 4;
        let data_end = data_start + seg_len - 2;
        if data_end > buf.len() {
            break;
        }

        if marker == 0xFE {
            let comment = String::from_utf8_lossy(&buf[data_start..data_end])
                .trim_matches('\0')
                .trim()
                .to_string();
            if !comment.is_empty() {
                tags.push(("Comment", comment));
            }
        }
        i = data_end;
    }

    if !tags.is_empty() {
        fill_tags(fa, &tags, StreamKind::General);
        true
    } else {
        false
    }
}

// ---------- Update parse_tags ----------

fn general_format<'fa, 'src>(fa: &'fa FileAnalyze<'src>) -> Option<&'fa str> {
    fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str())
}

fn should_scan_tail_audio_tags(fa: &FileAnalyze) -> bool {
    let Some(format) = general_format(fa) else {
        return true;
    };
    if fa.stream_count(StreamKind::Video) > 0 || fa.stream_count(StreamKind::Image) > 0 {
        return false;
    }
    fa.stream_count(StreamKind::Audio) > 0
        || matches!(
            format,
            "MPEG Audio"
                | "Monkey's Audio"
                | "Musepack"
                | "WavPack"
                | "True Audio"
                | "FLAC"
                | "Ogg"
        )
}

fn should_scan_embedded_metadata(fa: &FileAnalyze) -> bool {
    let Some(format) = general_format(fa) else {
        return true;
    };
    if fa.stream_count(StreamKind::Image) > 0
        || fa.stream_count(StreamKind::Exif) > 0
        || fa.stream_count(StreamKind::Iptc) > 0
        || fa.stream_count(StreamKind::Xmp) > 0
        || fa.stream_count(StreamKind::Icc) > 0
        || fa.stream_count(StreamKind::C2pa) > 0
        || fa.stream_count(StreamKind::MakerNotes) > 0
    {
        return true;
    }

    matches!(
        format,
        "JPEG"
            | "PNG"
            | "TIFF"
            | "JPEG 2000"
            | "HEIF"
            | "WebP"
            | "BMP"
            | "GIF"
            | "PSD"
            | "DPX"
            | "DDS"
            | "OpenEXR"
            | "BPG"
            | "PCX"
            | "ARRIRAW"
            | "TGA"
            | "Gain Map"
    )
}

pub fn parse_tags(fa: &mut FileAnalyze) -> bool {
    let _ = parse_id3v1(fa);
    let _ = parse_id3v2(fa);
    if should_scan_tail_audio_tags(fa) {
        let _ = parse_ape_tag(fa);
        let _ = parse_lyrics3(fa);
    }
    if should_scan_embedded_metadata(fa) {
        let _ = parse_exif(fa);
        let _ = parse_xmp(fa);
        let _ = parse_icc(fa);
        let _ = parse_c2pa(fa);
        let _ = parse_iim(fa);
        let _ = parse_property_list(fa);
        let _ = parse_spherical_video(fa);
        let _ = parse_jpeg_com(fa);
        let _ = parse_png_text(fa);
    }
    infer_raw_format(fa);
    true
}

// ---------- MakerNotes ----------

type TagNameFn = fn(u16) -> Option<&'static str>;

fn parse_tiff_header_or_raw<'a>(data: &'a [u8], default_bo: &'a str) -> (&'a str, usize) {
    if data.len() >= 8 && (&data[0..2] == b"II" || &data[0..2] == b"MM") {
        let bo = if &data[0..2] == b"II" { "LE" } else { "BE" };
        (bo, read_tiff_u32(data, 4, bo) as usize)
    } else {
        (default_bo, 0)
    }
}

/// Maker-note vendors that share the generic [`parse_makernote_ifd`]
/// walker. Carries enough identity to consult the richer ExifTool-derived
/// tables when the `exiftool-tables` feature is enabled; otherwise it is
/// only used to keep the call sites uniform.
#[derive(Copy, Clone)]
#[allow(dead_code)] // some variants only matter under `exiftool-tables`
enum MakerVendor {
    Samsung,
    Apple,
    GoPro,
    Dji,
    Google,
    Leica,
    Sigma,
    Minolta,
    Casio,
    Flir,
    Olympus,
    Panasonic,
    Pentax,
    Fujifilm,
}

#[cfg(feature = "exiftool-tables")]
impl MakerVendor {
    /// Map to the GPL ExifTool table vendor, or `None` when no bundled
    /// table exists (GoPro's GPMF, Pixel/Google, Leica's multi-format
    /// maker notes) — those keep the hand-written clean-room tables.
    fn exiftool(self) -> Option<revelo_exiftool_tables::Vendor> {
        use revelo_exiftool_tables::Vendor as V;
        Some(match self {
            MakerVendor::Samsung => V::Samsung,
            MakerVendor::Apple => V::Apple,
            MakerVendor::Dji => V::Dji,
            MakerVendor::Sigma => V::Sigma,
            MakerVendor::Minolta => V::Minolta,
            MakerVendor::Casio => V::Casio,
            MakerVendor::Flir => V::Flir,
            MakerVendor::Olympus => V::Olympus,
            MakerVendor::Panasonic => V::Panasonic,
            MakerVendor::Pentax => V::Pentax,
            MakerVendor::Fujifilm => V::Fujifilm,
            MakerVendor::GoPro | MakerVendor::Google | MakerVendor::Leica => return None,
        })
    }
}

/// Tag-name lookup: prefer the ExifTool table (richer) when the feature
/// is on and a bundled table exists, else fall back to the hand-written
/// clean-room table.
fn resolve_tag_name(vendor: MakerVendor, hand: TagNameFn, id: u16) -> Option<&'static str> {
    #[cfg(feature = "exiftool-tables")]
    if let Some(v) = vendor.exiftool() {
        if let Some(name) = revelo_exiftool_tables::tag_name(v, id as u32) {
            return Some(name);
        }
    }
    let _ = vendor;
    hand(id)
}

/// Value decoding: when the feature is on and the raw value is an integer
/// with an ExifTool PrintConv mapping, return the decoded string; else the
/// raw value is returned unchanged.
fn apply_print_conv(vendor: MakerVendor, id: u16, raw: String) -> String {
    #[cfg(feature = "exiftool-tables")]
    if let Some(v) = vendor.exiftool() {
        if let Ok(n) = raw.trim().parse::<i64>() {
            if let Some(decoded) = revelo_exiftool_tables::print_conv(v, id as u32, n) {
                return decoded.to_string();
            }
        }
    }
    let _ = (vendor, id);
    raw
}

fn parse_makernote_ifd(
    data: &[u8],
    offset: usize,
    bo: &str,
    tags: &mut Vec<TagEntry>,
    tag_name: TagNameFn,
    vendor: MakerVendor,
    base: usize,
) {
    if offset + 2 > data.len() {
        return;
    }
    let count = read_tiff_u16(data, offset, bo) as usize;
    if count > 100 {
        return;
    }
    let mut pos = offset + 2;

    for _ in 0..count.min(100) {
        if pos + 12 > data.len() {
            break;
        }
        let tag_id = read_tiff_u16(data, pos, bo);
        let tag_type = read_tiff_u16(data, pos + 2, bo);
        let tag_count = read_tiff_u32(data, pos + 4, bo) as usize;
        let tag_size = exif_type_size(tag_type) * tag_count;
        pos += 8;

        let value = if tag_size <= 4 {
            if tag_type == 2 {
                let end = data[pos..pos + tag_count.min(4)]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count.min(4));
                String::from_utf8_lossy(&data[pos..pos + end]).to_string()
            } else {
                read_exif_val(data, pos, tag_type, tag_count, bo)
            }
        } else {
            let mut val_off = read_tiff_u32(data, pos, bo) as usize;
            if val_off + tag_size > data.len() && val_off >= base {
                val_off -= base;
            }
            if val_off + tag_size > data.len() {
                pos += 4;
                continue;
            }
            if tag_type == 2 {
                let end = data[val_off..val_off + tag_count]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count);
                String::from_utf8_lossy(&data[val_off..val_off + end]).to_string()
            } else {
                read_exif_val(data, val_off, tag_type, tag_count, bo)
            }
        };
        pos += 4;

        if let Some(name) = resolve_tag_name(vendor, tag_name, tag_id) {
            tags.push((name, apply_print_conv(vendor, tag_id, value)));
        }
    }
}

/// `base` is the offset of `data` within the enclosing TIFF block; only
/// Canon (whose value offsets are TIFF-relative) needs it.
fn parse_makernote(make: &str, data: &[u8], base: usize, tags: &mut Vec<TagEntry>) {
    let make_upper = make.to_uppercase();
    if make_upper.contains("CANON") {
        parse_canon_makernote(data, base, tags);
    } else if make_upper.contains("NIKON") {
        parse_nikon_makernote(data, tags);
    } else if make_upper.contains("SONY") {
        parse_sony_makernote(data, tags);
    } else if make_upper.contains("OLYMPUS")
        || make_upper.contains("OM DIGITAL")
        || make_upper.contains("OM SYSTEM")
    {
        parse_olympus_makernote(data, tags);
    } else if make_upper.contains("PANASONIC") {
        parse_panasonic_makernote(data, base, tags);
    } else if make_upper.contains("PENTAX") || make_upper.contains("RICOH") {
        parse_pentax_makernote(data, tags);
    } else if make_upper.contains("FUJIFILM") || make_upper.contains("FUJI") {
        parse_fujifilm_makernote(data, tags);
    } else if make_upper.contains("SAMSUNG") {
        parse_samsung_makernote(data, tags);
    } else if make_upper.contains("APPLE") {
        parse_apple_makernote(data, tags);
    } else if make_upper.contains("GOPRO") {
        parse_gopro_makernote(data, tags);
    } else if make_upper.contains("DJI") {
        parse_dji_makernote(data, tags);
    } else if make_upper.contains("GOOGLE") || make_upper.contains("PIXEL") {
        parse_google_makernote(data, tags);
    } else if make_upper.contains("LEICA") {
        parse_leica_makernote(data, tags);
    } else if make_upper.contains("SIGMA") || make_upper.contains("FOVEON") {
        parse_sigma_makernote(data, tags);
    } else if make_upper.contains("MINOLTA") || make_upper.contains("KONICA") {
        parse_minolta_makernote(data, base, tags);
    } else if make_upper.contains("CASIO") {
        parse_casio_makernote(data, tags);
    } else if make_upper.contains("FLIR") {
        parse_flir_makernote(data, tags);
    }
}

// ---------- Samsung MakerNote ----------

fn samsung_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "SamsungVersion",
        0x0020 => "SamsungColorSpace",
        0x0021 => "SamsungSmartColor",
        0x0022 => "SamsungPictureMode",
        0x0023 => "SamsungExposureTime",
        0x0024 => "SamsungFNumber",
        0x0025 => "SamsungISO",
        0x0032 => "SamsungDRange",
        0x0035 => "SamsungLensType",
        0x0036 => "SamsungLensFirmware",
        0x0043 => "SamsungRawFormat",
        0x0044 => "SamsungRawTone",
        0x0050 => "SamsungColorMatrix",
        0x00A0 => "SamsungCameraTemperature",
        0x0100 => "SamsungFaceDetect",
        0x0120 => "SamsungFaceInfo",
        0xA001 => "SamsungPreviewImage",
        _ => return None,
    })
}

fn parse_samsung_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Samsung MakerNote starts with "Samsung   " (8 bytes) or "Samsung2 " (8 bytes)
    let offset = if data.len() >= 8 && &data[0..7] == b"Samsung" { 8 } else { 0 };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, samsung_tag_name, MakerVendor::Samsung, 0);
}

// ---------- Apple MakerNote ----------

/// Clean-room Apple maker-note tag names. IDs and meanings were derived from
/// the on-disk IFD of real iPhone images cross-referenced against ExifTool's
/// *printed output* (the human field labels), not its source tables. Binary
/// plist tags (RunTime, AccelerationVector list payloads) are intentionally
/// left unmapped so the walker never dumps raw plist bytes as a string.
fn apple_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "MakerNoteVersion",
        0x0004 => "AEStable",
        0x0005 => "AETarget",
        0x0006 => "AEAverage",
        0x0007 => "AFStable",
        0x0008 => "AccelerationVector",
        0x000c => "FocusDistanceRange",
        0x0011 => "ContentIdentifier",
        0x0014 => "ImageCaptureType",
        0x0017 => "LivePhotoVideoIndex",
        0x001f => "PhotosAppFeatureFlags",
        0x0021 => "HDRHeadroom",
        0x0027 => "SignalToNoiseRatio",
        0x002b => "PhotoIdentifier",
        0x002d => "ColorTemperature",
        0x002e => "CameraType",
        0x002f => "FocusPosition",
        _ => return None,
    })
}

fn parse_apple_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Apple maker notes begin with "Apple iOS\0" + a 2-byte version + a 2-byte
    // TIFF byte-order mark; the IFD starts at offset 14 and value offsets are
    // relative to the start of this maker-note block (base 0).
    if data.len() >= 14 && &data[0..10] == b"Apple iOS\0" {
        let bo = if &data[12..14] == b"II" { "LE" } else { "BE" };
        parse_makernote_ifd(data, 14, bo, tags, apple_tag_name, MakerVendor::Apple, 0);
        return;
    }
    // Fallback: a plain TIFF-style maker note (II/MM at offset 0).
    if data.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(data, "LE");
    parse_makernote_ifd(data, ifd_off, bo, tags, apple_tag_name, MakerVendor::Apple, 0);
}

/// Apply Apple-specific PrintConv to maker-note values already parsed into
/// `tags` (matched by field name). Enum/boolean codes become words and signed
/// rationals become decimals — ExifTool renders SRATIONAL signed, but the
/// generic decoder leaves them unsigned, so sign is recovered here.
fn format_apple_makernote(tags: &mut [TagEntry]) {
    for (k, v) in tags.iter_mut() {
        let nv = match *k {
            "AEStable" | "AFStable" => match v.as_str() {
                "0" => Some("No".to_string()),
                "1" => Some("Yes".to_string()),
                _ => None,
            },
            "ImageCaptureType" => match v.as_str() {
                "1" => Some("ProRAW".to_string()),
                "2" => Some("Portrait".to_string()),
                "10" => Some("Photo".to_string()),
                "11" => Some("Manual Focus".to_string()),
                "12" => Some("Scene".to_string()),
                _ => None,
            },
            "CameraType" => match v.as_str() {
                "0" => Some("Back Wide Angle".to_string()),
                "1" => Some("Back Normal".to_string()),
                "2" => Some("Back Telephoto".to_string()),
                "3" => Some("Front".to_string()),
                _ => None,
            },
            "AccelerationVector" => {
                let parts: Vec<String> =
                    v.split(',').filter_map(|p| apple_srational(p).map(apple_decimal)).collect();
                (parts.len() == 3).then(|| parts.join(" "))
            }
            "FocusDistanceRange" => {
                let mut ds: Vec<f64> = v.split(',').filter_map(apple_srational).collect();
                if ds.len() == 2 {
                    ds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    Some(format!("{:.2} - {:.2} m", ds[0], ds[1]))
                } else {
                    None
                }
            }
            "HDRHeadroom" | "SignalToNoiseRatio" => apple_srational(v).map(apple_decimal),
            _ => None,
        };
        if let Some(s) = nv {
            *v = s;
        }
    }
}

/// Parse an "n/d" rational, reinterpreting each 32-bit field as signed (ExifTool
/// SRATIONAL semantics) so negative values round-trip correctly.
fn apple_srational(s: &str) -> Option<f64> {
    let (n, d) = s.trim().split_once('/').unwrap_or((s.trim(), "1"));
    let signed = |x: i64| if x > i32::MAX as i64 { x - (1i64 << 32) } else { x };
    let num = signed(n.trim().parse::<i64>().ok()?);
    let den = signed(d.trim().parse::<i64>().ok()?);
    if den == 0 {
        return None;
    }
    Some(num as f64 / den as f64)
}

/// Format a float with up to 9 decimals, trimming trailing zeros.
fn apple_decimal(x: f64) -> String {
    let s = format!("{x:.9}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" { "0".to_string() } else { s.to_string() }
}

// ---------- GoPro MakerNote ----------

fn gopro_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "GoProModelName",
        0x0002 => "GoProFirmware",
        0x0003 => "GoProSerial",
        0x0004 => "GoProBatteryLevel",
        0x0005 => "GoProMode",
        0x0006 => "GoProTimeLapse",
        0x0007 => "GoProBurstRate",
        0x0008 => "GoProFieldOfView",
        0x0009 => "GoProCameraOrientation",
        0x000A => "GoProColorMode",
        0x000B => "GoProExposureMode",
        0x000C => "GoProFrameRate",
        0x000D => "GoProResolution",
        0x000E => "GoProPhotoISO",
        0x000F => "GoProVideoISO",
        0x0010 => "GoProWhiteBalance",
        _ => return None,
    })
}

fn parse_gopro_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // GoPro MakerNote: "GoPro " (5 bytes) + TIFF IFD
    let offset = if data.len() >= 6 && &data[0..5] == b"GoPro" { 6 } else { 0 };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, gopro_tag_name, MakerVendor::GoPro, 0);
}

// ---------- DJI MakerNote ----------

fn dji_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "DJIMake",
        0x0002 => "DJIModel",
        0x0003 => "DJIFirmware",
        0x0004 => "DJIOrientation",
        0x0005 => "DJIFlightSpeed",
        0x0006 => "DJIFlightAltitude",
        0x0007 => "DJIGpsLatitude",
        0x0008 => "DJIGpsLongitude",
        0x0009 => "DJIGpsAltitude",
        0x000A => "DJIBatteryLevel",
        0x000B => "DJICameraTemperature",
        0x000C => "DJILensFocusDistance",
        0x000D => "DJIRollAngle",
        0x000E => "DJIPitchAngle",
        0x000F => "DJIYawAngle",
        0x0010 => "DJIImageWidth",
        0x0011 => "DJIImageHeight",
        0x0012 => "DJIAppTimestamp",
        _ => return None,
    })
}

fn parse_dji_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // DJI MakerNote: "DJI " (4 bytes) + TIFF IFD
    let offset = if data.len() >= 4 && &data[0..3] == b"DJI" { 4 } else { 0 };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, dji_tag_name, MakerVendor::Dji, 0);
}

// ---------- Google / Pixel MakerNote ----------

fn google_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "GoogleMotionPhoto",
        0x0002 => "GoogleDepthMap",
        0x0003 => "GoogleFocusDistance",
        0x0004 => "GoogleAperture",
        0x0005 => "GoogleFocalLength35mm",
        0x0006 => "GoogleHdrPlus",
        0x0007 => "GoogleMergedFrames",
        0x0008 => "GoogleFaceCount",
        _ => return None,
    })
}

fn parse_google_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Google/Pixel MakerNote is a plain TIFF IFD
    if data.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(data, "LE");
    parse_makernote_ifd(data, ifd_off, bo, tags, google_tag_name, MakerVendor::Google, 0);
}

// ---------- Leica MakerNote ----------

fn leica_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "LeicaQuality",
        0x0002 => "LeicaUserProfile",
        0x0003 => "LeicaSerial",
        0x0004 => "LeicaLensType",
        0x0005 => "LeicaExternalSensor",
        0x0007 => "LeicaFirmware",
        0x0008 => "LeicaISO",
        0x000A => "LeicaColorMode",
        0x000C => "LeicaWB",
        0x000F => "LeicaLensSerial",
        _ => return None,
    })
}

fn parse_leica_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Leica MakerNote: "LEICA   " (7 bytes) + TIFF IFD, or bare TIFF
    let offset = if data.len() >= 7 && &data[0..7] == b"LEICA   " {
        7
    } else if data.len() >= 6 && &data[0..5] == b"LEICA" {
        6
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "BE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, leica_tag_name, MakerVendor::Leica, 0);
}

// ---------- Sigma / Foveon MakerNote ----------

fn sigma_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0002 => "SigmaSerial",
        0x0003 => "SigmaFirmware",
        0x0004 => "SigmaImageProperties",
        0x0005 => "SigmaSensorCalibration",
        0x0006 => "SigmaIsoRating",
        0x0007 => "SigmaExposureMode",
        0x0008 => "SigmaMeterMode",
        0x0009 => "SigmaDriveMode",
        0x000A => "SigmaLensApertureRange",
        0x000B => "SigmaFocusSetting",
        0x000C => "SigmaAFMode",
        0x000D => "SigmaColorSpace",
        0x000E => "SigmaColorMode",
        0x000F => "SigmaResolutionMode",
        0x0010 => "SigmaWhiteBalance",
        0x0011 => "SigmaSharpness",
        0x0012 => "SigmaContrast",
        0x0013 => "SigmaSaturation",
        0x0014 => "SigmaColorAdjustment",
        0x0015 => "SigmaSceneMode",
        _ => return None,
    })
}

fn parse_sigma_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Sigma/Foveon MakerNote: "SIGMA   " (7 bytes) + TIFF IFD
    let offset = if data.len() >= 7 && &data[0..7] == b"SIGMA   " { 7 } else { 0 };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, sigma_tag_name, MakerVendor::Sigma, 0);
}

// ---------- Minolta / Konica Minolta MakerNote ----------

fn minolta_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "MinoltaCameraSettings",
        0x0003 => "MinoltaCameraSettings2",
        0x0004 => "MinoltaImageStabilization",
        0x0005 => "MinoltaZoneMatching",
        0x0006 => "MinoltaColorMode",
        0x0007 => "MinoltaColorFilter",
        0x0008 => "MinoltaBWFilter",
        0x0009 => "MinoltaHueAdjustment",
        0x000A => "MinoltaSaturation",
        0x000B => "MinoltaSharpness",
        0x000C => "MinoltaContrast",
        0x000D => "MinoltaSceneMode",
        0x000E => "MinoltaISO",
        0x000F => "MinoltaExposureCompensation",
        0x0010 => "MinoltaFlashMode",
        0x0011 => "MinoltaFocusMode",
        0x0012 => "MinoltaMeterMode",
        0x0013 => "MinoltaWhiteBalance",
        0x0014 => "MinoltaColorTemperature",
        0x0015 => "MinoltaLensType",
        _ => return None,
    })
}

fn parse_minolta_makernote(data: &[u8], base: usize, tags: &mut Vec<TagEntry>) {
    // Minolta/Konica-Minolta MakerNote: "MINOLTA    " (10 bytes) or "MLT0" (4 bytes) + TIFF IFD
    let offset = if data.len() >= 10 && &data[0..10] == b"MINOLTA    " {
        10
    } else if data.len() >= 4 && &data[0..4] == b"MLT0" {
        4
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (mut bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    // Some Konica-Minolta bodies store a header-less big-endian IFD. When
    // there's no TIFF byte-order mark, pick the order that yields a sane
    // entry count (the little-endian read would be a huge bogus number).
    if ifd_off == 0 && &sub[0..2] != b"II" && &sub[0..2] != b"MM" {
        let le = read_tiff_u16(sub, 0, "LE") as usize;
        let be = read_tiff_u16(sub, 0, "BE") as usize;
        if le > 100 && be <= 100 {
            bo = "BE";
        }
    }
    // Minolta value offsets are relative to the enclosing TIFF; the base
    // for `sub` is the maker-note base plus the stripped header length.
    parse_makernote_ifd(
        sub,
        ifd_off,
        bo,
        tags,
        minolta_tag_name,
        MakerVendor::Minolta,
        base + offset,
    );
}

// ---------- Casio MakerNote ----------

fn casio_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "CasioRecordingMode",
        0x0002 => "CasioQuality",
        0x0003 => "CasioFocusMode",
        0x0004 => "CasioFlashMode",
        0x0005 => "CasioFlashIntensity",
        0x0006 => "CasioObjectDistance",
        0x0007 => "CasioWhiteBalance",
        0x000A => "CasioSharpness",
        0x000B => "CasioContrast",
        0x000C => "CasioSaturation",
        0x000D => "CasioISO",
        0x000E => "CasioColorMode",
        0x000F => "CasioEnhancement",
        0x0010 => "CasioFilter",
        0x0020 => "CasioDigitalZoom",
        0x0021 => "CasioSceneMode",
        0x0022 => "CasioBracketSequence",
        0x0023 => "CasioBeep",
        0x0024 => "CasioGrid",
        0x0025 => "CasioTimeLapse",
        0x0026 => "CasioIntervalLength",
        _ => return None,
    })
}

fn parse_casio_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Casio MakerNote: "QVC      " (9 bytes) for Type 2, or 0-offset TIFF for Type 1
    let offset = if data.len() >= 9 && &data[0..3] == b"QVC" {
        9
    } else if data.len() >= 8 && &data[0..2] == b"  " {
        2
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, casio_tag_name, MakerVendor::Casio, 0);
}

// ---------- FLIR MakerNote ----------

fn flir_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "FlirRawThermalImage",
        0x0002 => "FlirRawTemperature",
        0x0003 => "FlirEmissivity",
        0x0004 => "FlirAtmosphericTemperature",
        0x0005 => "FlirReflectedTemperature",
        0x0006 => "FlirDistance",
        0x0007 => "FlirRelativeHumidity",
        0x0008 => "FlirPlanckR1",
        0x0009 => "FlirPlanckB",
        0x000A => "FlirPlanckF",
        0x000B => "FlirPlanckO",
        0x000C => "FlirAtomosphericTransX",
        0x000D => "FlirPlanckR2",
        _ => return None,
    })
}

fn parse_flir_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // FLIR MakerNote: typically a plain TIFF IFD or starts with "FLIR "
    let offset = if data.len() >= 5 && &data[0..4] == b"FLIR" { 5 } else { 0 };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, flir_tag_name, MakerVendor::Flir, 0);
}

fn canon_format_value(tag_id: u16, raw: &str) -> String {
    match tag_id {
        // CanonModelID is tag 0x0010 (0x000C is the serial number).
        0x0010 => {
            let id: u32 = raw.parse().unwrap_or(u32::MAX);
            canon_model_name(id).unwrap_or(raw).to_string()
        }
        // ImageUniqueID: 16 bytes rendered as lowercase hex (no separators).
        0x0028 => {
            let hex: String = raw
                .split(", ")
                .filter_map(|b| b.parse::<u8>().ok())
                .map(|b| format!("{b:02x}"))
                .collect();
            if hex.len() == 32 { hex } else { raw.to_string() }
        }
        0x00A2 => {
            let id: u16 = raw.parse().unwrap_or(u16::MAX);
            canon_wb_name(id).unwrap_or(raw).to_string()
        }
        0x00A5 => match raw {
            "1" => "sRGB".into(),
            "2" => "Adobe RGB".into(),
            _ => raw.to_string(),
        },
        0x00A9 => {
            let id: u16 = raw.parse().unwrap_or(u16::MAX);
            canon_picture_style_name(id).unwrap_or(raw).to_string()
        }
        0x0104 => {
            let id: u16 = raw.parse().unwrap_or(u16::MAX);
            canon_quality_name(id).unwrap_or(raw).to_string()
        }
        _ => raw.to_string(),
    }
}

fn canon_model_name(id: u32) -> Option<&'static str> {
    Some(match id {
        0x1010000 => "PowerShot A460",
        0x1040000 => "PowerShot A560",
        0x1070000 => "PowerShot SD750",
        0x1160000 => "PowerShot A590 IS",
        0x1180000 => "PowerShot A1000 IS",
        0x1220000 => "PowerShot G10",
        0x1250000 => "PowerShot SX110 IS",
        0x1260000 => "PowerShot SX10 IS",
        0x1290000 => "PowerShot SX1 IS",
        0x1300000 => "PowerShot SD990 IS",
        0x1310000 => "PowerShot SD880 IS",
        0x1340000 => "PowerShot A1100 IS",
        0x1350000 => "PowerShot SD780 IS",
        0x1360000 => "PowerShot A480",
        0x1370000 => "PowerShot SX200 IS",
        0x1380000 => "PowerShot SD960 IS",
        0x1390000 => "PowerShot SD970 IS",
        0x1400000 => "PowerShot S90",
        0x1410000 => "PowerShot G11",
        0x1440000 => "PowerShot SX20 IS",
        0x1460000 => "PowerShot SD3500 IS",
        0x1470000 => "PowerShot SD1400 IS",
        0x1480000 => "PowerShot ELPH 300 HS",
        0x1490000 => "PowerShot S95",
        0x1500000 => "PowerShot G12",
        0x1510000 => "PowerShot SX30 IS",
        0x1520000 => "PowerShot SX220 HS",
        0x1540000 => "PowerShot S100",
        0x1570000 => "PowerShot G1 X",
        0x1580000 => "PowerShot SX260 HS",
        0x1590000 => "PowerShot SX240 HS",
        0x1600000 => "PowerShot ELPH 530 HS",
        0x1620000 => "PowerShot G15",
        0x1640000 => "PowerShot SX280 HS",
        0x1660000 => "PowerShot S120",
        0x1670000 => "EOS 5D Mark III",
        0x1680000 => "EOS-1D X",
        0x1690000 => "EOS 6D",
        0x1700000 => "EOS 7D",
        0x1730000 => "EOS 70D",
        0x1740000 => "EOS 1200D",
        0x1750000 => "EOS 100D",
        0x1760000 => "EOS M",
        0x1770000 => "EOS M2",
        0x1790000 => "EOS-1D C",
        0x1810000 => "EOS 700D",
        0x1830000 => "EOS 1300D",
        0x2500000 => "EOS 5D Mark IV",
        0x2530000 => "EOS 80D",
        0x2560000 => "EOS M5",
        0x2570000 => "EOS 200D",
        0x2580000 => "EOS 77D",
        0x2590000 => "EOS 800D",
        0x2610000 => "EOS 6D Mark II",
        0x2640000 => "EOS-1D X Mark II",
        0x2660000 => "EOS 4000D",
        0x2670000 => "EOS M50",
        0x2700000 => "EOS R",
        0x2720000 => "EOS RP",
        0x2730000 => "EOS 250D",
        0x2740000 => "EOS 90D",
        0x2750000 => "EOS M6 Mark II",
        0x2790000 => "EOS 850D",
        0x2810000 => "EOS R5",
        0x2820000 => "EOS R6",
        0x2830000 => "EOS R3",
        0x2840000 => "EOS R7",
        0x2850000 => "EOS R10",
        0x2860000 => "EOS R8",
        0x2870000 => "EOS R50",
        0x2880000 => "EOS R100",
        0x2890000 => "EOS R6 Mark II",
        0x2920000 => "EOS R5 Mark II",
        0x2960000 => "EOS R1",
        _ => return None,
    })
}

fn canon_wb_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0 => "Auto",
        1 => "Daylight",
        2 => "Cloudy",
        3 => "Tungsten",
        4 => "Fluorescent",
        5 => "Flash",
        6 => "Custom",
        7 => "Black & White",
        8 => "Shade",
        9 => "Manual Temperature (Kelvin)",
        10 => "PC Set 1",
        11 => "PC Set 2",
        12 => "PC Set 3",
        14 => "Daylight Fluorescent",
        15 => "Custom 1",
        16 => "Custom 2",
        17 => "Underwater",
        18 => "Custom 3",
        19 => "Custom 4",
        _ => return None,
    })
}

fn canon_picture_style_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x81 => "Standard",
        0x82 => "Portrait",
        0x83 => "Landscape",
        0x84 => "Neutral",
        0x85 => "Faithful",
        0x86 => "Monochrome",
        0x87 => "Auto",
        0x88 => "Fine Detail",
        0x90 => "User Def. 1",
        0x91 => "User Def. 2",
        0x92 => "User Def. 3",
        _ => return None,
    })
}

fn canon_quality_name(id: u16) -> Option<&'static str> {
    Some(match id {
        1 => "RAW",
        2 => "Large Fine",
        3 => "Large Normal",
        4 => "Medium Fine",
        5 => "Medium Normal",
        6 => "Small Fine",
        7 => "Small Normal",
        8 => "RAW + Large Fine",
        9 => "RAW + Large Normal",
        10 => "RAW + Medium Fine",
        11 => "RAW + Medium Normal",
        12 => "RAW + Small Fine",
        13 => "RAW + Small Normal",
        _ => return None,
    })
}

/// `base` is the offset of `data` within the TIFF block. Canon maker-note
/// value offsets are TIFF-relative, so we subtract `base` to index into
/// `data` (which starts at the maker note). Self-contained test buffers
/// pass `base = 0`.
fn parse_canon_makernote(data: &[u8], base: usize, tags: &mut Vec<TagEntry>) {
    if data.len() < 6 {
        return;
    }

    let (bo, ifd_off) = if &data[0..2] == b"II" || &data[0..2] == b"MM" {
        let bo = if &data[0..2] == b"II" { "LE" } else { "BE" };
        let off = read_tiff_u32(data, 4, bo) as usize;
        (bo, off)
    } else {
        // Canon maker notes are a bare IFD that begins directly with the
        // 2-byte entry count (no header, no offset prefix). The previous
        // "hint" heuristic mis-read a low first tag id (e.g. 0x0000 on the
        // IXUS) as an offset; the IFD is always at 0.
        ("LE", 0)
    };

    if ifd_off + 2 > data.len() {
        return;
    }
    let count = read_tiff_u16(data, ifd_off, bo) as usize;

    // FixBase: edited files mis-base maker-note offsets. Detect the delta
    // once (cross-validated across CameraSettings + ShotInfo), fold it into
    // `base`, and remember whether the base was confirmed (gates the
    // header-less FIRST_ENTRY 0 sub-tables). Delta 0 for well-formed files.
    #[cfg(feature = "exiftool-tables")]
    let (base, base_validated) = {
        let (delta, validated) = canon_base_delta(data, ifd_off, base, bo);
        (base + delta, validated)
    };

    let mut pos = ifd_off + 2;

    for _ in 0..count.min(100) {
        if pos + 12 > data.len() {
            break;
        }
        let tag_id = read_tiff_u16(data, pos, bo);
        let tag_type = read_tiff_u16(data, pos + 2, bo);
        let tag_count = read_tiff_u32(data, pos + 4, bo) as usize;
        let tag_size = exif_type_size(tag_type) * tag_count;
        pos += 8;

        let value = if tag_size <= 4 {
            if tag_type == 2 {
                let end = data[pos..pos + tag_count.min(4)]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count.min(4));
                String::from_utf8_lossy(&data[pos..pos + end]).to_string()
            } else {
                read_exif_val(data, pos, tag_type, tag_count, bo)
            }
        } else {
            let val_off = (read_tiff_u32(data, pos, bo) as usize).saturating_sub(base);
            if val_off + tag_size > data.len() {
                pos += 4;
                continue;
            }
            if tag_type == 2 {
                let end = data[val_off..val_off + tag_count]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count);
                String::from_utf8_lossy(&data[val_off..val_off + end]).to_string()
            } else {
                read_exif_val(data, val_off, tag_type, tag_count, bo)
            }
        };
        let val_field = pos;
        pos += 4;

        if let Some(name) = canon_tag_name(tag_id) {
            let formatted = canon_format_value(tag_id, &value);
            // Decode a scalar enum via the ExifTool PrintConv when the
            // bespoke formatter left it raw (e.g. DateStampMode 0 -> Off).
            // A non-numeric/formatted value just passes through. The lens
            // post-pass still runs afterwards and overrides by name.
            #[cfg(feature = "exiftool-tables")]
            let formatted = formatted
                .parse::<i64>()
                .ok()
                .and_then(|v| {
                    revelo_exiftool_tables::print_conv(
                        revelo_exiftool_tables::Vendor::Canon,
                        tag_id as u32,
                        v,
                    )
                })
                .map(str::to_string)
                .unwrap_or(formatted);
            // Align the few Canon Main names that diverge from exiftool.
            #[cfg(feature = "exiftool-tables")]
            let name = revelo_exiftool_tables::canon_main_name_alias(name).unwrap_or(name);
            tags.push((name, formatted));
        } else {
            // For Main tags revelo's (older) bespoke table lacks, fall back to
            // the fuller ExifTool Main table, decoding the value where a
            // simple PrintConv exists.
            #[cfg(feature = "exiftool-tables")]
            if let Some(name) = revelo_exiftool_tables::tag_name(
                revelo_exiftool_tables::Vendor::Canon,
                tag_id as u32,
            ) {
                let decoded = value
                    .parse::<i64>()
                    .ok()
                    .and_then(|v| {
                        revelo_exiftool_tables::print_conv(
                            revelo_exiftool_tables::Vendor::Canon,
                            tag_id as u32,
                            v,
                        )
                    })
                    .map(str::to_string)
                    .unwrap_or_else(|| value.clone());
                tags.push((name, decoded));
            }
        }

        // Sub-directory binary tables. With the ExifTool tables, decode
        // them into individual named entries; without, the legacy IFD-style
        // decoder handles CameraSettings/ShotInfo.
        #[cfg(feature = "exiftool-tables")]
        if tag_size > 0 {
            decode_canon_subdir(tag_id, data, val_field, tag_size, base, base_validated, bo, tags);
        }
        #[cfg(not(feature = "exiftool-tables"))]
        if matches!(tag_id, 0x0001 | 0x0004) && tag_size >= 2 {
            let expected = (read_tiff_u32(data, val_field, bo) as usize).saturating_sub(base);
            let sub_raw = if tag_size <= 4 {
                if val_field + tag_size.min(4) <= data.len() {
                    data[val_field..val_field + tag_size.min(4)].to_vec()
                } else {
                    vec![]
                }
            } else if expected + tag_size <= data.len() {
                data[expected..expected + tag_size].to_vec()
            } else {
                vec![]
            };
            if !sub_raw.is_empty() {
                parse_canon_sub_ifd(&sub_raw, bo, tag_id, tags);
            }
        }
    }
}

fn canon_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "CanonCameraSettings",
        0x0002 => "CanonFocalLength",
        0x0003 => "CanonFlashInfo",
        0x0004 => "CanonShotInfo",
        0x0006 => "CanonImageType",
        0x0007 => "CanonFirmwareVersion",
        0x0008 => "CanonImageNumber",
        0x0009 => "CanonOwnerName",
        // 0x000C/0x0010/0x0013/0x0015 corrected to match ExifTool's Canon
        // Main table (was: 0x000C=ModelID, 0x0010=ThumbnailValidArea,
        // 0x0015=SerialNumber — all off by tag id).
        0x000C => "CanonSerialNumber",
        0x000D => "CanonCameraInfo",
        0x0010 => "CanonModelID",
        0x0013 => "CanonThumbnailValidArea",
        0x0015 => "CanonSerialNumberFormat",
        0x001A => "CanonProcessingInfo",
        0x001C => "DateStampMode", // was mislabelled "CanonAFInfo"
        0x0020 => "CanonColorBalance",
        0x0021 => "CanonColorCalibration",
        0x0022 => "CanonColorMatrix",
        0x0024 => "CanonColorInfo",
        0x0026 => "CanonColorData",
        0x0028 => "ImageUniqueID", // was mislabelled "CanonCRWParam"
        0x002A => "CanonTimeZone",
        0x002D => "CanonDaylightSavings",
        0x0032 => "CanonBlackLevel",
        0x0033 => "CanonCustomPictureStyle",
        0x0035 => "CanonLensModel",
        0x0036 => "CanonInternalSerialNumber",
        0x0037 => "CanonDustRemovalData",
        0x0038 => "CanonCropInfo",
        0x0039 => "CanonCropInfo",
        0x003B => "CanonAspectRatioInfo",
        0x003C => "CanonCustomFunctions",
        0x003D => "CanonAEMicroadjustment",
        0x003E => "CanonAFMicroadjustment",
        0x0047 => "CanonAFConfig",
        0x004D => "CanonVRD",
        0x0050 => "CanonLensInfo",
        0x0052 => "CanonFaceDetect",
        0x0053 => "CanonFaceDetectData",
        0x0060 => "CanonMultiExp",
        0x0061 => "CanonHDRInfo",
        0x0062 => "CanonAFInfo2",
        0x0093 => "CanonFileDescription",
        0x0094 => "CanonModelName",
        0x0095 => "CanonOwnerName2",
        0x0096 => "CanonSerialNumber2",
        0x00A0 => "CanonTemperature",
        0x00A2 => "CanonWhiteBalance",
        0x00A3 => "CanonColorTemp",
        0x00A4 => "CanonWhitePoint",
        0x00A5 => "CanonColorSpace",
        0x00A9 => "CanonPictureStyle",
        0x00AA => "CanonDigitalGain",
        0x00AE => "CanonSensorInfo",
        0x00B4 => "CanonCustomFunctions2",
        0x00B5 => "CanonColorTemperature",
        0x00D1 => "CanonFlashExposureComp",
        0x00D2 => "CanonLensDriveNoise",
        0x00D5 => "CanonExternalFlash",
        0x00DB => "CanonCameraType",
        0x00E0 => "CanonFirmwareVersion2",
        0x00E1 => "CanonCameraType2",
        0x00E2 => "CanonFirmwareVersion3",
        0x00E4 => "CanonCategory",
        0x00E5 => "CanonModifiedInfo",
        0x00E6 => "CanonPictureStyle2",
        0x00E7 => "CanonImageStabilization",
        0x00E8 => "CanonShutterMode",
        0x00E9 => "CanonDriveMode",
        0x00EA => "CanonContrast",
        0x00EB => "CanonSaturation",
        0x00EC => "CanonSharpness",
        0x00ED => "CanonColorTone",
        0x00EE => "CanonFilterEffect",
        0x00EF => "CanonToningEffect",
        0x00F0 => "CanonBrightness",
        0x00F1 => "CanonISO",
        0x00F2 => "CanonMeteringMode",
        0x00F3 => "CanonExposureMode",
        0x00F4 => "CanonLensType",
        0x00F5 => "CanonLongExposureNoiseReduction",
        0x00F6 => "CanonHighISONoiseReduction",
        0x00F7 => "CanonAFAssistantLight",
        0x00F8 => "CanonFlashMode",
        0x00F9 => "CanonFlashActivity",
        0x00FA => "CanonFocusMode",
        0x00FB => "CanonAFPoint",
        0x00FC => "CanonAFPointsInFocus",
        0x00FD => "CanonBracketMode",
        0x00FE => "CanonBracketValue",
        0x00FF => "CanonBracketShotNumber",
        0x0100 => "CanonMirrorLockup",
        0x0101 => "CanonFlashSyncSpeedAV",
        0x0102 => "CanonLensAFStopButton",
        0x0103 => "CanonContinuousDrive",
        0x0104 => "CanonQuality",
        0x0105 => "CanonSharpnessFrequency",
        0x0106 => "CanonWhiteBalanceAdjust",
        0x0107 => "CanonWhiteBalanceBracket",
        0x0108 => "CanonExposureCompensation",
        0x0109 => "CanonISOSpeed",
        0x010A => "CanonFlashFired",
        0x010B => "CanonFlashExposureLock",
        0x010C => "CanonLensID",
        0x010D => "CanonCameraType3",
        0x010E => "CanonFirmwareVersion4",
        0x010F => "CanonCameraTemperature",
        _ => return None,
    })
}

fn nikon_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "NikonVersion",
        0x0002 => "NikonISOSpeed",
        0x0003 => "NikonColorMode",
        0x0004 => "NikonQuality",
        0x0005 => "NikonWhiteBalance",
        0x0006 => "NikonSharpening",
        0x0007 => "NikonFocusMode",
        0x0008 => "NikonFlashSetting",
        0x0009 => "NikonFlashType",
        0x000B => "NikonWhiteBalanceFine",
        0x000C => "NikonColorAdjustment",
        0x000D => "NikonLensType",
        0x0011 => "NikonLens",
        0x0012 => "NikonFlashMode",
        0x0013 => "NikonAutoFlash",
        0x0014 => "NikonFlashExposureComp",
        0x0017 => "NikonSceneMode",
        0x0018 => "NikonNoiseReduction",
        0x0023 => "NikonShootingMode",
        0x0024 => "NikonImageOptimization",
        0x0025 => "NikonSaturation",
        0x0026 => "NikonVariProgram",
        0x0027 => "NikonImageStabilization",
        0x0028 => "NikonAFResponse",
        0x0032 => "NikonActiveDLighting",
        0x0080 => "NikonModelID",
        0x0085 => "NikonSerialNumber",
        0x0086 => "NikonColorSpace",
        0x0088 => "NikonImageAuthentication",
        0x0098 => "NikonLensSpec",
        0x00A0 => "NikonShutterCount",
        0x00A7 => "NikonVignetteControl",
        0x00A8 => "NikonDistortionControl",
        0x00E6 => "NikonISOInfo",
        0x00E7 => "NikonVRActive",
        _ => return None,
    })
}

/// Decode the small Nikon AFInfo (0x0088) record: AFAreaMode (byte 0),
/// AFPoint (byte 1), AFPointsInFocus (int16 at bytes 2-3).
#[cfg(feature = "exiftool-tables")]
fn decode_nikon_afinfo(bytes: &[u8], bo: &str, tags: &mut Vec<TagEntry>) {
    use revelo_exiftool_tables::{nikon_afinfo_print_conv, nikon_afinfo_tag_name};
    if bytes.len() < 4 {
        return;
    }
    let fields =
        [(0u32, bytes[0] as i64), (1, bytes[1] as i64), (2, read_tiff_u16(bytes, 2, bo) as i64)];
    for (idx, raw) in fields {
        if let Some(name) = nikon_afinfo_tag_name(idx) {
            let v = nikon_afinfo_print_conv(idx, raw)
                .map(str::to_string)
                .unwrap_or_else(|| raw.to_string());
            tags.push((name, v));
        }
    }
}

fn parse_nikon_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    if data.len() < 6 {
        return;
    }
    // Nikon Type 2/3: "Nikon\0" + version(2) + pad(2) + a self-contained
    // TIFF header at offset 10, whose value offsets are relative to that
    // header. Anchor `sub` there so offsets resolve. Type 1 is a bare IFD
    // right after "Nikon\0".
    let offset = if data.len() >= 12
        && &data[0..6] == b"Nikon\0"
        && (&data[10..12] == b"II" || &data[10..12] == b"MM")
    {
        10
    } else if data.len() >= 6 && &data[0..6] == b"Nikon\0" {
        6
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }

    let (bo, ifd_off) = if sub.len() >= 8 && (&sub[0..2] == b"II" || &sub[0..2] == b"MM") {
        let bo = if &sub[0..2] == b"II" { "LE" } else { "BE" };
        (bo, read_tiff_u32(sub, 4, bo) as usize)
    } else {
        ("BE", 0usize)
    };

    if ifd_off + 2 > sub.len() {
        return;
    }
    let count = read_tiff_u16(sub, ifd_off, bo) as usize;
    if count > 100 {
        return;
    }
    let mut pos = ifd_off + 2;

    for _ in 0..count.min(100) {
        if pos + 12 > sub.len() {
            break;
        }
        let tag_id = read_tiff_u16(sub, pos, bo);
        let tag_type = read_tiff_u16(sub, pos + 2, bo);
        let tag_count = read_tiff_u32(sub, pos + 4, bo) as usize;
        let tag_size = exif_type_size(tag_type) * tag_count;
        pos += 8;

        let value = if tag_size <= 4 {
            if tag_type == 2 {
                let end = sub[pos..pos + tag_count.min(4)]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count.min(4));
                String::from_utf8_lossy(&sub[pos..pos + end]).to_string()
            } else {
                read_exif_val(sub, pos, tag_type, tag_count, bo)
            }
        } else {
            let val_off = read_tiff_u32(sub, pos, bo) as usize;
            if val_off + tag_size > sub.len() {
                pos += 4;
                continue;
            }
            if tag_type == 2 {
                let end = sub[val_off..val_off + tag_count]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count);
                String::from_utf8_lossy(&sub[val_off..val_off + end]).to_string()
            } else {
                read_exif_val(sub, val_off, tag_type, tag_count, bo)
            }
        };
        pos += 4;

        // On COOLPIX bodies, 0x0088 is the 4-byte AFInfo record and 0x00A8
        // is FlashInfo (version string), not the scalar tags revelo's
        // bespoke table names. Decode the sub-fields under the feature; the
        // size guard avoids touching other models' uses of these ids.
        #[cfg(feature = "exiftool-tables")]
        if (tag_id == 0x0088 && tag_size == 4) || (tag_id == 0x00A8 && tag_size >= 4) {
            let vf = pos - 4;
            let raw_off = if tag_size <= 4 { vf } else { read_tiff_u32(sub, vf, bo) as usize };
            if raw_off + 4 <= sub.len() {
                if tag_id == 0x0088 {
                    decode_nikon_afinfo(&sub[raw_off..], bo, tags);
                } else {
                    let v = String::from_utf8_lossy(&sub[raw_off..raw_off + 4])
                        .trim_matches('\0')
                        .to_string();
                    tags.push(("FlashInfoVersion", v));
                }
            }
            continue;
        }

        if let Some(name) = nikon_tag_name(tag_id) {
            let formatted = nikon_format_value(tag_id, &value);
            // Prefer the ExifTool name (revelo's bespoke names are
            // Nikon-prefixed; exiftool uses ColorMode, Quality, …).
            #[cfg(feature = "exiftool-tables")]
            let name = revelo_exiftool_tables::tag_name(
                revelo_exiftool_tables::Vendor::Nikon,
                tag_id as u32,
            )
            .unwrap_or(name);
            tags.push((name, formatted));
        } else {
            // Tags the bespoke table lacks: use the fuller ExifTool table,
            // decoding an integer PrintConv where one exists.
            #[cfg(feature = "exiftool-tables")]
            if let Some(name) = revelo_exiftool_tables::tag_name(
                revelo_exiftool_tables::Vendor::Nikon,
                tag_id as u32,
            ) {
                let decoded = value
                    .parse::<i64>()
                    .ok()
                    .and_then(|v| {
                        revelo_exiftool_tables::print_conv(
                            revelo_exiftool_tables::Vendor::Nikon,
                            tag_id as u32,
                            v,
                        )
                    })
                    .map(str::to_string)
                    .unwrap_or_else(|| value.clone());
                tags.push((name, decoded));
            }
        }
    }
}

fn sony_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0102 => "SonyQuality",
        0x0104 => "SonyFlashExposureComp",
        0x0105 => "SonyWhiteBalanceFine",
        0x0114 => "SonyColorTemperature",
        0x0115 => "SonyWhiteBalance",
        0x0116 => "SonyColorMode",
        0x0117 => "SonyColorSpace",
        0x0120 => "SonySharpness",
        0x0121 => "SonyContrast",
        0x0122 => "SonySaturation",
        0x0124 => "SonyDynamicRangeOptimizer",
        0x0127 => "SonyExposureMode",
        0x0131 => "SonyFocusMode",
        0x0132 => "SonyAFAreaMode",
        0xB000 => "SonyModelID",
        0xB001 => "SonyModelName",
        _ => return None,
    })
}

fn parse_sony_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    if data.len() < 2 {
        return;
    }

    let offset = if data.len() >= 8 && (&data[0..4] == b"SONY" || &data[0..4] == b"Sony") {
        if &data[4..8] == b" DSC" || &data[4..8] == b"\0\0\0\0" { 8 } else { 4 }
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }

    let (bo, ifd_off) = if sub.len() >= 8 && (&sub[0..2] == b"II" || &sub[0..2] == b"MM") {
        let bo = if &sub[0..2] == b"II" { "LE" } else { "BE" };
        (bo, read_tiff_u32(sub, 4, bo) as usize)
    } else {
        ("LE", 0usize)
    };

    if ifd_off + 2 > sub.len() {
        return;
    }
    let count = read_tiff_u16(sub, ifd_off, bo) as usize;
    if count > 100 {
        return;
    }
    let mut pos = ifd_off + 2;

    for _ in 0..count.min(100) {
        if pos + 12 > sub.len() {
            break;
        }
        let tag_id = read_tiff_u16(sub, pos, bo);
        let tag_type = read_tiff_u16(sub, pos + 2, bo);
        let tag_count = read_tiff_u32(sub, pos + 4, bo) as usize;
        let tag_size = exif_type_size(tag_type) * tag_count;
        pos += 8;

        let value = if tag_size <= 4 {
            if tag_type == 2 {
                let end = sub[pos..pos + tag_count.min(4)]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count.min(4));
                String::from_utf8_lossy(&sub[pos..pos + end]).to_string()
            } else {
                read_exif_val(sub, pos, tag_type, tag_count, bo)
            }
        } else {
            let val_off = read_tiff_u32(sub, pos, bo) as usize;
            if val_off + tag_size > sub.len() {
                pos += 4;
                continue;
            }
            if tag_type == 2 {
                let end = sub[val_off..val_off + tag_count]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count);
                String::from_utf8_lossy(&sub[val_off..val_off + end]).to_string()
            } else {
                read_exif_val(sub, val_off, tag_type, tag_count, bo)
            }
        };
        pos += 4;

        if let Some(name) = sony_tag_name(tag_id) {
            let formatted = sony_format_value(tag_id, &value);
            tags.push((name, formatted));
        }
    }
}

// ---------- Olympus MakerNote ----------

fn olympus_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0200 => "OlympusSpecialMode",
        0x0201 => "OlympusQuality",
        0x0202 => "OlympusMacro",
        0x0203 => "OlympusBWMode",
        0x0204 => "OlympusDigitalZoom",
        0x0205 => "OlympusFocalPlaneDiagonal",
        0x0206 => "OlympusLensDistortionParams",
        0x0207 => "OlympusCameraType",
        0x0208 => "OlympusTextInfo",
        0x0209 => "OlympusCameraID",
        0x020B => "OlympusEpsonImageWidth",
        0x020C => "OlympusEpsonImageHeight",
        0x020D => "OlympusEpsonSoftware",
        0x0303 => "OlympusWhiteBalanceBracket",
        0x0304 => "OlympusWhiteBalanceBias",
        0x0600 => "OlympusPreCaptureFrames",
        0x1010 => "OlympusSerialNumber",
        0x1011 => "OlympusFirmware",
        0x2010 => "OlympusEquipment",
        0x2020 => "OlympusCameraSettings",
        0x2030 => "OlympusRawDevelopment",
        0x2040 => "OlympusRawDev2",
        0x3000 => "OlympusRawInfo",
        _ => return None,
    })
}

/// Read one IFD entry's value as a display string (mirrors the maker-note
/// readers): ASCII strings, else `read_exif_val`. `vf` is the value field
/// offset within `sub`.
#[cfg(feature = "exiftool-tables")]
fn read_ifd_value(
    sub: &[u8],
    vf: usize,
    tag_type: u16,
    tag_count: usize,
    bo: &str,
) -> Option<String> {
    let tag_size = exif_type_size(tag_type) * tag_count;
    let off = if tag_size <= 4 { vf } else { read_tiff_u32(sub, vf, bo) as usize };
    if off + tag_size > sub.len() {
        return None;
    }
    Some(if tag_type == 2 {
        let end = sub[off..off + tag_count].iter().position(|&b| b == 0).unwrap_or(tag_count);
        String::from_utf8_lossy(&sub[off..off + end]).trim().to_string()
    } else {
        read_exif_val(sub, off, tag_type, tag_count, bo)
    })
}

/// Walk an Olympus "type-2" IFD. The main IFD (`table == None`) recurses
/// into the Equipment/CameraSettings/… sub-IFDs; sub-IFDs decode via their
/// generated table. All offsets are relative to `sub` (the "II"/"MM" base).
#[cfg(feature = "exiftool-tables")]
fn olympus_walk(
    sub: &[u8],
    ifd_off: usize,
    bo: &str,
    table: Option<revelo_exiftool_tables::OlympusSubTable>,
    tags: &mut Vec<TagEntry>,
) {
    use revelo_exiftool_tables::{OlympusSubTable, Vendor};
    if ifd_off + 2 > sub.len() {
        return;
    }
    let count = read_tiff_u16(sub, ifd_off, bo) as usize;
    if count > 256 {
        return;
    }
    let mut pos = ifd_off + 2;
    for _ in 0..count.min(256) {
        if pos + 12 > sub.len() {
            break;
        }
        let tag_id = read_tiff_u16(sub, pos, bo);
        let tag_type = read_tiff_u16(sub, pos + 2, bo);
        let tag_count = read_tiff_u32(sub, pos + 4, bo) as usize;
        pos += 8;
        // Main IFD: sub-IFD pointers recurse.
        if table.is_none() {
            if let Some(st) = OlympusSubTable::from_tag(tag_id) {
                let off = read_tiff_u32(sub, pos, bo) as usize;
                olympus_walk(sub, off, bo, Some(st), tags);
                pos += 4;
                continue;
            }
        }
        let value = read_ifd_value(sub, pos, tag_type, tag_count, bo);
        pos += 4;
        let Some(value) = value else { continue };
        let (name, conv) = match table {
            Some(st) => (
                revelo_exiftool_tables::olympus_sub_tag_name(st, tag_id as u32),
                value.parse::<i64>().ok().and_then(|v| {
                    revelo_exiftool_tables::olympus_sub_print_conv(st, tag_id as u32, v)
                }),
            ),
            None => (
                revelo_exiftool_tables::tag_name(Vendor::Olympus, tag_id as u32)
                    .or_else(|| olympus_tag_name(tag_id)),
                value.parse::<i64>().ok().and_then(|v| {
                    revelo_exiftool_tables::print_conv(Vendor::Olympus, tag_id as u32, v)
                }),
            ),
        };
        if let Some(name) = name {
            tags.push((name, conv.map(str::to_string).unwrap_or(value)));
        }
    }
}

fn parse_olympus_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Olympus "type-2": "OLYMPUS\0" + "II"/"MM" + 2-byte magic, then the
    // main IFD, with nested sub-IFDs. Offsets are relative to the "II".
    #[cfg(feature = "exiftool-tables")]
    if data.len() >= 12
        && &data[0..8] == b"OLYMPUS\0"
        && (&data[8..10] == b"II" || &data[8..10] == b"MM")
    {
        // All offsets are relative to the maker-note start (data[0]); the
        // main IFD sits at offset 12 ("OLYMPUS\0"(8) + BOM(2) + magic(2)).
        let bo = if &data[8..10] == b"II" { "LE" } else { "BE" };
        olympus_walk(data, 12, bo, None, tags);
        return;
    }

    let offset = if data.len() >= 6 && &data[0..6] == b"OLYMP\0" {
        if data.len() >= 8 && &data[0..8] == b"OLYMPUS\0" { 8 } else { 6 }
    } else if data.len() >= 8 && &data[0..8] == b"OLYMPUS\0" {
        8
    } else if data.len() >= 2 && (&data[0..2] == b"II" || &data[0..2] == b"MM") {
        0
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "BE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, olympus_tag_name, MakerVendor::Olympus, 0);
}

// ---------- Panasonic MakerNote ----------

fn panasonic_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "PanasonicQuality",
        0x0002 => "PanasonicVersion",
        0x0003 => "PanasonicWhiteBalance",
        0x0007 => "PanasonicFocusMode",
        0x000F => "PanasonicAFAreaMode",
        0x001A => "PanasonicStabilization",
        0x001C => "PanasonicMacro",
        0x001F => "PanasonicAudio",
        0x0020 => "PanasonicImageStabilization",
        0x0021 => "PanasonicShootingMode",
        0x0025 => "PanasonicWhiteBalanceBias",
        0x0026 => "PanasonicFlashBias",
        0x0027 => "PanasonicSerialNumber",
        0x0028 => "PanasonicExifVersion",
        0x002C => "PanasonicColorEffect",
        0x002E => "PanasonicTimeSincePowerOn",
        0x0031 => "PanasonicBurstMode",
        0x0032 => "PanasonicSequenceNumber",
        0x0033 => "PanasonicContrastMode",
        0x0034 => "PanasonicNoiseReduction",
        0x0035 => "PanasonicSelfTimer",
        0x0036 => "PanasonicRotation",
        0x0037 => "PanasonicAFAssistLamp",
        0x0038 => "PanasonicColorMode",
        0x0039 => "PanasonicBabyAge",
        0x003A => "PanasonicOpticalZoomMode",
        0x003B => "PanasonicConversionLens",
        0x003C => "PanasonicTravelDay",
        0x003E => "PanasonicProgramISO",
        0x003F => "PanasonicAdvancedSceneMode",
        0x0040 => "PanasonicTextStamp",
        0x0041 => "PanasonicFacesDetected",
        0x0044 => "PanasonicAFPointPosition",
        0x0045 => "PanasonicFilmMode",
        0x0047 => "PanasonicColorTemp",
        0x0048 => "PanasonicSceneMode",
        0x004B => "PanasonicWBAdjustAB",
        0x004C => "PanasonicWBAdjustGM",
        0x004D => "PanasonicAuxiliaryLens",
        _ => return None,
    })
}

fn parse_panasonic_makernote(data: &[u8], base: usize, tags: &mut Vec<TagEntry>) {
    let offset = if data.len() >= 12 && &data[..12] == b"Panasonic\0\0\0" {
        12
    } else if data.len() >= 11 && &data[..11] == b"Panasonic\0\0" {
        11
    } else if data.len() >= 10 && &data[..10] == b"Panasonic\0" {
        10
    } else if data.len() >= 2 && (&data[0..2] == b"II" || &data[0..2] == b"MM") {
        0
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "LE");
    // Panasonic value offsets are relative to the maker-note start, so the
    // base for `sub` (which skips the header) is the header length.
    parse_makernote_ifd(
        sub,
        ifd_off,
        bo,
        tags,
        panasonic_tag_name,
        MakerVendor::Panasonic,
        base + offset,
    );
}

// ---------- Pentax MakerNote ----------

fn pentax_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x0001 => "PentaxModelType",
        0x0002 => "PentaxModelID",
        0x0003 => "PentaxQuality",
        0x0004 => "PentaxISO",
        0x0005 => "PentaxPictureMode",
        0x0006 => "PentaxFlashMode",
        0x0007 => "PentaxFocusMode",
        0x0008 => "PentaxAFPointSelected",
        0x0009 => "PentaxAFPointsInFocus",
        0x000B => "PentaxWhiteBalance",
        0x000C => "PentaxWhiteBalanceMode",
        0x000E => "PentaxSaturation",
        0x000F => "PentaxContrast",
        0x0010 => "PentaxSharpness",
        0x0012 => "PentaxColorSpace",
        0x0013 => "PentaxHue",
        0x0014 => "PentaxExposureCompensation",
        0x001B => "PentaxShutterSpeed",
        0x001C => "PentaxAperture",
        0x0020 => "PentaxDestinationCity",
        0x0023 => "PentaxDestinationDST",
        0x0041 => "PentaxCameraInfo",
        0x0059 => "PentaxLensInfo",
        0x005C => "PentaxLensType",
        0x007F => "PentaxCameraModel",
        0x0200 => "PentaxBlackPoint",
        0x0201 => "PentaxWhitePoint",
        0x0202 => "PentaxColorMatrix",
        0x0203 => "PentaxWBInfo",
        0x0213 => "PentaxLensData",
        _ => return None,
    })
}

fn parse_pentax_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Pentax MakerNote starts with "AOC\0" (Asahi Optical Co.) or "PENTAX \0" or "PENTAX\0"
    let offset = if data.len() >= 4 && &data[0..4] == b"AOC\0" {
        4
    } else if data.len() >= 7 && &data[0..7] == b"PENTAX " {
        7
    } else if data.len() >= 8 && (&data[0..2] == b"II" || &data[0..2] == b"MM") {
        0
    } else {
        0
    };
    let sub = &data[offset..];
    if sub.len() < 2 {
        return;
    }
    let (bo, ifd_off) = parse_tiff_header_or_raw(sub, "BE");
    parse_makernote_ifd(sub, ifd_off, bo, tags, pentax_tag_name, MakerVendor::Pentax, 0);
}

// ---------- Fujifilm MakerNote ----------

fn fujifilm_tag_name(tag_id: u16) -> Option<&'static str> {
    Some(match tag_id {
        0x1000 => "FujiQuality",
        0x1001 => "FujiSharpness",
        0x1002 => "FujiWhiteBalance",
        0x1003 => "FujiColorSaturation",
        0x1004 => "FujiTone",
        0x1005 => "FujiColorTemperature",
        0x1006 => "FujiContrast",
        0x1007 => "FujiColorMode",
        0x1010 => "FujiMacro",
        0x1011 => "FujiFlashMode",
        0x1020 => "FujiFocusMode",
        0x1021 => "FujiAFMode",
        0x1030 => "FujiSlowSync",
        0x1031 => "FujiPictureMode",
        0x1032 => "FujiExposureCount",
        0x1100 => "FujiShadowTone",
        0x1101 => "FujiHighlightTone",
        0x1102 => "FujiNoiseReduction",
        0x1200 => "FujiSequenceNumber",
        0x1300 => "FujiFineSharpness",
        0x1400 => "FujiBlurWarning",
        0x1401 => "FujiFocusWarning",
        0x1402 => "FujiExposureWarning",
        0x1403 => "FujiDRange",
        0x1404 => "FujiDynamicRange",
        0x1405 => "FujiFilmMode",
        0x1406 => "FujiDynamicRangeSetting",
        0x1407 => "FujiDevelopmentDynamicRange",
        0x1408 => "FujiMinFocalLength",
        0x1409 => "FujiMaxFocalLength",
        0x140A => "FujiMaxApertureAtMinFocal",
        0x140B => "FujiMaxApertureAtMaxFocal",
        0x1420 => "FujiAutoDynamicRange",
        0x2F00 => "FujiFaceInfo",
        _ => return None,
    })
}

fn parse_fujifilm_makernote(data: &[u8], tags: &mut Vec<TagEntry>) {
    // Fujifilm MakerNote: "FUJIFILM" (8 bytes) + a 4-byte little-endian
    // offset to the IFD. Both the IFD and the entries' value offsets are
    // relative to the start of the maker note (the 'F' of FUJIFILM), so we
    // parse against the full `data` with that IFD offset — not a slice,
    // which would mis-resolve the value offsets.
    let (ifd_off, bo) = if data.len() >= 12 && &data[0..8] == b"FUJIFILM" {
        (u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize, "LE")
    } else if data.len() >= 2 && (&data[0..2] == b"II" || &data[0..2] == b"MM") {
        let bo = if &data[0..2] == b"II" { "LE" } else { "BE" };
        (read_tiff_u32(data, 4, bo) as usize, bo)
    } else {
        return;
    };
    if ifd_off + 2 > data.len() {
        return;
    }
    parse_makernote_ifd(data, ifd_off, bo, tags, fujifilm_tag_name, MakerVendor::Fujifilm, 0);
}

// ---------- Nikon human-readable tag formatting ----------

fn nikon_format_value(tag_id: u16, raw_value: &str) -> String {
    match tag_id {
        0x0080 => {
            let id: u32 = raw_value.parse().unwrap_or(u32::MAX);
            nikon_model_name(id).unwrap_or(raw_value).to_string()
        }
        0x0086 => match raw_value {
            "1" => "sRGB",
            "2" => "Adobe RGB",
            _ => raw_value,
        }
        .to_string(),
        0x000D => {
            // NikonLensType is a bitmask
            let id: u16 = raw_value.parse().unwrap_or(0);
            if id == 0 {
                return raw_value.to_string();
            }
            let mut parts = Vec::new();
            if id & 0x01 != 0 {
                parts.push("MF");
            }
            if id & 0x02 != 0 {
                parts.push("D");
            }
            if id & 0x04 != 0 {
                parts.push("G");
            }
            if id & 0x08 != 0 {
                parts.push("VR");
            }
            if parts.is_empty() { raw_value.to_string() } else { parts.join(" ") }
        }
        _ => raw_value.to_string(),
    }
}

fn nikon_model_name(id: u32) -> Option<&'static str> {
    Some(match id {
        0x800001 => "D1H",
        0x800002 => "D1X",
        0x800004 => "D100",
        0x800005 => "D2H",
        0x800006 => "D2X",
        0x800007 => "D2Hs",
        0x800008 => "D2Xs",
        0x800009 => "D200",
        0x80000A => "D80",
        0x80000B => "D40",
        0x80000C => "D40x",
        0x80000E => "D300",
        0x80000F => "D3",
        0x800011 => "D3X",
        0x800012 => "D3S",
        0x800013 => "D90",
        0x800014 => "D700",
        0x800015 => "D5000",
        0x800016 => "D3000",
        0x800017 => "D300S",
        0x800018 => "D3100",
        0x800019 => "D7000",
        0x80001B => "D5100",
        0x80001C => "D800",
        0x80001E => "D3200",
        0x80001F => "D600",
        0x800020 => "D800E",
        0x800022 => "D5200",
        0x800023 => "D7100",
        0x800025 => "D5300",
        0x800026 => "Df",
        0x800027 => "D3300",
        0x800028 => "D4S",
        0x800029 => "D750",
        0x80002A => "D810",
        0x80002B => "D5500",
        0x80002C => "D7200",
        0x80002E => "D5",
        0x80002F => "D500",
        0x800030 => "D3400",
        0x800031 => "D5600",
        0x800032 => "D850",
        0x800033 => "D7500",
        0x800035 => "D3500",
        0x800036 => "D780",
        0x800038 => "D6",
        0x80003A => "Z 7",
        0x80003B => "Z 6",
        0x80003C => "Z 50",
        0x80003D => "Z 5",
        0x80003E => "Z 7II",
        0x80003F => "Z 6II",
        0x800040 => "Z 9",
        0x800041 => "Z fc",
        0x800042 => "Z 30",
        0x800043 => "Z 8",
        0x800045 => "Z f",
        _ => return None,
    })
}

// ---------- Sony human-readable tag formatting ----------

fn sony_format_value(tag_id: u16, raw_value: &str) -> String {
    match tag_id {
        0x0117 => match raw_value {
            "1" => "sRGB",
            "2" => "Adobe RGB",
            _ => raw_value,
        }
        .to_string(),
        0xB000 => {
            let id: u16 = raw_value.parse().unwrap_or(u16::MAX);
            sony_model_name(id).unwrap_or(raw_value).to_string()
        }
        _ => raw_value.to_string(),
    }
}

fn sony_model_name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x0000 => "DSLR-A100",
        0x0001 => "DSLR-A700",
        0x0002 => "DSLR-A200",
        0x0003 => "DSLR-A300",
        0x0004 => "DSLR-A350",
        0x0005 => "DSLR-A900",
        0x0006 => "DSLR-A380",
        0x0007 => "DSLR-A330",
        0x0008 => "DSLR-A230",
        0x0009 => "DSLR-A290",
        0x000A => "DSLR-A390",
        0x000B => "DSLR-A450",
        0x000C => "DSLR-A500",
        0x000D => "DSLR-A550",
        0x000E => "DSLR-A560",
        0x000F => "DSLR-A580",
        0x0010 => "DSLR-A33",
        0x0011 => "DSLR-A55",
        0x0012 => "DSLR-A77",
        0x0013 => "DSLR-A65",
        0x0014 => "DSLR-A37",
        0x0015 => "DSLR-A57",
        0x0016 => "DSLR-NEX-3",
        0x0017 => "DSLR-NEX-5",
        0x0018 => "DSLR-NEX-5C",
        0x0019 => "DSLR-NEX-C3",
        0x001A => "DSLR-NEX-VG10",
        0x001B => "DSLR-NEX-5N",
        0x001C => "DSLR-NEX-7",
        0x001D => "DSLR-NEX-3N",
        0x001E => "DSLR-NEX-6",
        0x001F => "DSLR-NEX-F3",
        0x0020 => "SLT-A99",
        0x0021 => "DSLR-NEX-5R",
        0x0022 => "DSLR-NEX-5T",
        0x0023 => "DSLR-NEX-3NL",
        0x0025 => "ILCE-3000",
        0x0026 => "ILCE-5000",
        0x0027 => "ILCE-6000",
        0x0028 => "ILCE-7S",
        0x0029 => "ILCE-7",
        0x002A => "ILCE-7R",
        0x002B => "ILCE-7M2",
        0x002C => "ILCE-7RM2",
        0x002D => "ILCE-7SM2",
        0x002E => "ILCE-6300",
        0x0030 => "ILCE-9",
        0x0031 => "ILCE-6500",
        0x0032 => "ILCE-7RM3",
        0x0034 => "ILCE-7M3",
        0x0038 => "ILCE-7RM4",
        0x0039 => "ILCE-7C",
        0x003B => "ILCE-1",
        0x003F => "ILCE-7SM3",
        0x0042 => "ILCE-7M4",
        0x0043 => "ILCE-7RM5",
        0x0045 => "ZV-1",
        0x0046 => "ZV-E10",
        0x004B => "ZV-E1",
        0x004E => "ZV-1F",
        0x0050 => "ILCE-6700",
        _ => return None,
    })
}

// ---------- Canon sub-IFD parsing ----------

#[cfg(not(feature = "exiftool-tables"))]
fn canon_sub_tag_name(tag_id: u16, sub_tag: u16) -> Option<&'static str> {
    let name = match tag_id {
        0x0001 => match sub_tag {
            1 => "MacroMode",
            2 => "SelfTimer",
            3 => "Quality",
            4 => "FlashMode",
            5 => "DriveMode",
            6 => "FocusMode",
            7 => "RecordMode",
            8 => "ImageSize",
            9 => "EasyShootingMode",
            10 => "DigitalZoom",
            11 => "Contrast",
            12 => "Saturation",
            13 => "Sharpness",
            14 => "CameraISO",
            15 => "MeteringMode",
            16 => "FocusRange",
            17 => "AFPoint",
            18 => "CanonExposureMode",
            19 => "LongExposureNoiseReduction",
            20 => "HighISONoiseReduction",
            _ => return None,
        },
        0x0004 => match sub_tag {
            1 => "ShotInfoISO",
            2 => "ShutterSpeed",
            3 => "Aperture",
            4 => "ExposureCompensation",
            5 => "FlashCompensation",
            6 => "FocalLength",
            7 => "FocalLengthIn35mm",
            8 => "ShootingDistance",
            _ => return None,
        },
        _ => return None,
    };
    Some(name)
}

/// Accept a Canon binary record only when the stored offset lands on a
/// well-formed record. These records (CameraSettings, ShotInfo, …) begin
/// with a 16-bit field equal to their own byte length, so we require
/// `data[expected] == tag_size`. On unedited files the offset is correct
/// and the record decodes; on edited files whose maker-note offsets are
/// mis-based (exiftool: "Adjusted MakerNotes base by N"), the header
/// won't match and we return `None` — decoding nothing rather than a
/// wrong region. We deliberately do NOT scan for the header, since an
/// unrelated int16 can equal `tag_size` and yield a false positive.
#[cfg(feature = "exiftool-tables")]
fn locate_canon_record(data: &[u8], expected: usize, tag_size: usize, bo: &str) -> Option<usize> {
    let valid =
        expected + tag_size <= data.len() && read_tiff_u16(data, expected, bo) as usize == tag_size;
    valid.then_some(expected)
}

/// Detect a Canon maker-note base mis-adjustment (exiftool's "Adjusted
/// MakerNotes base by N"), returned as the number of bytes to subtract
/// from every stored value offset. Edited files (re-muxed/re-saved) leave
/// the maker-note offsets pointing past the relocated data.
///
/// The delta is found by locating the CameraSettings record via its
/// self-describing byte-count header, then **cross-checking** that the
/// same delta also makes ShotInfo self-validate. Requiring two
/// independent records to validate at one delta makes a false positive
/// astronomically unlikely, so we never shift onto a wrong region.
/// Returns `(delta, base_validated)`. `base_validated` is true only when a
/// CameraSettings record was found and confirmed (either already correct,
/// or recovered via a cross-validated delta) — the gate for decoding
/// `FIRST_ENTRY 0` sub-tables that lack a self-locating header.
#[cfg(feature = "exiftool-tables")]
fn canon_base_delta(data: &[u8], ifd_off: usize, base: usize, bo: &str) -> (usize, bool) {
    let mut cs: Option<(usize, usize)> = None; // CameraSettings (raw_off, size)
    let mut si: Option<(usize, usize)> = None; // ShotInfo
    if ifd_off + 2 > data.len() {
        return (0, false);
    }
    let count = read_tiff_u16(data, ifd_off, bo) as usize;
    let mut pos = ifd_off + 2;
    for _ in 0..count.min(100) {
        if pos + 12 > data.len() {
            break;
        }
        let tag_id = read_tiff_u16(data, pos, bo);
        let tag_type = read_tiff_u16(data, pos + 2, bo);
        let tag_count = read_tiff_u32(data, pos + 4, bo) as usize;
        let tag_size = exif_type_size(tag_type) * tag_count;
        if tag_size > 4 {
            let raw_off = read_tiff_u32(data, pos + 8, bo) as usize;
            match tag_id {
                0x0001 => cs = Some((raw_off, tag_size)),
                0x0004 => si = Some((raw_off, tag_size)),
                _ => {}
            }
        }
        pos += 12;
    }

    let Some((cs_off, cs_size)) = cs else {
        return (0, false);
    };
    let valid =
        |p: usize, sz: usize| p + sz <= data.len() && read_tiff_u16(data, p, bo) as usize == sz;
    let cs_expected = cs_off.saturating_sub(base);
    if valid(cs_expected, cs_size) {
        return (0, true); // offsets already correct (unedited)
    }
    // Scan for a position whose byte-count header matches CameraSettings,
    // then require ShotInfo to validate at the same delta.
    let mut p = 0;
    while p + 2 <= cs_expected.min(data.len()) {
        if valid(p, cs_size) {
            let delta = cs_expected - p;
            match si {
                Some((si_off, si_size)) => {
                    let si_p = si_off.saturating_sub(base).saturating_sub(delta);
                    if valid(si_p, si_size) {
                        return (delta, true);
                    }
                }
                None => return (delta, true),
            }
        }
        p += 2;
    }
    (0, false)
}

/// Extract and decode a Canon `ProcessBinaryData` sub-directory. Resolves
/// the record bytes safely — self-locating tables (CameraSettings,
/// ShotInfo) by their byte-count header, header-less `FIRST_ENTRY 0`
/// tables (FocalLength) only when FixBase confirmed the maker-note base —
/// then decodes by index.
#[cfg(feature = "exiftool-tables")]
#[allow(clippy::too_many_arguments)]
fn decode_canon_subdir(
    tag_id: u16,
    data: &[u8],
    val_field: usize,
    tag_size: usize,
    base: usize,
    base_validated: bool,
    bo: &str,
    tags: &mut Vec<TagEntry>,
) {
    use revelo_exiftool_tables::CanonSubTable;
    // AFInfo (0x0012) is variable-length — hand-walked, header-less, so it
    // needs a confirmed base. Its own size-match gate is the final guard.
    if tag_id == 0x0012 {
        if base_validated && tag_size > 4 {
            let expected = (read_tiff_u32(data, val_field, bo) as usize).saturating_sub(base);
            if expected + tag_size <= data.len() {
                decode_canon_afinfo(&data[expected..expected + tag_size], bo, tags);
            }
        }
        return;
    }
    // AFInfo2 (0x0026) and AFInfo3 (0x003C, newer SerialData framing, same
    // field layout) both lead with the AFInfoSize byte-count header, so
    // they self-locate — no base guess needed.
    if tag_id == 0x0026 || tag_id == 0x003C {
        if tag_size > 4 {
            let expected = (read_tiff_u32(data, val_field, bo) as usize).saturating_sub(base);
            if let Some(p) = locate_canon_record(data, expected, tag_size, bo) {
                decode_canon_afinfo2(&data[p..p + tag_size], bo, tags);
            }
        }
        return;
    }
    let Some(table) = CanonSubTable::from_tag(tag_id) else {
        return;
    };
    if tag_size < 2 {
        return;
    }
    let sub_raw = if tag_size <= 4 {
        if val_field + tag_size.min(4) <= data.len() {
            data[val_field..val_field + tag_size.min(4)].to_vec()
        } else {
            return;
        }
    } else {
        let expected = (read_tiff_u32(data, val_field, bo) as usize).saturating_sub(base);
        if table.has_byte_count_header() {
            match locate_canon_record(data, expected, tag_size, bo) {
                Some(p) => data[p..p + tag_size].to_vec(),
                None => return,
            }
        } else if base_validated && expected + tag_size <= data.len() {
            data[expected..expected + tag_size].to_vec()
        } else {
            return;
        }
    };
    decode_canon_binary(table, table.first_entry(), &sub_raw, bo, tags);
}

/// Decode a Canon `ProcessBinaryData` sub-table (a flat int16 array) by
/// index, starting at `first_entry` (entries below it — e.g. the
/// byte-count header at index 0 — are not tags).
#[cfg(feature = "exiftool-tables")]
fn decode_canon_binary(
    table: revelo_exiftool_tables::CanonSubTable,
    first_entry: u32,
    sub_raw: &[u8],
    bo: &str,
    tags: &mut Vec<TagEntry>,
) {
    use revelo_exiftool_tables::{canon_sub_print_conv, canon_sub_tag_name};
    // Element size follows the table's ExifTool FORMAT (int16 vs int32).
    let sz = table.element_size();
    let entries = sub_raw.len() / sz;
    for idx in (first_entry as usize)..entries {
        let Some(name) = canon_sub_tag_name(table, idx as u32) else {
            continue;
        };
        let raw = if sz == 4 {
            read_tiff_u32(sub_raw, idx * 4, bo) as i32 as i64
        } else {
            read_tiff_u16(sub_raw, idx * 2, bo) as i16 as i64
        };
        let value = canon_sub_print_conv(table, idx as u32, raw)
            .map(str::to_string)
            .unwrap_or_else(|| raw.to_string());
        tags.push((name, value));
    }
}

/// Decode the newer Canon AFInfo2 (0x0026) record. Like AFInfo but with
/// `AFInfoSize` (a self-validating byte-count header) first, an AFAreaMode
/// enum, and four `[N]` arrays plus two bitmasks. The exact size match is
/// the safety gate.
#[cfg(feature = "exiftool-tables")]
fn decode_canon_afinfo2(sub_raw: &[u8], bo: &str, tags: &mut Vec<TagEntry>) {
    use revelo_exiftool_tables::{CanonSubTable, canon_sub_print_conv, canon_sub_tag_name};
    let entries = sub_raw.len() / 2;
    if entries < 8 {
        return;
    }
    let read = |i: usize| read_tiff_u16(sub_raw, i * 2, bo) as i16 as i64;
    let n = read(2); // NumAFPoints
    if !(1..=100).contains(&n) {
        return;
    }
    let n = n as usize;
    let m = n.div_ceil(16);
    // 8 scalars + Widths/Heights/X/Y[N] + InFocus/Selected[M] + 0x000d[M+1] + Primary.
    let expected = 8 + 4 * n + 3 * m + 2;
    if entries != expected {
        return;
    }
    let t = CanonSubTable::AfInfo2;
    for k in 0..8u32 {
        if let Some(name) = canon_sub_tag_name(t, k) {
            let raw = read(k as usize);
            let v = canon_sub_print_conv(t, k, raw)
                .map(str::to_string)
                .unwrap_or_else(|| raw.to_string());
            tags.push((name, v));
        }
    }
    let arr = |start: usize, len: usize| {
        (0..len).map(|j| read(start + j).to_string()).collect::<Vec<_>>().join(" ")
    };
    let mut off = 8;
    for (key, len) in [(8u32, n), (9, n), (10, n), (11, n), (12, m), (13, m)] {
        if let Some(name) = canon_sub_tag_name(t, key) {
            tags.push((name, arr(off, len)));
        }
        off += len;
    }
    off += m + 1; // skip Canon_AFInfo2_0x000d[ceil(N/16)+1]
    if let Some(name) = canon_sub_tag_name(t, 14) {
        tags.push((name, read(off).to_string())); // PrimaryAFPoint
    }
}

/// Decode the older Canon AFInfo (0x0012) record, which is variable-length:
/// 8 fixed int16 fields, then `AFAreaXPositions[N]`, `AFAreaYPositions[N]`,
/// `AFPointsInFocus[ceil(N/16)]` and `PrimaryAFPoint`, where `N` =
/// NumAFPoints (the first field). The exact total-size match is the safety
/// gate: a mis-laid or wrongly-offset record won't match and decodes
/// nothing rather than garbage.
#[cfg(feature = "exiftool-tables")]
fn decode_canon_afinfo(sub_raw: &[u8], bo: &str, tags: &mut Vec<TagEntry>) {
    use revelo_exiftool_tables::{CanonSubTable, canon_sub_tag_name};
    let entries = sub_raw.len() / 2;
    if entries < 8 {
        return;
    }
    let read = |i: usize| read_tiff_u16(sub_raw, i * 2, bo) as i16 as i64;
    let n = read(0); // NumAFPoints
    if !(1..=100).contains(&n) {
        return;
    }
    let n = n as usize;
    let m = n.div_ceil(16); // AFPointsInFocus bitmask words
    let expected = 8 + 2 * n + m + 1;
    // Optional trailing Canon_AFInfo_0x000b[8].
    if entries != expected && entries != expected + 8 {
        return;
    }
    let t = CanonSubTable::AfInfo;
    for k in 0..8u32 {
        if let Some(name) = canon_sub_tag_name(t, k) {
            tags.push((name, read(k as usize).to_string()));
        }
    }
    let arr = |start: usize, len: usize| {
        (0..len).map(|j| read(start + j).to_string()).collect::<Vec<_>>().join(" ")
    };
    if let Some(name) = canon_sub_tag_name(t, 8) {
        tags.push((name, arr(8, n))); // AFAreaXPositions
    }
    if let Some(name) = canon_sub_tag_name(t, 9) {
        tags.push((name, arr(8 + n, n))); // AFAreaYPositions
    }
    if let Some(name) = canon_sub_tag_name(t, 10) {
        tags.push((name, arr(8 + 2 * n, m))); // AFPointsInFocus
    }
    if let Some(name) = canon_sub_tag_name(t, 11) {
        tags.push((name, read(8 + 2 * n + m).to_string())); // PrimaryAFPoint
    }
}

#[cfg(not(feature = "exiftool-tables"))]
fn parse_canon_sub_ifd(data: &[u8], bo: &str, parent_tag_id: u16, tags: &mut Vec<TagEntry>) {
    if data.len() < 2 {
        return;
    }
    let count = read_tiff_u16(data, 0, bo) as usize;
    if count > 50 {
        return;
    }
    let mut pos = 2;

    for _ in 0..count {
        if pos + 12 > data.len() {
            break;
        }
        let sub_tag_id = read_tiff_u16(data, pos, bo);
        let sub_tag_type = read_tiff_u16(data, pos + 2, bo);
        let sub_count = read_tiff_u32(data, pos + 4, bo) as usize;
        let sub_size = exif_type_size(sub_tag_type) * sub_count;
        pos += 8;

        let value = if sub_size <= 4 {
            if sub_tag_type == 2 {
                let end = data[pos..pos + sub_count.min(4)]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(sub_count.min(4));
                String::from_utf8_lossy(&data[pos..pos + end]).to_string()
            } else {
                read_exif_val(data, pos, sub_tag_type, sub_count, bo)
            }
        } else {
            let val_off = read_tiff_u32(data, pos, bo) as usize;
            if val_off + sub_size > data.len() {
                pos += 4;
                continue;
            }
            if sub_tag_type == 2 {
                let end = data[val_off..val_off + sub_count]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(sub_count);
                String::from_utf8_lossy(&data[val_off..val_off + end]).to_string()
            } else {
                read_exif_val(data, val_off, sub_tag_type, sub_count, bo)
            }
        };
        pos += 4;

        if let Some(name) = canon_sub_tag_name(parent_tag_id, sub_tag_id) {
            tags.push((name, value));
        }
    }
}

fn format_lens_spec(value: &str) -> String {
    // LensSpecification (0xA432): 4 rationals: min_focal, max_focal, min_aperture, max_aperture
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() < 4 {
        return value.to_string();
    }
    let min_f = parse_gps_rational(parts[0]).unwrap_or(0.0);
    let max_f = parse_gps_rational(parts[1]).unwrap_or(0.0);
    let min_a = parse_gps_rational(parts[2]).unwrap_or(0.0);
    let max_a = parse_gps_rational(parts[3]).unwrap_or(0.0);
    if max_f == 0.0 || min_a == 0.0 {
        return value.to_string();
    }
    let focal =
        if min_f == max_f { format!("{}mm", min_f) } else { format!("{}-{}mm", min_f, max_f) };
    let aperture =
        if min_a == max_a { format!("f/{}", min_a) } else { format!("f/{}-{}", min_a, max_a) };
    format!("{} {}", focal, aperture)
}

fn format_lens_type(name: &str, type_id: &str) -> String {
    // CanonLensType (0x00F4): map to lens name if available, otherwise return type_id
    let id: u16 = type_id.parse().unwrap_or(u16::MAX);
    let lens_name = match id {
        1 => "Canon EF 50mm f/1.8",
        2 => "Canon EF 28-80mm f/3.5-5.6",
        3 => "Canon EF 28-105mm f/3.5-4.5",
        6 => "Canon EF 35-80mm f/4-5.6",
        23 => "Canon EF 28-200mm f/3.5-5.6",
        26 => "Canon EF 100-400mm f/4.5-5.6L IS",
        28 => "Canon EF 24-105mm f/4L IS",
        29 => "Canon EF 24-70mm f/2.8L",
        30 => "Canon EF 70-200mm f/4L",
        35 => "Canon EF 70-300mm f/4-5.6 IS",
        39 => "Canon EF 24-105mm f/4L II IS",
        48 => "Canon EF 16-35mm f/2.8L III",
        56 => "Canon RF 24-105mm f/4L IS",
        57 => "Canon RF 50mm f/1.2L",
        61 => "Canon RF 35mm f/1.8 Macro",
        65 => "Canon RF 24-70mm f/2.8L IS",
        69 => "Canon RF 15-35mm f/2.8L IS",
        70 => "Canon RF 24-240mm f/4-6.3 IS",
        71 => "Canon RF 70-200mm f/2.8L IS",
        73 => "Canon RF 85mm f/1.2L",
        75 => "Canon RF 100-500mm f/4.5-7.1L IS",
        78 => "Canon RF 100mm f/2.8L Macro",
        82 => "Canon RF 14-35mm f/4L IS",
        84 => "Canon RF 100-400mm f/5.6-8 IS",
        85 => "Canon RF 24mm f/1.8 Macro",
        86 => "Canon RF 15-30mm f/4.5-6.3 IS",
        87 => "Canon RF 28mm f/2.8",
        90 => "Canon RF 135mm f/1.8L",
        91 => "Canon RF 24-50mm f/4.5-6.3 IS",
        93 => "Canon RF 28-70mm f/2.8L",
        94 => "Canon RFS 55-210mm f/5-7.1 IS",
        95 => "Canon RF 200-800mm f/6.3-9 IS",
        97 => "Canon RF 24-105mm f/2.8L IS Z",
        _ => return format!("{} ({})", name, type_id),
    };
    format!("{} ({})", lens_name, type_id)
}

/// Infer camera RAW format name from Make tag when the current format is TIFF.
/// This catches formats like NEF, ARW, ORF, RW2, PEF that don't have unique magic bytes.
fn infer_raw_format(fa: &mut FileAnalyze) {
    let fmt =
        fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.to_string()).unwrap_or_default();
    if fmt != "TIFF" {
        return;
    }
    let make =
        fa.retrieve(StreamKind::General, 0, "Make").map(|z| z.to_string()).unwrap_or_default();
    let make_upper = make.to_uppercase();
    let raw_fmt = if make_upper.contains("NIKON") {
        "NEF"
    } else if make_upper.contains("SONY") {
        "ARW"
    } else if make_upper.contains("OLYMPUS") || make_upper.contains("OM DIGITAL") {
        "ORF"
    } else if make_upper.contains("PANASONIC") || make_upper.contains("LEICA") {
        "RW2"
    } else if make_upper.contains("PENTAX") || make_upper.contains("RICOH") {
        "PEF"
    } else if make_upper.contains("KODAK") {
        "KDC"
    } else if make_upper.contains("MINOLTA") || make_upper.contains("KONICA") {
        "MRW"
    } else if make_upper.contains("SAMSUNG") {
        "SRW"
    } else if make_upper.contains("DJI") {
        "DNG"
    } else {
        return;
    };
    fa.set_field(StreamKind::General, 0, "Format", raw_fmt);
}

#[cfg(test)]
mod exif_tests {
    use super::*;

    #[test]
    fn exif_parses_tiff_header() {
        // Minimal TIFF header: II (LE), magic 42, IFD offset
        let mut buf = vec![0u8; 16];
        buf[0..2].copy_from_slice(b"II");
        buf[2..4].copy_from_slice(&[42, 0]); // TIFF magic LE
        buf[4..8].copy_from_slice(&[8, 0, 0, 0]); // IFD offset = 8
        buf[8..10].copy_from_slice(&[0, 0]); // 0 entries
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_exif(&mut fa));
    }

    #[test]
    fn exif_parses_make_tag() {
        let mut buf = vec![0u8; 64];
        buf[0..2].copy_from_slice(b"II");
        buf[2..4].copy_from_slice(&[42, 0]);
        buf[4..8].copy_from_slice(&[8, 0, 0, 0]); // IFD offset 8 (LE)
        // 1 IFD entry at offset 8
        buf[8] = 0x01;
        buf[9] = 0x00; // count = 1 (LE u16)
        buf[10] = 0x0F;
        buf[11] = 0x01; // tag 0x010F (LE u16)
        buf[12] = 0x02;
        buf[13] = 0x00; // type = 2 ASCII (LE u16)
        buf[14] = 0x05;
        buf[15] = 0x00;
        buf[16] = 0x00;
        buf[17] = 0x00; // count = 5 (LE u32)
        buf[18..23].copy_from_slice(b"Canon"); // 5 bytes value (fits in 4 bytes inline)
        buf[22] = 0x00;
        buf[23] = 0x00;
        buf[24] = 0x00;
        buf[25] = 0x00; // next IFD offset = 0
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_exif(&mut fa));
    }

    #[test]
    fn xmp_detects_rdf() {
        let xml = br#"<?xpacket begin=""?><x:xmpmeta><rdf:RDF xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Test</dc:title></rdf:RDF></x:xmpmeta>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_xmp(&mut fa));
    }

    #[test]
    fn icc_parses_header() {
        let mut buf = vec![0u8; 256];
        buf[0..4].copy_from_slice(&(256u32.to_be_bytes())); // profile size
        buf[8..12].copy_from_slice(&0x0400_0000u32.to_be_bytes()); // version 4.0.0
        buf[12..16].copy_from_slice(b"mntr"); // display
        buf[16..20].copy_from_slice(b"RGB "); // RGB
        buf[36..40].copy_from_slice(b"acsp"); // mandatory file signature
        buf[128..132].copy_from_slice(&[0, 0, 0, 0]); // 0 tags
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_icc(&mut fa));
        let icc = fa.streams().stream(StreamKind::Icc, 0).expect("icc stream");
        assert_eq!(icc.get("ProfileClass").map(|z| z.as_str()), Some("Display Device Profile"));
        assert_eq!(icc.get("ProfileVersion").map(|z| z.as_str()), Some("4.0.0"));
    }

    #[test]
    fn canon_makernote_parses_raw_ifd() {
        // Raw IFD (no TIFF header): entry count followed by entries.
        // CanonModelName (0x0094) with inline 4-byte ASCII value "EOS\0"
        // CanonSerialNumber (0x0015) with inline LONG value 0x0A0B0C0D
        let mut mn = vec![0u8; 32];
        // 2 entries
        mn[0..2].copy_from_slice(&[2, 0]);
        // Entry 0: CanonModelName 0x0094, type=2, count=4, inline="EOS\0"
        mn[2..4].copy_from_slice(&[0x94, 0x00]); // tag
        mn[4..6].copy_from_slice(&[2, 0]); // type ASCII
        mn[6..10].copy_from_slice(&[4, 0, 0, 0]); // count
        mn[10..14].copy_from_slice(b"EOS\0"); // inline value
        // Entry 1: CanonSerialNumber 0x000C, type=4, count=1, inline=0x0A0B0C0D
        mn[14..16].copy_from_slice(&[0x0C, 0x00]); // tag
        mn[16..18].copy_from_slice(&[4, 0]); // type LONG
        mn[18..22].copy_from_slice(&[1, 0, 0, 0]); // count
        mn[22..26].copy_from_slice(&[0x0D, 0x0C, 0x0B, 0x0A]); // inline LE value
        // next IFD offset = 0
        mn[26..30].copy_from_slice(&[0, 0, 0, 0]);

        let mut tags = Vec::new();
        parse_canon_makernote(&mn, 0, &mut tags);
        assert_eq!(tags.len(), 2);
        assert!(tags.iter().any(|(k, v)| *k == "CanonModelName" && v == "EOS"));
        // 0x0A0B0C0D LE = 168496141. Name is "CanonSerialNumber" by default,
        // aliased to "SerialNumber" under `exiftool-tables`.
        assert!(tags.iter().any(|(k, v)| {
            (*k == "CanonSerialNumber" || *k == "SerialNumber") && v == "168496141"
        }));
    }

    #[test]
    fn canon_makernote_parses_tiff_header_ifd() {
        // Canon MakerNote with standard TIFF header: II + 0x2A + IFD offset
        let mut mn = vec![0u8; 64];
        mn[0..2].copy_from_slice(b"II");
        mn[2..4].copy_from_slice(&[42, 0]); // magic 42 LE
        mn[4..8].copy_from_slice(&[8, 0, 0, 0]); // IFD at offset 8
        // IFD at 8: 1 entry
        mn[8..10].copy_from_slice(&[1, 0]); // 1 entry
        // Entry: CanonImageType 0x0006, type=2, count=10, value at offset
        mn[10..12].copy_from_slice(&[6, 0]); // tag 0x0006
        mn[12..14].copy_from_slice(&[2, 0]); // type ASCII
        mn[14..18].copy_from_slice(&[10, 0, 0, 0]); // count = 10
        mn[18..22].copy_from_slice(&[40, 0, 0, 0]); // value offset = 40
        mn[22..26].copy_from_slice(&[0, 0, 0, 0]); // next IFD = 0
        // String data at offset 40
        mn[40..50].copy_from_slice(b"CR2 JPEG\0\0");

        let mut tags = Vec::new();
        parse_canon_makernote(&mn, 0, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "CanonImageType" && v == "CR2 JPEG"));
    }

    #[test]
    fn compute_gps_decimal_from_rational() {
        let mut tags = vec![
            ("GPSLatitudeRef", "S".into()),
            ("GPSLatitude", "40/1, 42/1, 455/10".into()),
            ("GPSLongitudeRef", "W".into()),
            ("GPSLongitude", "74/1, 0/1, 214/10".into()),
        ];
        compute_gps_decimal(&mut tags);
        let lat_dec = tags.iter().find(|(k, _)| *k == "GPSLatitudeDecimal").unwrap();
        let lon_dec = tags.iter().find(|(k, _)| *k == "GPSLongitudeDecimal").unwrap();
        // 40 + 42/60 + 45.5/3600 = -40.712639
        assert_eq!(lat_dec.1, "-40.712639");
        // -(74 + 0/60 + 21.4/3600) = -74.005944
        assert_eq!(lon_dec.1, "-74.005944");
    }

    #[test]
    fn compute_gps_decimal_north_east() {
        let mut tags = vec![
            ("GPSLatitudeRef", "N".into()),
            ("GPSLatitude", "48, 51, 30".into()),
            ("GPSLongitudeRef", "E".into()),
            ("GPSLongitude", "2, 17, 40".into()),
        ];
        compute_gps_decimal(&mut tags);
        let lat_dec = tags.iter().find(|(k, _)| *k == "GPSLatitudeDecimal").unwrap();
        let lon_dec = tags.iter().find(|(k, _)| *k == "GPSLongitudeDecimal").unwrap();
        // 48 + 51/60 + 30/3600 = 48.858333
        assert_eq!(lat_dec.1, "48.858333");
        // 2 + 17/60 + 40/3600 = 2.294444
        assert_eq!(lon_dec.1, "2.294444");
    }

    #[test]
    fn compute_gps_decimal_no_tags() {
        let mut tags: Vec<TagEntry> = Vec::new();
        compute_gps_decimal(&mut tags);
        assert!(tags.is_empty());
    }

    #[test]
    fn jpeg_com_parses_marker() {
        let mut buf = vec![0u8; 256];
        buf[0] = 0xFF;
        buf[1] = 0xD8; // SOI
        buf[2] = 0xFF;
        buf[3] = 0xFE; // COM marker
        let comment = b"Test JPEG Comment!";
        let data_len = comment.len() + 2; // length includes 2-byte length field
        buf[4] = ((data_len >> 8) & 0xFF) as u8;
        buf[5] = (data_len & 0xFF) as u8;
        buf[6..6 + comment.len()].copy_from_slice(comment);
        let after_com = 6 + comment.len();
        buf[after_com] = 0xFF;
        buf[after_com + 1] = 0xDA; // SOS
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_jpeg_com(&mut fa));
    }

    #[test]
    fn jpeg_com_rejects_non_jpeg() {
        let buf = b"Not a JPEG";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_jpeg_com(&mut fa));
    }

    #[test]
    fn jpeg_com_skips_empty_comments() {
        // Build: SOI + COM(len=4, data="") + SOS
        // COM data after length bytes: just 2 null bytes → empty after trim
        let raw = vec![0xFF, 0xD8, 0xFF, 0xFE, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xDA];
        let mut fa = FileAnalyze::new(&raw);
        assert!(!parse_jpeg_com(&mut fa));
    }

    #[test]
    fn png_text_parses_texh_chunk() {
        let mut buf = vec![0u8; 64];
        buf[0..8].copy_from_slice(b"\x89PNG\r\n\x1A\n");
        let payload = b"Description\0A test image";
        let chunk_len = payload.len() as u32;
        buf[8..12].copy_from_slice(&chunk_len.to_be_bytes());
        buf[12..16].copy_from_slice(b"tEXt");
        buf[16..16 + payload.len()].copy_from_slice(payload);
        let crc_off = 16 + payload.len();
        buf[crc_off..crc_off + 4].copy_from_slice(&[0; 4]);
        let iend = crc_off + 4;
        buf[iend..iend + 4].copy_from_slice(&[0, 0, 0, 0]);
        buf[iend + 4..iend + 8].copy_from_slice(b"IEND");
        buf[iend + 8..iend + 12].copy_from_slice(&[0; 4]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_png_text(&mut fa));
    }

    #[test]
    fn png_text_rejects_non_png() {
        let buf = b"Not a PNG";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_png_text(&mut fa));
    }

    #[test]
    fn png_text_skips_unknown_chunks() {
        // PNG with only IHDR and IEND — no text chunks
        let mut buf = vec![0u8; 48];
        buf[0..8].copy_from_slice(b"\x89PNG\r\n\x1A\n");
        // IHDR
        buf[8..12].copy_from_slice(&[0, 0, 0, 13]); // length 13
        buf[12..16].copy_from_slice(b"IHDR");
        // IHDR data: minimal
        buf[16..20].copy_from_slice(&[0, 0, 0, 1]); // width 1
        buf[20..24].copy_from_slice(&[0, 0, 0, 1]); // height 1
        buf[24] = 8;
        buf[25] = 2;
        buf[26] = 0;
        buf[27] = 0;
        buf[28] = 0; // rest
        let crc_off = 29;
        buf[crc_off..crc_off + 4].copy_from_slice(&[0; 4]); // crc
        // IEND
        let iend_off = crc_off + 4;
        buf[iend_off..iend_off + 4].copy_from_slice(&[0, 0, 0, 0]); // len 0
        buf[iend_off + 4..iend_off + 8].copy_from_slice(b"IEND");
        buf[iend_off + 8..iend_off + 12].copy_from_slice(&[0; 4]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_png_text(&mut fa));
    }

    #[test]
    fn canon_makernote_handles_unknown_tags() {
        // Raw IFD with a tag ID we don't know about (0xFFFF)
        let mut mn = vec![0u8; 20];
        mn[0..2].copy_from_slice(&[1, 0]); // 1 entry
        mn[2..4].copy_from_slice(&[0xFF, 0xFF]); // unknown tag
        mn[4..6].copy_from_slice(&[4, 0]); // type LONG
        mn[6..10].copy_from_slice(&[1, 0, 0, 0]); // count
        mn[10..14].copy_from_slice(&[42, 0, 0, 0]); // value

        let mut tags = Vec::new();
        parse_canon_makernote(&mn, 0, &mut tags);
        assert_eq!(tags.len(), 0); // unknown tag silently skipped
    }

    #[test]
    fn canon_main_tag_ids_match_exiftool() {
        // Regression for the off-by-tag-id cluster: 0x000C is the serial
        // number, 0x0010 the model id, 0x0013 the thumbnail valid area,
        // 0x001C the date-stamp mode, 0x0028 the image unique id.
        assert_eq!(canon_tag_name(0x000C), Some("CanonSerialNumber"));
        assert_eq!(canon_tag_name(0x0010), Some("CanonModelID"));
        assert_eq!(canon_tag_name(0x0013), Some("CanonThumbnailValidArea"));
        assert_eq!(canon_tag_name(0x001C), Some("DateStampMode"));
        assert_eq!(canon_tag_name(0x0028), Some("ImageUniqueID"));
        // Model-name PrintConv applies to 0x0010, not 0x000C.
        assert_eq!(canon_format_value(0x000C, "12345"), "12345");
        // ImageUniqueID renders its 16 bytes as lowercase hex.
        assert_eq!(
            canon_format_value(0x0028, "84, 232, 178, 181, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255"),
            "54e8b2b50000000000000000000000ff"
        );
    }

    #[test]
    fn canon_makernote_first_tag_zero_is_not_misread_as_offset() {
        // Regression: when the first maker-note tag id is 0x0000 (as on the
        // Canon IXUS), the bare IFD's leading bytes were misread as an IFD
        // offset, so nothing parsed. The IFD must be read at offset 0.
        let mut mn = vec![0u8; 2 + 6 * 12 + 4]; // count(2) + 6 entries + next(4)
        mn[0..2].copy_from_slice(&[6, 0]); // count = 6 (old bug: u32([6,0,0,0]) = 6)
        // entry 0: tag 0x0000 (unknown) — left as zeros, makes the leading
        // u32 equal the entry count and trip the old heuristic.
        // entry 1: tag 0x0006 CanonImageType, ASCII "Cam".
        let e1 = 2 + 12;
        mn[e1..e1 + 2].copy_from_slice(&[0x06, 0x00]);
        mn[e1 + 2..e1 + 4].copy_from_slice(&[2, 0]); // ASCII
        mn[e1 + 4..e1 + 8].copy_from_slice(&[4, 0, 0, 0]); // count 4
        mn[e1 + 8..e1 + 12].copy_from_slice(b"Cam\0");
        let mut tags = Vec::new();
        parse_canon_makernote(&mn, 0, &mut tags);
        assert!(
            tags.iter().any(|(k, v)| *k == "CanonImageType" && v == "Cam"),
            "IFD misdetected; got {tags:?}"
        );
    }

    // With the ExifTool tables, a well-formed CameraSettings record decodes
    // by index into named, value-converted tags. (The offset-locate is
    // tested implicitly: a correct record validates its byte-count header.)
    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn canon_camerasettings_decodes_named_values() {
        // int16s LE: a[0]=byte count (8), a[1]=2 (MacroMode=Normal),
        // a[2]=0, a[3]=3 (Quality=Fine).
        let sub_raw = [0x08, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00];
        let mut tags = Vec::new();
        decode_canon_binary(
            revelo_exiftool_tables::CanonSubTable::CameraSettings,
            1,
            &sub_raw,
            "LE",
            &mut tags,
        );
        assert!(
            tags.iter().any(|(k, v)| *k == "MacroMode" && v == "Normal"),
            "expected MacroMode=Normal in {tags:?}"
        );
        assert!(
            tags.iter().any(|(k, v)| *k == "Quality" && v == "Fine"),
            "expected Quality=Fine in {tags:?}"
        );
    }

    // locate_canon_record refuses a mis-based offset (no garbage) but
    // accepts a well-formed one.
    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn canon_locate_validates_byte_count_header() {
        // record of 8 bytes at offset 4; header a[0]=8 matches tag_size.
        let data = [0xff, 0xff, 0xff, 0xff, 0x08, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00];
        assert_eq!(locate_canon_record(&data, 4, 8, "LE"), Some(4));
        // wrong expected offset whose header != tag_size -> rejected.
        assert_eq!(locate_canon_record(&data, 0, 8, "LE"), None);
    }

    #[test]
    fn nikon_makernote_parses_raw_ifd_be() {
        // Nikon raw IFD (no TIFF header, big-endian default).
        // First 6 bytes: "Nikon\0" header, then raw IFD.
        let mut mn = vec![0u8; 40];
        mn[0..6].copy_from_slice(b"Nikon\0");
        // 2 entries
        mn[6..8].copy_from_slice(&[0, 2]); // count = 2 (BE)
        // Entry 0: NikonShutterCount 0x00A0, type=4 LONG, count=1
        mn[8..10].copy_from_slice(&[0, 0xA0]); // tag 0x00A0 (BE)
        mn[10..12].copy_from_slice(&[0, 4]); // type LONG
        mn[12..16].copy_from_slice(&[0, 0, 0, 1]); // count = 1
        mn[16..20].copy_from_slice(&[0, 0, 0x1A, 0x2B]); // value = 6699 (BE)
        // Entry 1: NikonISOSpeed 0x0002, type=3 SHORT, count=1
        mn[20..22].copy_from_slice(&[0, 2]); // tag
        mn[22..24].copy_from_slice(&[0, 3]); // type SHORT
        mn[24..28].copy_from_slice(&[0, 0, 0, 1]); // count
        mn[28..30].copy_from_slice(&[0, 200]); // value = 200 (BE)
        mn[30..34].copy_from_slice(&[0, 0, 0, 0]); // next IFD = 0 (4 bytes)

        let mut tags = Vec::new();
        parse_nikon_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 2);
        // Bespoke names; the feature prefers the ExifTool names.
        #[cfg(not(feature = "exiftool-tables"))]
        {
            assert!(tags.iter().any(|(k, v)| *k == "NikonShutterCount" && v == "6699"));
            assert!(tags.iter().any(|(k, v)| *k == "NikonISOSpeed" && v == "200"));
        }
    }

    #[test]
    fn nikon_makernote_parses_tiff_header() {
        // Nikon with TIFF header: "Nikon\0" + MM + 0x002A + IFD offset
        // IFD offset 14 is relative to sub[4..8] (after Nikon\0), so IFD starts at data[20]
        let mut mn = vec![0u8; 64];
        mn[0..6].copy_from_slice(b"Nikon\0");
        mn[6..8].copy_from_slice(b"MM"); // big-endian in sub[0..2]
        mn[8..10].copy_from_slice(&[0, 42]); // magic 42 BE in sub[2..4]
        mn[10..14].copy_from_slice(&[0, 0, 0, 14]); // IFD offset at sub[4..8] = 14
        // IFD at sub[14] = data[6+14] = data[20]
        mn[20..22].copy_from_slice(&[0, 1]); // 1 entry
        // Entry: NikonModelID 0x0080, type=7 UNDEFINED, count=4
        mn[22..24].copy_from_slice(&[0, 0x80]); // tag
        mn[24..26].copy_from_slice(&[0, 7]); // type UNDEFINED
        mn[26..30].copy_from_slice(&[0, 0, 0, 4]); // count
        mn[30..34].copy_from_slice(b"\x01\x02\x03\x04"); // inline value
        mn[34..38].copy_from_slice(&[0, 0, 0, 0]); // next IFD = 0

        let mut tags = Vec::new();
        parse_nikon_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        #[cfg(not(feature = "exiftool-tables"))]
        assert!(tags.iter().any(|(k, _)| *k == "NikonModelID"));
    }

    #[test]
    fn nikon_makernote_no_header() {
        // Nikon without "Nikon\0" prefix - raw IFD
        let mut mn = vec![0u8; 16];
        mn[0..2].copy_from_slice(&[0, 1]); // count = 1 (BE)
        mn[2..4].copy_from_slice(&[0, 0x85]); // tag 0x0085 NikonSerialNumber
        mn[4..6].copy_from_slice(&[0, 2]); // type ASCII
        mn[6..10].copy_from_slice(&[0, 0, 0, 3]); // count = 3
        mn[10..14].copy_from_slice(b"ABC\0"); // inline

        let mut tags = Vec::new();
        parse_nikon_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        // Default uses the bespoke "NikonSerialNumber"; the feature prefers
        // the ExifTool name ("SerialNumber").
        #[cfg(not(feature = "exiftool-tables"))]
        assert!(tags.iter().any(|(k, v)| *k == "NikonSerialNumber" && v == "ABC"));
    }

    #[test]
    fn sony_makernote_parses_tiff_header() {
        // Sony with TIFF header (LE)
        let mut mn = vec![0u8; 64];
        mn[0..2].copy_from_slice(b"II");
        mn[2..4].copy_from_slice(&[42, 0]);
        mn[4..8].copy_from_slice(&[8, 0, 0, 0]); // IFD at offset 8
        // IFD at 8: 1 entry
        mn[8..10].copy_from_slice(&[1, 0]); // 1 entry (LE)
        // Entry: SonyQuality 0x0102, type=3 SHORT, count=1
        mn[10..12].copy_from_slice(&[0x02, 0x01]); // tag 0x0102 (LE)
        mn[12..14].copy_from_slice(&[3, 0]); // type SHORT
        mn[14..18].copy_from_slice(&[1, 0, 0, 0]); // count = 1
        mn[18..20].copy_from_slice(&[2, 0]); // value = 2 (LE)
        mn[20..24].copy_from_slice(&[0, 0, 0, 0]); // next IFD = 0

        let mut tags = Vec::new();
        parse_sony_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "SonyQuality" && v == "2"));
    }

    #[test]
    fn sony_makernote_with_sony_dsc_header() {
        // Sony with "SONY DSC" header (8 bytes)
        // IFD offset 16 is relative to sub (data after header), so IFD at data[24]
        let mut mn = vec![0u8; 64];
        mn[0..8].copy_from_slice(b"SONY DSC");
        mn[8..10].copy_from_slice(b"MM"); // BE in sub[0..2]
        mn[10..12].copy_from_slice(&[0, 42]); // magic in sub[2..4]
        mn[12..16].copy_from_slice(&[0, 0, 0, 16]); // IFD offset in sub[4..8] = 16
        // IFD at sub[16] = data[8+16] = data[24]
        mn[24..26].copy_from_slice(&[0, 1]); // 1 entry (BE)
        // Entry: SonyColorSpace 0x0117, type=3 SHORT, count=1
        mn[26..28].copy_from_slice(&[0x01, 0x17]); // tag (BE)
        mn[28..30].copy_from_slice(&[0, 3]); // type SHORT
        mn[30..34].copy_from_slice(&[0, 0, 0, 1]); // count
        mn[34..36].copy_from_slice(&[0, 1]); // value = 1 (sRGB, BE)
        mn[36..40].copy_from_slice(&[0, 0, 0, 0]); // next IFD = 0

        let mut tags = Vec::new();
        parse_sony_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "SonyColorSpace" && v == "sRGB"));
    }

    #[test]
    fn nikon_format_color_space() {
        assert_eq!(nikon_format_value(0x0086, "1"), "sRGB");
        assert_eq!(nikon_format_value(0x0086, "2"), "Adobe RGB");
        assert_eq!(nikon_format_value(0x0086, "999"), "999");
    }

    #[test]
    fn nikon_format_model_id() {
        let formatted = nikon_format_value(0x0080, "8388658");
        assert_eq!(formatted, "D850");
        let formatted = nikon_format_value(0x0080, "8388664");
        assert_eq!(formatted, "D6");
        let formatted = nikon_format_value(0x0080, "8388666");
        assert_eq!(formatted, "Z 7");
        let formatted = nikon_format_value(0x0080, "99999999");
        assert_eq!(formatted, "99999999");
    }

    #[test]
    fn nikon_format_lens_type() {
        let formatted = nikon_format_value(0x000D, "15");
        assert_eq!(formatted, "MF D G VR");
        let formatted = nikon_format_value(0x000D, "8");
        assert_eq!(formatted, "VR");
        let formatted = nikon_format_value(0x000D, "0");
        assert_eq!(formatted, "0");
    }

    #[test]
    fn sony_format_color_space() {
        assert_eq!(sony_format_value(0x0117, "1"), "sRGB");
        assert_eq!(sony_format_value(0x0117, "2"), "Adobe RGB");
        assert_eq!(sony_format_value(0x0117, "999"), "999");
    }

    #[test]
    fn sony_format_model_id() {
        let formatted = sony_format_value(0xB000, "31");
        assert_eq!(formatted, "DSLR-NEX-F3");
        let formatted = sony_format_value(0xB000, "41");
        assert_eq!(formatted, "ILCE-7");
        let formatted = sony_format_value(0xB000, "99999");
        assert_eq!(formatted, "99999");
    }

    #[test]
    fn format_lens_spec_zoom() {
        let result = format_lens_spec("24/1,70/1,4/1,4/1");
        assert_eq!(result, "24-70mm f/4");
    }

    #[test]
    fn format_lens_spec_prime() {
        let result = format_lens_spec("50/1,50/1,18/10,18/10");
        assert_eq!(result, "50mm f/1.8");
    }

    #[test]
    fn format_lens_spec_insufficient_parts() {
        let result = format_lens_spec("50/1,50/1");
        assert_eq!(result, "50/1,50/1");
    }

    #[test]
    fn format_lens_spec_zoom_aperture() {
        let result = format_lens_spec("100/1,400/1,45/10,56/10");
        assert_eq!(result, "100-400mm f/4.5-5.6");
    }

    // Legacy IFD-style sub-table parsing; under `exiftool-tables` the
    // binary record is decoded by the ExifTool path instead (see
    // canon_camerasettings_decodes_named_values).
    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn canon_sub_ifd_parses_camera_settings() {
        let sub_data: Vec<u8> = [
            3, 0, 11, 0, 3, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 3, 0, 1, 0, 0, 0, 1, 0, 0, 0, 13, 0,
            3, 0, 1, 0, 0, 0, 2, 0, 0, 0,
        ]
        .to_vec();
        let sub_len = sub_data.len() as u32;
        let mn_size = 18 + sub_data.len();
        let mut mn = vec![0u8; mn_size];
        mn[0..2].copy_from_slice(&[1, 0]);
        mn[2..4].copy_from_slice(&[1, 0]);
        mn[4..6].copy_from_slice(&[7, 0]);
        mn[6..10].copy_from_slice(&sub_len.to_le_bytes());
        let val_off: u32 = 14;
        mn[10..14].copy_from_slice(&val_off.to_le_bytes());
        let next_off = 14 + sub_data.len();
        if next_off + 4 <= mn.len() {
            mn[next_off..next_off + 4].copy_from_slice(&[0, 0, 0, 0]);
        }
        mn[14..14 + sub_data.len()].copy_from_slice(&sub_data);
        let mut tags = Vec::new();
        parse_canon_makernote(&mn, 0, &mut tags);
        assert_eq!(tags.len(), 4);
        assert!(tags.iter().any(|(k, _)| *k == "CanonCameraSettings"));
        assert!(tags.iter().any(|(k, v)| *k == "Contrast" && v == "0"));
        assert!(tags.iter().any(|(k, v)| *k == "Saturation" && v == "1"));
        assert!(tags.iter().any(|(k, v)| *k == "Sharpness" && v == "2"));
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn olympus_makernote_parses_raw_ifd() {
        let mut mn = vec![0u8; 48];
        mn[0..6].copy_from_slice(b"OLYMP\0");
        mn[6..8].copy_from_slice(&[0, 1]);
        mn[8..10].copy_from_slice(&[0x02, 0x00]);
        mn[10..12].copy_from_slice(&[0, 4]);
        mn[12..16].copy_from_slice(&[0, 0, 0, 1]);
        mn[16..20].copy_from_slice(&[0, 0, 0, 42]);
        mn[20..24].copy_from_slice(&[0, 0, 0, 0]);
        let mut tags = Vec::new();
        parse_olympus_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "OlympusSpecialMode" && v == "42"));
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn olympus_makernote_skips_without_header() {
        let mut mn = vec![0u8; 24];
        mn[0..2].copy_from_slice(&[0, 1]);
        mn[2..4].copy_from_slice(&[0x02, 0x01]);
        mn[4..6].copy_from_slice(&[0, 4]);
        mn[6..10].copy_from_slice(&[0, 0, 0, 1]);
        mn[10..14].copy_from_slice(&[0, 0, 0, 99]);
        mn[14..18].copy_from_slice(&[0, 0, 0, 0]);
        let mut tags = Vec::new();
        parse_olympus_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "OlympusQuality" && v == "99"));
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn panasonic_makernote_parses_raw_ifd() {
        // Panasonic MakerNote: "Panasonic\0\0\0" (12 bytes) + raw IFD at offset 12 (LE default)
        let mut mn = vec![0u8; 48];
        mn[0..12].copy_from_slice(b"Panasonic\0\0\0");
        // IFD at offset 12: raw IFD (LE)
        mn[12..14].copy_from_slice(&[1, 0]); // count = 1 (LE)
        // Entry: PanasonicQuality 0x0001, type=3 SHORT, count=1
        mn[14..16].copy_from_slice(&[1, 0]); // tag 0x0001 (LE)
        mn[16..18].copy_from_slice(&[3, 0]); // type SHORT
        mn[18..22].copy_from_slice(&[1, 0, 0, 0]); // count = 1
        mn[22..24].copy_from_slice(&[5, 0]); // value = 5 (LE)
        mn[24..28].copy_from_slice(&[0, 0, 0, 0]); // next IFD = 0

        let mut tags = Vec::new();
        parse_panasonic_makernote(&mn, 0, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "PanasonicQuality" && v == "5"));
    }

    #[test]
    fn pentax_makernote_parses_aoc_header() {
        // Pentax MakerNote: "AOC\0" + raw IFD (BE default)
        let mut mn = vec![0u8; 36];
        mn[0..4].copy_from_slice(b"AOC\0");
        // IFD at offset 4: raw IFD (BE)
        mn[4..6].copy_from_slice(&[0, 1]); // count = 1
        // Entry: PentaxModelType 0x0001, type=3 SHORT, count=1
        mn[6..8].copy_from_slice(&[0, 1]); // tag (BE)
        mn[8..10].copy_from_slice(&[0, 3]); // type SHORT
        mn[10..14].copy_from_slice(&[0, 0, 0, 1]); // count
        mn[14..16].copy_from_slice(&[0, 3]); // value = 3 (BE)
        mn[16..20].copy_from_slice(&[0, 0, 0, 0]); // next IFD

        let mut tags = Vec::new();
        parse_pentax_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "PentaxModelType" && v == "3"));
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn pentax_makernote_parses_without_header() {
        // Pentax without header — raw IFD (BE default)
        let mut mn = vec![0u8; 24];
        mn[0..2].copy_from_slice(&[0, 1]); // count = 1 (BE)
        mn[2..4].copy_from_slice(&[0, 3]); // tag 0x0003 PentaxQuality
        mn[4..6].copy_from_slice(&[0, 4]); // type LONG
        mn[6..10].copy_from_slice(&[0, 0, 0, 1]); // count
        mn[10..14].copy_from_slice(&[0, 0, 0, 7]); // value = 7

        let mut tags = Vec::new();
        parse_pentax_makernote(&mn, &mut tags);
        assert!(tags.iter().any(|(k, v)| *k == "PentaxQuality" && v == "7"));
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn fujifilm_makernote_parses_header() {
        // Fujifilm MakerNote: "FUJIFILM" + 4-byte IFD offset at byte 8.
        let mut mn = vec![0u8; 64];
        mn[0..8].copy_from_slice(b"FUJIFILM");
        mn[8..12].copy_from_slice(&[16, 0, 0, 0]); // IFD offset = 16 (LE)
        // Raw IFD at offset 16.
        mn[16..18].copy_from_slice(&[1, 0]); // count = 1 (LE)
        mn[18..20].copy_from_slice(&[0x00, 0x10]); // tag 0x1000 (Quality)
        mn[20..22].copy_from_slice(&[3, 0]); // type SHORT
        mn[22..26].copy_from_slice(&[1, 0, 0, 0]); // count = 1
        mn[26..28].copy_from_slice(&[2, 0]); // value = 2
        mn[30..34].copy_from_slice(&[0, 0, 0, 0]); // next IFD = 0

        let mut tags = Vec::new();
        parse_fujifilm_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        #[cfg(not(feature = "exiftool-tables"))]
        assert!(tags.iter().any(|(k, v)| *k == "FujiQuality" && v == "2"));
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn fujifilm_makernote_uses_ifd_offset() {
        // Fujifilm: "FUJIFILM" + 4-byte IFD offset at byte 8 (no version).
        let mut mn = vec![0u8; 64];
        mn[0..8].copy_from_slice(b"FUJIFILM");
        mn[8..12].copy_from_slice(&[12, 0, 0, 0]); // IFD offset = 12 (LE)
        // Raw IFD at offset 12: count(2) + entry(12) + next(4).
        mn[12..14].copy_from_slice(&[1, 0]); // count = 1
        mn[14..16].copy_from_slice(&[0x01, 0x10]); // tag 0x1001 (Sharpness)
        mn[16..18].copy_from_slice(&[3, 0]); // type SHORT
        mn[18..22].copy_from_slice(&[1, 0, 0, 0]); // count
        mn[22..24].copy_from_slice(&[3, 0]); // value = 3
        mn[26..30].copy_from_slice(&[0, 0, 0, 0]); // next IFD

        let mut tags = Vec::new();
        parse_fujifilm_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1); // IFD-offset parsing worked
        // Default uses the bespoke "FujiSharpness"; the feature uses the
        // ExifTool name/PrintConv (Sharpness).
        #[cfg(not(feature = "exiftool-tables"))]
        assert!(tags.iter().any(|(k, v)| *k == "FujiSharpness" && v == "3"));
    }

    #[test]
    fn nikon_makernote_uses_formatted_values() {
        // Build a Nikon MakerNote that triggers value formatting.
        // Tag 0x0086 NikonColorSpace with value 2 should become "Adobe RGB"
        let mut mn = vec![0u8; 36];
        mn[0..6].copy_from_slice(b"Nikon\0");
        mn[6..8].copy_from_slice(&[0, 1]); // count = 1 (BE)
        mn[8..10].copy_from_slice(&[0, 0x86]); // tag 0x0086 (BE)
        mn[10..12].copy_from_slice(&[0, 3]); // type SHORT
        mn[12..16].copy_from_slice(&[0, 0, 0, 1]); // count
        mn[16..18].copy_from_slice(&[0, 2]); // value = 2 (Adobe RGB, BE)
        mn[18..22].copy_from_slice(&[0, 0, 0, 0]); // next IFD

        let mut tags = Vec::new();
        parse_nikon_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        #[cfg(not(feature = "exiftool-tables"))]
        assert!(tags.iter().any(|(k, v)| *k == "NikonColorSpace" && v == "Adobe RGB"));
    }

    #[test]
    fn sony_makernote_uses_formatted_values() {
        // Build a Sony MakerNote that triggers value formatting.
        // Tag 0x0117 SonyColorSpace with value 2 should become "Adobe RGB"
        let mut mn = vec![0u8; 36];
        mn[0..2].copy_from_slice(b"II");
        mn[2..4].copy_from_slice(&[42, 0]); // magic
        mn[4..8].copy_from_slice(&[8, 0, 0, 0]); // IFD at offset 8
        mn[8..10].copy_from_slice(&[1, 0]); // 1 entry (LE)
        mn[10..12].copy_from_slice(&[0x17, 0x01]); // tag 0x0117 (LE)
        mn[12..14].copy_from_slice(&[3, 0]); // type SHORT
        mn[14..18].copy_from_slice(&[1, 0, 0, 0]); // count
        mn[18..20].copy_from_slice(&[2, 0]); // value = 2 (Adobe RGB)
        mn[20..24].copy_from_slice(&[0, 0, 0, 0]); // next IFD

        let mut tags = Vec::new();
        parse_sony_makernote(&mn, &mut tags);
        assert_eq!(tags.len(), 1);
        assert!(tags.iter().any(|(k, v)| *k == "SonyColorSpace" && v == "Adobe RGB"));
    }

    // ---------- Vendor MakerNotes (round-trip extraction) ----------
    //
    // These cover the parsers that dispatch from `parse_makernote` but had
    // no direct coverage: Samsung, Apple, GoPro, DJI, Google, Leica,
    // Sigma, Minolta, Casio, FLIR. Each builds a bare single-entry TIFF
    // IFD (no II/MM header, no vendor prefix → the offset-0 path) carrying
    // an inline ASCII value, and asserts the parser maps the tag id to its
    // name and extracts the value. ASCII payloads are byte-order
    // independent, so only the IFD header is encoded per the parser's
    // default byte order (BE for Leica, LE for the rest).

    /// Bare single-entry TIFF IFD: count(2) + 12-byte entry + next-IFD(4).
    /// The entry is an ASCII value (type 2, ≤4 bytes, stored inline).
    fn mn_ifd(le: bool, tag: u16, ascii: &[u8]) -> Vec<u8> {
        assert!(ascii.len() <= 4, "inline ASCII must be ≤4 bytes");
        let u16b = |x: u16| if le { x.to_le_bytes() } else { x.to_be_bytes() };
        let u32b = |x: u32| if le { x.to_le_bytes() } else { x.to_be_bytes() };
        let mut v = Vec::new();
        v.extend_from_slice(&u16b(1)); // entry count
        v.extend_from_slice(&u16b(tag)); // tag id
        v.extend_from_slice(&u16b(2)); // type 2 = ASCII
        v.extend_from_slice(&u32b(ascii.len() as u32)); // value count
        let mut val = [0u8; 4];
        val[..ascii.len()].copy_from_slice(ascii);
        v.extend_from_slice(&val); // inline value field
        v.extend_from_slice(&u32b(0)); // next IFD = 0
        v
    }

    fn assert_mn(tags: &[TagEntry], key: &str, value: &str) {
        assert!(
            tags.iter().any(|(k, v)| *k == key && v == value),
            "expected ({key}={value}) in {tags:?}"
        );
    }

    #[test]
    fn samsung_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_samsung_makernote(&mn_ifd(true, 0x0036, b"1.00"), &mut tags);
        assert_mn(&tags, "SamsungLensFirmware", "1.00");
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn apple_makernote_extracts_named_tag() {
        // Raw-IFD fallback path: ContentIdentifier (0x0011) ASCII.
        let mut tags = Vec::new();
        parse_apple_makernote(&mn_ifd(true, 0x0011, b"ABCD"), &mut tags);
        assert_mn(&tags, "ContentIdentifier", "ABCD");
    }

    #[test]
    fn apple_ios_header_makernote_parses_and_formats() {
        // The real Apple layout: "Apple iOS\0" + version + "MM" (big-endian) +
        // an IFD whose value offsets are relative to the block start.
        let mut v = Vec::new();
        v.extend_from_slice(b"Apple iOS\0"); // 10 bytes
        v.extend_from_slice(&[0x00, 0x01]); // version
        v.extend_from_slice(b"MM"); // big-endian
        // IFD: 2 entries — MakerNoteVersion (0x0001 SLONG=12) and
        // CameraType (0x002e SLONG=1 → "Back Normal").
        v.extend_from_slice(&2u16.to_be_bytes()); // entry count
        v.extend_from_slice(&0x0001u16.to_be_bytes());
        v.extend_from_slice(&9u16.to_be_bytes()); // type SLONG
        v.extend_from_slice(&1u32.to_be_bytes());
        v.extend_from_slice(&12u32.to_be_bytes());
        v.extend_from_slice(&0x002eu16.to_be_bytes());
        v.extend_from_slice(&9u16.to_be_bytes());
        v.extend_from_slice(&1u32.to_be_bytes());
        v.extend_from_slice(&1u32.to_be_bytes());
        v.extend_from_slice(&0u32.to_be_bytes()); // next IFD

        let mut tags = Vec::new();
        parse_apple_makernote(&v, &mut tags);
        format_apple_makernote(&mut tags);
        assert_mn(&tags, "MakerNoteVersion", "12");
        assert_mn(&tags, "CameraType", "Back Normal");
    }

    #[test]
    fn gopro_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_gopro_makernote(&mn_ifd(true, 0x0001, b"H11"), &mut tags);
        assert_mn(&tags, "GoProModelName", "H11");
    }

    #[test]
    fn dji_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_dji_makernote(&mn_ifd(true, 0x0002, b"FC30"), &mut tags);
        assert_mn(&tags, "DJIModel", "FC30");
    }

    #[test]
    fn google_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_google_makernote(&mn_ifd(true, 0x0001, b"1"), &mut tags);
        assert_mn(&tags, "GoogleMotionPhoto", "1");
    }

    #[test]
    fn leica_makernote_extracts_named_tag() {
        // Leica defaults to big-endian.
        let mut tags = Vec::new();
        parse_leica_makernote(&mn_ifd(false, 0x0003, b"AB12"), &mut tags);
        assert_mn(&tags, "LeicaSerial", "AB12");
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn sigma_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_sigma_makernote(&mn_ifd(true, 0x0003, b"1.00"), &mut tags);
        assert_mn(&tags, "SigmaFirmware", "1.00");
    }

    #[test]
    fn minolta_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_minolta_makernote(&mn_ifd(true, 0x0015, b"25"), 0, &mut tags);
        assert_mn(&tags, "MinoltaLensType", "25");
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn casio_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_casio_makernote(&mn_ifd(true, 0x0002, b"Fine"), &mut tags);
        assert_mn(&tags, "CasioQuality", "Fine");
    }

    #[test]
    #[cfg(not(feature = "exiftool-tables"))]
    fn flir_makernote_extracts_named_tag() {
        let mut tags = Vec::new();
        parse_flir_makernote(&mn_ifd(true, 0x0003, b"0.95"), &mut tags);
        assert_mn(&tags, "FlirEmissivity", "0.95");
    }

    /// Build a bare single-entry LE IFD carrying one SHORT value.
    #[cfg(feature = "exiftool-tables")]
    fn mn_ifd_short(tag: u16, value: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&1u16.to_le_bytes()); // entry count
        v.extend_from_slice(&tag.to_le_bytes());
        v.extend_from_slice(&3u16.to_le_bytes()); // type 3 = SHORT
        v.extend_from_slice(&1u32.to_le_bytes()); // count
        v.extend_from_slice(&value.to_le_bytes());
        v.extend_from_slice(&[0, 0]); // pad value field to 4 bytes
        v.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        v
    }

    // With the feature on, the ExifTool tables supply both a richer tag
    // name and PrintConv value decoding that the hand-written tables lack.
    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn exiftool_tables_add_name_and_decode_enum() {
        // Apple 0x000a = HDRImageType, value 3 → "HDR Image". The
        // hand-written apple_tag_name has no 0x000a entry at all, so this
        // only resolves via the ExifTool table.
        let mut tags = Vec::new();
        parse_apple_makernote(&mn_ifd_short(0x000a, 3), &mut tags);
        assert!(
            tags.iter().any(|(k, v)| *k == "HDRImageType" && v == "HDR Image"),
            "expected decoded Apple HDRImageType in {tags:?}"
        );
    }

    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn canon_afinfo_variable_length_decodes() {
        // AFInfo with N=2: 8 fixed int16 + X[2] + Y[2] + InFocus[1] + Primary.
        // Total = 8 + 2 + 2 + 1 + 1 = 14 int16 = 28 bytes.
        let v: [i16; 14] = [2, 2, 100, 80, 0, 0, 0, 0, -10, 10, 5, 5, 0, 1];
        let mut sub = Vec::new();
        for x in v {
            sub.extend_from_slice(&x.to_le_bytes());
        }
        let mut tags = Vec::new();
        decode_canon_afinfo(&sub, "LE", &mut tags);
        let get = |k: &str| tags.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("NumAFPoints"), Some("2"));
        assert_eq!(get("AFAreaXPositions"), Some("-10 10"));
        assert_eq!(get("AFAreaYPositions"), Some("5 5"));
        assert_eq!(get("PrimaryAFPoint"), Some("1"));
    }

    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn canon_afinfo2_variable_length_decodes() {
        // AFInfo2/3 N=2: 8 scalars + Widths/Heights/X/Y[2] + InFocus/Selected[1]
        // + 0x000d[2] + Primary = 8 + 8 + 2 + 2 + 1 = 21 int16. AFInfoSize=42.
        let v: [i16; 21] = [
            42, 4, 2, 2, 100, 80, 50, 40, // AFInfoSize, AFAreaMode(Auto), N, Valid, …
            11, 12, // AFAreaWidths[2]
            13, 14, // AFAreaHeights[2]
            -5, 5, // AFAreaXPositions[2]
            -3, 3, // AFAreaYPositions[2]
            0, // AFPointsInFocus[1]
            0, // AFPointsSelected[1]
            0, 0, // 0x000d[ceil(N/16)+1 = 2] (skipped)
            7, // PrimaryAFPoint
        ];
        let mut sub = Vec::new();
        for x in v {
            sub.extend_from_slice(&x.to_le_bytes());
        }
        let mut tags = Vec::new();
        decode_canon_afinfo2(&sub, "LE", &mut tags);
        let get = |k: &str| tags.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("NumAFPoints"), Some("2"));
        assert_eq!(get("AFAreaMode"), Some("Auto")); // PrintConv 4 => Auto
        assert_eq!(get("AFAreaWidths"), Some("11 12"));
        assert_eq!(get("AFAreaXPositions"), Some("-5 5"));
    }

    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn canon_int32_subtable_decodes() {
        // AspectInfo is int32u, FIRST_ENTRY 0: AspectRatio, CroppedImage{W,H,L,T}.
        use revelo_exiftool_tables::CanonSubTable;
        let v: [u32; 5] = [0, 4608, 3456, 0, 0]; // AspectRatio 0 => "3:2"
        let mut sub = Vec::new();
        for x in v {
            sub.extend_from_slice(&x.to_le_bytes());
        }
        let mut tags = Vec::new();
        decode_canon_binary(CanonSubTable::AspectInfo, 0, &sub, "LE", &mut tags);
        let get = |k: &str| tags.iter().find(|(n, _)| *n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("CroppedImageWidth"), Some("4608"));
        assert_eq!(get("CroppedImageHeight"), Some("3456"));
        assert_eq!(get("AspectRatio"), Some("3:2")); // PrintConv 0 => 3:2
    }

    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn canon_afinfo_rejects_size_mismatch() {
        // Same N=2 but a truncated record must decode nothing (no garbage).
        let v: [i16; 10] = [2, 2, 100, 80, 0, 0, 0, 0, -10, 10];
        let mut sub = Vec::new();
        for x in v {
            sub.extend_from_slice(&x.to_le_bytes());
        }
        let mut tags = Vec::new();
        decode_canon_afinfo(&sub, "LE", &mut tags);
        assert!(tags.is_empty(), "size mismatch must decode nothing: {tags:?}");
    }

    #[cfg(feature = "exiftool-tables")]
    #[test]
    fn exiftool_tables_leave_raw_value_when_no_printconv() {
        // Apple 0x0008 = AccelerationVector has a name but no integer
        // PrintConv; the raw value must pass through unchanged.
        let mut tags = Vec::new();
        parse_apple_makernote(&mn_ifd_short(0x0008, 42), &mut tags);
        assert!(
            tags.iter().any(|(k, v)| *k == "AccelerationVector" && v == "42"),
            "expected raw AccelerationVector value in {tags:?}"
        );
    }

    // ---------- IPTC IIM ----------

    fn build_iim(pairs: &[(u8, u8, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        for &(record, dataset, data) in pairs {
            buf.push(0x1C);
            buf.push(record);
            buf.push(dataset);
            if data.len() < 128 {
                buf.push(data.len() as u8);
            } else {
                let len = data.len();
                buf.push(0x80 | (len >> 8) as u8);
                buf.push(len as u8);
            }
            buf.extend_from_slice(data);
        }
        buf
    }

    /// Wrap raw IIM datasets in a minimal Photoshop Image Resource Block
    /// (APP13 "Photoshop 3.0" identifier + 8BIM resource 0x0404), the
    /// only container `parse_iim` accepts.
    fn build_irb(pairs: &[(u8, u8, &[u8])]) -> Vec<u8> {
        let iim = build_iim(pairs);
        let mut buf = b"Photoshop 3.0\0".to_vec();
        buf.extend_from_slice(b"8BIM");
        buf.extend_from_slice(&[0x04, 0x04]); // resource id 0x0404 = IPTC-NAA
        buf.extend_from_slice(&[0x00, 0x00]); // empty Pascal name, padded to even
        buf.extend_from_slice(&(iim.len() as u32).to_be_bytes());
        buf.extend_from_slice(&iim);
        if iim.len() % 2 == 1 {
            buf.push(0x00); // resource data padded to even length
        }
        buf
    }

    #[test]
    fn iim_parses_basic_fields() {
        let buf = build_irb(&[
            (2, 5, b"Sunset"),
            (2, 80, b"John Doe"),
            (2, 90, b"Paris"),
            (2, 101, b"France"),
            (2, 116, b"2024 Acme Corp"),
            (2, 120, b"A beautiful sunset"),
        ]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ObjectName").map(|z| z.as_str()),
            Some("Sunset")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Byline").map(|z| z.as_str()),
            Some("John Doe")
        );
        assert_eq!(fa.retrieve(StreamKind::Iptc, 0, "City").map(|z| z.as_str()), Some("Paris"));
        assert_eq!(fa.retrieve(StreamKind::Iptc, 0, "Country").map(|z| z.as_str()), Some("France"));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Copyright").map(|z| z.as_str()),
            Some("2024 Acme Corp")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Description").map(|z| z.as_str()),
            Some("A beautiful sunset")
        );
    }

    #[test]
    fn iim_aggregates_keywords() {
        let buf = build_irb(&[(2, 25, b"travel"), (2, 25, b"sunset"), (2, 25, b"landscape")]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Keyword").map(|z| z.as_str()),
            Some("travel / sunset / landscape")
        );
    }

    #[test]
    fn iim_parses_envelope_record() {
        let buf = build_irb(&[
            (1, 30, b"JFIF"), // IIMFileFormat
            (1, 40, b"1.02"), // IIMFileVersion
            (1, 50, b"AP"),   // ServiceIdentifier
            (1, 90, b"3"),    // EnvelopePriority
        ]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "IIMFileFormat").map(|z| z.as_str()),
            Some("JFIF")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "IIMFileVersion").map(|z| z.as_str()),
            Some("1.02")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ServiceIdentifier").map(|z| z.as_str()),
            Some("AP")
        );
    }

    #[test]
    fn iim_parses_additional_fields() {
        let buf = build_irb(&[
            (2, 10, b"2"),                  // Urgency
            (2, 15, b"SCI"),                // Category
            (2, 55, b"2024-01-15"),         // DateCreated
            (2, 60, b"14:30:00"),           // TimeCreated
            (2, 65, b"Photoshop"),          // OriginatingProgram
            (2, 85, b"Staff Photographer"), // BylineTitle
            (2, 95, b"California"),         // ProvinceState
            (2, 100, b"US"),                // CountryCode
            (2, 105, b"Amazing View"),      // Headline
            (2, 110, b"Jane Smith"),        // Credit
            (2, 115, b"Acme Wire"),         // Source
            (2, 118, b"editor@acme.com"),   // Contact
        ]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(fa.retrieve(StreamKind::Iptc, 0, "Urgency").map(|z| z.as_str()), Some("2"));
        assert_eq!(fa.retrieve(StreamKind::Iptc, 0, "Category").map(|z| z.as_str()), Some("SCI"));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "DateCreated").map(|z| z.as_str()),
            Some("2024-01-15")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "TimeCreated").map(|z| z.as_str()),
            Some("14:30:00")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "OriginatingProgram").map(|z| z.as_str()),
            Some("Photoshop")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "BylineTitle").map(|z| z.as_str()),
            Some("Staff Photographer")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ProvinceState").map(|z| z.as_str()),
            Some("California")
        );
        assert_eq!(fa.retrieve(StreamKind::Iptc, 0, "CountryCode").map(|z| z.as_str()), Some("US"));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Headline").map(|z| z.as_str()),
            Some("Amazing View")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Credit").map(|z| z.as_str()),
            Some("Jane Smith")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Source").map(|z| z.as_str()),
            Some("Acme Wire")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Contact").map(|z| z.as_str()),
            Some("editor@acme.com")
        );
    }

    #[test]
    fn iim_handles_2byte_size() {
        let long_val = vec![b'A'; 200];
        let buf = build_irb(&[(2, 80, &long_val)]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        let expected = String::from_utf8_lossy(&long_val).trim_end().to_string();
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Byline").map(|z| z.as_str()),
            Some(expected.as_str())
        );
    }

    #[test]
    fn iim_rejects_non_iim_data() {
        let buf = b"Not IIM data at all";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_iim(&mut fa));
    }

    #[test]
    fn iim_skips_unknown_datasets() {
        // dataset 255 is not in our table
        let buf = build_irb(&[(2, 255, b"ignored"), (2, 5, b"Title")]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ObjectName").map(|z| z.as_str()),
            Some("Title")
        );
        // only the known dataset maps; unknown is skipped
        assert!(fa.retrieve(StreamKind::Iptc, 0, "ObjectName").is_some());
    }

    #[test]
    fn iim_parses_digital_creation_fields() {
        let buf = build_irb(&[
            (2, 62, b"2024-01-15"),     // DigitalCreationDate
            (2, 63, b"14:30:00+01:00"), // DigitalCreationTime
            (2, 70, b"25.0"),           // ProgramVersion
            (2, 75, b"a"),              // ObjectCycle
        ]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "DigitalCreationDate").map(|z| z.as_str()),
            Some("2024-01-15")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "DigitalCreationTime").map(|z| z.as_str()),
            Some("14:30:00+01:00")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ProgramVersion").map(|z| z.as_str()),
            Some("25.0")
        );
    }

    #[test]
    fn iim_trims_trailing_whitespace() {
        let buf = build_irb(&[(2, 5, b"Sunset  \t\n")]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ObjectName").map(|z| z.as_str()),
            Some("Sunset")
        );
    }

    #[test]
    fn iim_with_extra_noise_before_marker() {
        // Junk (e.g. preceding JPEG segments) before the Photoshop IRB
        // anchor must be skipped, not parsed.
        let mut buf = b"\xff\xd8\xff\xe0 random preceding bytes \x1c\x02\x69\x04junk".to_vec();
        buf.extend_from_slice(&build_irb(&[(2, 5, b"NoiseTest")]));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ObjectName").map(|z| z.as_str()),
            Some("NoiseTest")
        );
    }

    #[test]
    fn iim_rejects_unframed_marker_bytes() {
        // Regression: stray IIM marker bytes (as found in compressed image
        // data) with NO Photoshop IRB container must not be parsed —
        // otherwise random 0x1C runs surface as gibberish IPTC fields.
        let mut buf = vec![0xff, 0xd8];
        buf.extend_from_slice(&build_iim(&[(2, 105, b"\x80\x12\x9f\x00garbage")]));
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_iim(&mut fa));
        assert!(fa.retrieve(StreamKind::Iptc, 0, "Headline").is_none());
    }

    #[test]
    fn exif_tag_0x83bb_routes_to_iim_parser() {
        // Build a TIFF with IFD0 containing tag 0x83BB (IPTC/NAA).
        // The tag value points to raw IIM data at a separate offset.
        let iim_data =
            build_iim(&[(2, 5, b"ExifIPTC"), (2, 80, b"Tester"), (2, 120, b"Via 0x83BB")]);
        let iim_len = iim_data.len() as u32;

        // TIFF header (LE) + IFD0
        let mut buf = vec![0u8; 8 + 12 + 4 + iim_len as usize]; // header(8) + entry(12) + next(4) + iim_data
        buf[0..2].copy_from_slice(b"II");
        buf[2..4].copy_from_slice(&[42, 0]);
        buf[4..8].copy_from_slice(&[8, 0, 0, 0]); // IFD0 offset = 8 (LE)

        buf[8] = 0x01; // 1 IFD entry
        buf[9] = 0x00; // (LE u16 count)

        // Entry: tag=0x83BB, type=7 (undefined), count=iim_len, value offset
        buf[10..12].copy_from_slice(&[0xBB, 0x83]); // tag 0x83BB (LE)
        buf[12..14].copy_from_slice(&[7, 0]); // type 7 = undefined (LE)
        buf[14..18].copy_from_slice(&iim_len.to_le_bytes()); // count (LE)
        // value is > 4 bytes, so this is an offset pointer
        let val_off: u32 = 8 + 12 + 4; // after IFD0 header: 8 (header) + count(2) + entry(12) + next_ifd(4)
        buf[18..22].copy_from_slice(&val_off.to_le_bytes());

        // next IFD = 0
        let next_off = 8 + 2 + 12;
        buf[next_off..next_off + 4].copy_from_slice(&[0, 0, 0, 0]);

        // IIM data at val_off
        let iim_off = val_off as usize;
        buf[iim_off..iim_off + iim_data.len()].copy_from_slice(&iim_data);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_exif(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "ObjectName").map(|z| z.as_str()),
            Some("ExifIPTC")
        );
        assert_eq!(fa.retrieve(StreamKind::Iptc, 0, "Byline").map(|z| z.as_str()), Some("Tester"));
        assert_eq!(
            fa.retrieve(StreamKind::Iptc, 0, "Description").map(|z| z.as_str()),
            Some("Via 0x83BB")
        );
    }

    #[test]
    fn tag_parsers_do_not_reintroduce_full_raw_scans() {
        let source = include_str!("tags.rs");
        for forbidden in [
            concat!("peek_raw(", "fa.remain())"),
            concat!("read_raw(", "fa.remain())"),
            concat!("peek_raw(", "remain)"),
            concat!("read_raw(", "remain)"),
            concat!("peek_raw_at(0, ", "fa.element_size())"),
        ] {
            assert!(!source.contains(forbidden), "forbidden full-scan pattern: {forbidden}");
        }
    }
}
