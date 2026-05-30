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
const EXIF_MAGIC: &[u8; 6] = b"Exif\0\0";

fn find_exif_header(buf: &[u8]) -> Option<usize> {
    buf.windows(EXIF_MAGIC.len())
        .position(|w| w == EXIF_MAGIC.as_slice())
        .map(|p| p + EXIF_MAGIC.len())
}

pub fn parse_exif(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw_at(0, usize::MAX) {
        Some(b) => b.to_vec(),
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
    let mut exif_ptr = None;
    let mut gps_ptr = None;
    let mut interop_ptr = None;

    let next_ifd = parse_ifd(
        tiff_buf,
        ifd0_off,
        bo,
        &mut tags,
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
            parse_makernote(&make, mn_data, &mut tags);
        }

        if let Some(ip) = sub_interop {
            let mut it = Vec::new();
            parse_ifd(
                tiff_buf,
                ip as usize,
                bo,
                &mut it,
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
                &mut None,
                &mut None,
                &mut None,
                &mut None,
                IfdKind::Tiff,
            );
        }
    }

    compute_gps_decimal(&mut tags);
    if let Some(entry) = tags.iter_mut().find(|(k, _)| *k == "LensSpecification") {
        entry.1 = format_lens_spec(&entry.1);
    }
    if let Some(pos) = tags.iter().position(|(k, _)| *k == "CanonLensType") {
        let val = tags[pos].1.clone();
        tags[pos].1 = format_lens_type("CanonLensType", &val);
    }
    fill_tags(fa, &tags);
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
            0x83BB => {
                let iim_data = if tag_size <= 4 {
                    let end = pos + tag_size.min(data.len() - pos);
                    data[pos..end].to_vec()
                } else {
                    let off = read_tiff_u32(data, pos, bo) as usize;
                    let end = off + tag_size.min(data.len() - off);
                    data[off..end].to_vec()
                };
                parse_iim_buf(&iim_data, tags);
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
            tags.push((n, value));
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
            11 => f32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4])).to_string(),
            12 => f64::from_le_bytes(data[off..off + 8].try_into().unwrap_or([0; 8])).to_string(),
            7 => {
                let end = data[off..].iter().take(4).position(|&b| b == 0).unwrap_or(4);
                String::from_utf8_lossy(&data[off..off + end]).to_string()
            }
            _ => read_tiff_u32(data, off, bo).to_string(),
        };
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

    let mut tags: Vec<TagEntry> = Vec::new();
    parse_iim_buf(&buf, &mut tags);
    if tags.is_empty() {
        return false;
    }
    fill_tags(fa, &tags);
    true
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

// ---------- MakerNotes (stub) ----------

fn parse_makernote(_make: &str, _data: &[u8], _tags: &mut Vec<TagEntry>) {
    // Stub — full implementation deferred to a later release.
}

#[cfg(test)]
mod exif_tests {
    use super::*;

