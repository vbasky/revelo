use revelo_core::{FileAnalyze, StreamKind};

type TagEntry = (&'static str, String);

fn fill_tags(fa: &mut FileAnalyze, tags: &[TagEntry]) {
    if tags.is_empty() {
        return;
    }
    let pos = fa.stream_prepare(StreamKind::General);
    for (key, value) in tags {
        fa.set_field(StreamKind::General, pos, key, value.as_str());
    }
}

// ---------- ID3v1 ----------

pub fn parse_id3v1(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.remain();
    if remain < 128 {
        return None;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;
    let len = buf.len();
    if len < 128 {
        return None;
    }
    let start = len - 128;
    let tag = &buf[start..];
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

    fill_tags(fa, &tags);
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
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;
    if buf.len() < 10 {
        return None;
    }
    if &buf[0..3] != b"ID3" {
        return None;
    }

    let version_major = buf[3];
    let flags = buf[5];
    let size = synch_safe_int(&buf[6..10]);
    if size == 0 || size as usize + 10 > buf.len() {
        return None;
    }

    let has_footer = (flags & 0x10) != 0;
    let total_size = size as usize + 10 + if has_footer { 10 } else { 0 };
    if buf.len() < total_size {
        return None;
    }

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

    fill_tags(fa, &tags);
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
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;

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

    fill_tags(fa, &tags);
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
    fill_tags(fa, &tags);
}

// ---------- Lyrics3 ----------

pub fn parse_lyrics3(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.remain();
    if remain < 20 {
        return None;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;

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

    fill_tags(fa, &tags);
    Some((end - start) as u32)
}

// ---------- Generic ----------

#[cfg(test)]
mod tests {
    use super::*;

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
}

// ---------- EXIF ----------

/// Parse a TIFF/EXIF IFD from raw bytes at the given byte offset.
/// Returns the list of tag entries and the offset to the next IFD.
pub fn parse_exif(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 8 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 8 {
        return false;
    }

    let byte_order = if &buf[0..2] == b"II" {
        "LE"
    } else if &buf[0..2] == b"MM" {
        "BE"
    } else {
        return false;
    };

    let ifd_offset = read_tiff_u32(&buf, 4, byte_order) as usize;
    let mut tags: Vec<TagEntry> = Vec::new();
    read_exif_ifd(&buf, ifd_offset, byte_order, &mut tags);
    fill_tags(fa, &tags);
    true
}

fn read_exif_ifd(data: &[u8], offset: usize, bo: &str, tags: &mut Vec<TagEntry>) {
    if offset + 2 > data.len() {
        return;
    }
    let count = read_tiff_u16(data, offset, bo) as usize;
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
        let value: String;
        if tag_size <= 4 {
            value = if tag_type == 2 {
                // ASCII
                let end = data[pos..pos + tag_count.min(4)]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count.min(4));
                String::from_utf8_lossy(&data[pos..pos + end]).to_string()
            } else {
                read_exif_number(data, pos, tag_type, bo).to_string()
            };
            pos += 4;
        } else {
            let val_offset = read_tiff_u32(data, pos, bo) as usize;
            pos += 4;
            if val_offset + tag_size > data.len() {
                continue;
            }
            value = if tag_type == 2 {
                let end = data[val_offset..val_offset + tag_count]
                    .iter()
                    .position(|&b| b == 0)
                    .unwrap_or(tag_count);
                String::from_utf8_lossy(&data[val_offset..val_offset + end]).to_string()
            } else {
                read_exif_number(data, val_offset, tag_type, bo).to_string()
            };
        }

        match tag_id {
            0x010E => tags.push(("Image_Description", value)),
            0x010F => tags.push(("Make", value)),
            0x0110 => tags.push(("Model", value)),
            0x0112 => tags.push(("Orientation", value)),
            0x011A => tags.push(("Density_X", value)),
            0x011B => tags.push(("Density_Y", value)),
            0x0128 => tags.push(("Density_Unit", value)),
            0x0131 => tags.push(("Software", value)),
            0x0132 => tags.push(("Encoded_Date", value)),
            0x013B => tags.push(("Artist", value)),
            0x013E => tags.push(("WhitePoint", value)),
            0x8298 => tags.push(("Copyright", value)),
            0x829A => tags.push(("ExposureTime", value)),
            0x829D => tags.push(("FNumber", value)),
            0x8822 => tags.push(("ExposureProgram", value)),
            0x8827 => tags.push(("ISOSpeed", value)),
            0x9003 => tags.push(("DateTimeOriginal", value)),
            0x9004 => tags.push(("DateTimeDigitized", value)),
            0x9204 => tags.push(("ExposureBias", value)),
            0x9209 => tags.push(("Flash", value)),
            0x920A => tags.push(("FocalLength", value)),
            0xA402 => tags.push(("GPSAltitude", value)),
            _ => {}
        }
    }
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

fn read_exif_number(data: &[u8], off: usize, t: u16, bo: &str) -> i64 {
    if off + 8 > data.len() {
        return 0;
    }
    match (t, bo) {
        (3, "BE") => u16::from_be_bytes([data[off], data[off + 1]]) as i64,
        (3, "LE") => u16::from_le_bytes([data[off], data[off + 1]]) as i64,
        (4, "BE") => {
            u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]) as i64
        }
        (4, "LE") => {
            u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]) as i64
        }
        (5, _) => {
            let num = if bo == "BE" {
                i32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
            } else {
                i32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
            };
            num as i64
        }
        _ => 0,
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