    fn build_iim(items: &[(u8, u8, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        for &(record, dataset, data) in items {
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

    #[test]
    fn iim_parses_basic_fields() {
        let buf = build_iim(&[
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
            fa.retrieve(StreamKind::General, 0, "ObjectName").map(|z| z.as_str()),
            Some("Sunset")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Byline").map(|z| z.as_str()),
            Some("John Doe")
        );
        assert_eq!(fa.retrieve(StreamKind::General, 0, "City").map(|z| z.as_str()), Some("Paris"));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Country").map(|z| z.as_str()),
            Some("France")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Copyright").map(|z| z.as_str()),
            Some("2024 Acme Corp")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Description").map(|z| z.as_str()),
            Some("A beautiful sunset")
        );
    }

    #[test]
    fn iim_aggregates_keywords() {
        let buf = build_iim(&[(2, 25, b"travel"), (2, 25, b"sunset"), (2, 25, b"landscape")]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Keyword").map(|z| z.as_str()),
            Some("travel / sunset / landscape")
        );
    }

    #[test]
    fn iim_parses_envelope_record() {
        let buf = build_iim(&[
            (1, 30, b"JFIF"), // IIMFileFormat
            (1, 40, b"1.02"), // IIMFileVersion
            (1, 50, b"AP"),   // ServiceIdentifier
            (1, 90, b"3"),    // EnvelopePriority
        ]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "IIMFileFormat").map(|z| z.as_str()),
            Some("JFIF")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "IIMFileVersion").map(|z| z.as_str()),
            Some("1.02")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "ServiceIdentifier").map(|z| z.as_str()),
            Some("AP")
        );
    }

    #[test]
    fn iim_parses_additional_fields() {
        let buf = build_iim(&[
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
        assert_eq!(fa.retrieve(StreamKind::General, 0, "Urgency").map(|z| z.as_str()), Some("2"));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Category").map(|z| z.as_str()),
            Some("SCI")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "DateCreated").map(|z| z.as_str()),
            Some("2024-01-15")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "TimeCreated").map(|z| z.as_str()),
            Some("14:30:00")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "OriginatingProgram").map(|z| z.as_str()),
            Some("Photoshop")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "BylineTitle").map(|z| z.as_str()),
            Some("Staff Photographer")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "ProvinceState").map(|z| z.as_str()),
            Some("California")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "CountryCode").map(|z| z.as_str()),
            Some("US")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Headline").map(|z| z.as_str()),
            Some("Amazing View")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Credit").map(|z| z.as_str()),
            Some("Jane Smith")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Source").map(|z| z.as_str()),
            Some("Acme Wire")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Contact").map(|z| z.as_str()),
            Some("editor@acme.com")
        );
    }

    #[test]
    fn iim_handles_2byte_size() {
        let long_val = vec![b'A'; 200];
        let buf = build_iim(&[(2, 80, &long_val)]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        let expected = String::from_utf8_lossy(&long_val).trim_end().to_string();
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Byline").map(|z| z.as_str()),
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
        let buf = build_iim(&[(2, 255, b"ignored"), (2, 5, b"Title")]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "ObjectName").map(|z| z.as_str()),
            Some("Title")
        );
        // only the known dataset maps; unknown is skipped
        assert!(fa.retrieve(StreamKind::General, 0, "ObjectName").is_some());
    }

    #[test]
    fn iim_parses_digital_creation_fields() {
        let buf = build_iim(&[
            (2, 62, b"2024-01-15"),     // DigitalCreationDate
            (2, 63, b"14:30:00+01:00"), // DigitalCreationTime
            (2, 70, b"25.0"),           // ProgramVersion
            (2, 75, b"a"),              // ObjectCycle
        ]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "DigitalCreationDate").map(|z| z.as_str()),
            Some("2024-01-15")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "DigitalCreationTime").map(|z| z.as_str()),
            Some("14:30:00+01:00")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "ProgramVersion").map(|z| z.as_str()),
            Some("25.0")
        );
    }

    #[test]
    fn iim_trims_trailing_whitespace() {
        let buf = build_iim(&[(2, 5, b"Sunset  \t\n")]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "ObjectName").map(|z| z.as_str()),
            Some("Sunset")
        );
    }

    #[test]
    fn iim_with_extra_noise_before_marker() {
        let mut buf = b"Photoshop 3.0\x00\x00\x00\x00".to_vec();
        buf.extend_from_slice(&build_iim(&[(2, 5, b"NoiseTest")]));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_iim(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "ObjectName").map(|z| z.as_str()),
            Some("NoiseTest")
        );
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
            fa.retrieve(StreamKind::General, 0, "ObjectName").map(|z| z.as_str()),
            Some("ExifIPTC")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Byline").map(|z| z.as_str()),
            Some("Tester")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Description").map(|z| z.as_str()),
            Some("Via 0x83BB")
        );
    }
}