// ---------- XMP ----------

pub fn parse_xmp(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };

    let text = std::str::from_utf8(&buf).unwrap_or("");
    if !text.contains("xmpmeta") || !text.contains("rdf:RDF") {
        return false;
    }

    let mut tags: Vec<TagEntry> = Vec::new();
    extract_xmp_fields(text, &mut tags);
    fill_tags(fa, &tags);
    true
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
    let remain = fa.remain();
    if remain < 128 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 128 {
        return false;
    }

    let profile_size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if profile_size == 0 || profile_size > buf.len() {
        return false;
    }

    let _preferred_cmm = read_icc_u32(&buf, 4);
    let _version = read_icc_u32(&buf, 8);
    let device_class = std::str::from_utf8(&buf[12..16]).unwrap_or("");
    let color_space = std::str::from_utf8(&buf[16..20]).unwrap_or("");
    let _pcs = std::str::from_utf8(&buf[20..24]).unwrap_or("");

    let tag_count = u32::from_be_bytes([buf[128], buf[129], buf[130], buf[131]]) as usize;
    let mut tags: Vec<TagEntry> = Vec::new();
    let mut desc = String::new();

    let mut pos = 132;
    for _ in 0..tag_count.min(50) {
        if pos + 12 > buf.len() {
            break;
        }
        let _tag_sig = &buf[pos..pos + 4];
        let tag_offset =
            u32::from_be_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]) as usize;
        let tag_size =
            u32::from_be_bytes([buf[pos + 8], buf[pos + 9], buf[pos + 10], buf[pos + 11]]) as usize;
        pos += 12;

        if tag_offset + tag_size > buf.len() {
            continue;
        }
        if &buf[pos - 12..pos - 8] == b"desc" {
            let tag_data = &buf[tag_offset..tag_offset + tag_size.saturating_sub(12)];
            desc = String::from_utf8_lossy(tag_data).trim_end_matches('\0').to_string();
        }
    }

    tags.push(("ICC_Profile", format!("{} ({})", color_space, device_class)));
    if !desc.is_empty() {
        tags.push(("ICC_Description", desc));
    }

    fill_tags(fa, &tags);
    true
}

fn read_icc_u32(data: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
}

// ---------- C2PA ----------

pub fn parse_c2pa(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 16 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };

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
        fill_tags(fa, &tags);
        true
    } else {
        false
    }
}

// ---------- IIM / IPTC ----------

pub fn parse_iim(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 4 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };

    // IIM starts with 0x1C marker
    if !buf.windows(3).any(|w| w == [0x1C, 0x00, 0x02]) {
        return false;
    }

    let mut tags: Vec<TagEntry> = Vec::new();
    let mut pos = 0;

    while pos + 4 < buf.len() {
        if buf[pos] != 0x1C {
            pos += 1;
            continue;
        }
        let record = buf[pos + 1];
        let dataset = buf[pos + 2];
        let size = u16::from_be_bytes([buf[pos + 3], buf[pos + 4]]) as usize;
        pos += 5;
        if pos + size > buf.len() {
            break;
        }
        let value = String::from_utf8_lossy(&buf[pos..pos + size]).trim_end().to_string();
        pos += size;

        if record == 2 {
            match dataset {
                5 => tags.push(("Title", value)),
                25 => tags.push(("Keyword", value)),
                55 => tags.push(("Encoded_Date", value)),
                80 => tags.push(("Author", value)),
                85 => tags.push(("Author_Title", value)),
                90 => tags.push(("City", value)),
                101 => tags.push(("Country", value)),
                116 => tags.push(("Copyright", value)),
                118 => tags.push(("Contact", value)),
                120 => tags.push(("Description", value)),
                _ => {}
            }
        }
    }

    fill_tags(fa, &tags);
    true
}

// ---------- PropertyList (Apple plist) ----------

pub fn parse_property_list(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    let text = std::str::from_utf8(&buf).unwrap_or("");
    if !text.contains("<!DOCTYPE plist") && !text.contains("<plist") {
        return false;
    }

    let mut tags: Vec<TagEntry> = Vec::new();
    extract_plist_fields(text, &mut tags);
    fill_tags(fa, &tags);
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
    let remain = fa.remain();
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    let text = std::str::from_utf8(&buf).unwrap_or("");
    if !text.contains("SphericalVideo") && !text.contains("ProjectionType") {
        return false;
    }

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
        fill_tags(fa, &tags);
        true
    } else {
        false
    }
}

// ---------- Update parse_tags ----------

pub fn parse_tags(fa: &mut FileAnalyze) -> bool {
    let _ = parse_id3v1(fa);
    let _ = parse_id3v2(fa);
    let _ = parse_ape_tag(fa);
    let _ = parse_lyrics3(fa);
    let _ = parse_exif(fa);
    let _ = parse_xmp(fa);
    let _ = parse_icc(fa);
    let _ = parse_c2pa(fa);
    let _ = parse_iim(fa);
    let _ = parse_property_list(fa);
    let _ = parse_spherical_video(fa);
    true
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
        buf[12..16].copy_from_slice(b"mntr"); // display
        buf[16..20].copy_from_slice(b"RGB "); // RGB
        buf[128..132].copy_from_slice(&[0, 0, 0, 0]); // 0 tags
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_icc(&mut fa));
    }
}
