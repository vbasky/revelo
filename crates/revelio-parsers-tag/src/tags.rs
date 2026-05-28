use revelio_core::{FileAnalyze, StreamKind};

type TagEntry = (&'static str, String);

fn fill_tags(fa: &mut FileAnalyze, tags: &[TagEntry]) {
    if tags.is_empty() { return; }
    let pos = fa.Stream_Prepare(StreamKind::General);
    for (key, value) in tags {
        fa.Fill(StreamKind::General, pos, key, value.as_str(), false);
    }
}

// ---------- ID3v1 ----------

pub fn parse_id3v1(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.Remain() as usize;
    if remain < 128 { return None; }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;
    let len = buf.len();
    if len < 128 { return None; }
    let start = len - 128;
    let tag = &buf[start..];
    if &tag[0..3] != b"TAG" { return None; }

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
    if !title.is_empty() { tags.push(("Title", title)); }
    if !artist.is_empty() { tags.push(("Performer", artist)); }
    if !album.is_empty() { tags.push(("Album", album)); }
    if !year.is_empty() { tags.push(("Recorded_Date", year)); }
    if !comment.is_empty() {
        if comment.contains("ExactAudioCopy") {
            tags.push(("Encoded_Application", comment));
        } else {
            tags.push(("Comment", comment));
        }
    }
    if let Some(t) = track { tags.push(("Track_Position", t.to_string())); }

    let genre = tag[127];
    if genre < 80 { tags.push(("Genre", id3v1_genre(genre).to_string())); }

    fill_tags(fa, &tags);
    Some(128)
}

fn trim_str(data: &[u8]) -> String {
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).trim_end().to_string()
}

fn id3v1_genre(idx: u8) -> &'static str {
    match idx {
        0 => "Blues", 1 => "Classic Rock", 2 => "Country", 3 => "Dance",
        4 => "Disco", 5 => "Funk", 6 => "Grunge", 7 => "Hip-Hop",
        8 => "Jazz", 9 => "Metal", 10 => "New Age", 11 => "Oldies",
        12 => "Other", 13 => "Pop", 14 => "R&B", 15 => "Rap",
        16 => "Reggae", 17 => "Rock", 18 => "Techno", 19 => "Industrial",
        20 => "Alternative", 21 => "Ska", 22 => "Death Metal", 23 => "Pranks",
        24 => "Soundtrack", 25 => "Euro-Techno", 26 => "Ambient", 27 => "Trip-Hop",
        28 => "Vocal", 29 => "Jazz+Funk", 30 => "Fusion", 31 => "Trance",
        32 => "Classical", 33 => "Instrumental", 34 => "Acid", 35 => "House",
        36 => "Game", 37 => "Sound Clip", 38 => "Gospel", 39 => "Noise",
        40 => "Alternative Rock", 41 => "Bass", 42 => "Soul", 43 => "Punk",
        44 => "Space", 45 => "Meditative", 46 => "Instrumental Pop",
        47 => "Instrumental Rock", 48 => "Ethnic", 49 => "Gothic",
        50 => "Darkwave", 51 => "Techno-Industrial", 52 => "Electronic",
        53 => "Pop-Folk", 54 => "Eurodance", 55 => "Dream",
        56 => "Southern Rock", 57 => "Comedy", 58 => "Cult", 59 => "Gangsta",
        60 => "Top 40", 61 => "Christian Rap", 62 => "Pop/Funk",
        63 => "Jungle", 64 => "Native American", 65 => "Cabaret",
        66 => "New Wave", 67 => "Psychadelic", 68 => "Rave",
        69 => "Showtunes", 70 => "Trailer", 71 => "Lo-Fi", 72 => "Tribal",
        73 => "Acid Punk", 74 => "Acid Jazz", 75 => "Polka", 76 => "Retro",
        77 => "Musical", 78 => "Rock & Roll", 79 => "Hard Rock",
        _ => "Unknown",
    }
}

// ---------- ID3v2 ----------

pub fn parse_id3v2(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.Remain() as usize;
    if remain < 10 { return None; }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;
    if buf.len() < 10 { return None; }
    if &buf[0..3] != b"ID3" { return None; }

    let version_major = buf[3];
    let flags = buf[5];
    let size = synch_safe_int(&buf[6..10]);
    if size == 0 || size as usize + 10 > buf.len() { return None; }

    let has_footer = (flags & 0x10) != 0;
    let total_size = size as usize + 10 + if has_footer { 10 } else { 0 };
    if buf.len() < total_size { return None; }

    let mut offset = 10;
    let end = size as usize + 10;
    let mut tags: Vec<TagEntry> = Vec::new();

    while offset + 10 <= end {
        let frame_id_len = if version_major >= 3 { 4 } else { 3 };
        let frame_id = String::from_utf8_lossy(&buf[offset..offset + frame_id_len]).to_string();
        offset += frame_id_len;
        if offset + 4 > end { break; }

        let frame_size = if version_major >= 4 {
            synch_safe_int(&buf[offset..offset + 4]) as usize
        } else {
            u32::from_be_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]]) as usize
        };
        offset += 4;
        if version_major >= 3 { offset += 2; }

        if frame_id.as_bytes() == b"\0\0\0\0" || frame_id.is_empty() { break; }
        if frame_size == 0 || offset + frame_size > end { break; }

        let frame_data = &buf[offset..offset + frame_size];
        parse_id3v2_frame(&mut tags, &frame_id, frame_data);
        offset += frame_size;
    }

    fill_tags(fa, &tags);
    Some(total_size as u32)
}

fn synch_safe_int(bytes: &[u8]) -> u32 {
    let mut val = 0u32;
    for &b in bytes { val = (val << 7) | (b as u32 & 0x7F); }
    val
}

fn parse_id3v2_frame(tags: &mut Vec<TagEntry>, id: &str, data: &[u8]) {
    if data.is_empty() { return; }
    let encoding = data[0];
    let text_start = 1;
    let text = if encoding == 1 || encoding == 2 {
        read_utf16(&data[text_start..], encoding == 2)
    } else {
        String::from_utf8_lossy(&data[text_start..]).trim_end_matches('\0').to_string()
    };
    if text.is_empty() { return; }

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
        let b1 = data[i]; let b2 = data[i + 1];
        let cp = if big_endian { u16::from_be_bytes([b1, b2]) } else { u16::from_le_bytes([b1, b2]) };
        if cp == 0 { break; }
        if let Some(c) = char::from_u32(cp as u32) { result.push(c); }
        i += 2;
    }
    result
}

// ---------- APE tag ----------

pub fn parse_ape_tag(fa: &mut FileAnalyze) -> Option<u32> {
    let remain = fa.Remain() as usize;
    if remain < 32 { return None; }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;

    let footer_start = buf.windows(8).rposition(|w| w == b"APETAGEX")?;
    if footer_start + 32 > buf.len() { return None; }

    let footer = &buf[footer_start..footer_start + 32];
    let item_count = u32::from_le_bytes([footer[12], footer[13], footer[14], footer[15]]) as usize;
    let flags = u32::from_le_bytes([footer[16], footer[17], footer[18], footer[19]]);
    let has_header = (flags & 0x80000000) != 0;
    let tag_start = if has_header && footer_start >= 32 { footer_start - 32 } else { 0 };
    let tag_end = footer_start + 32;
    if tag_start >= tag_end || tag_end > buf.len() { return None; }

    let mut tags: Vec<TagEntry> = Vec::new();
    let mut offset = if has_header { tag_start + 32 } else { tag_start };

    for _ in 0..item_count.min(100) {
        if offset + 8 > tag_end { break; }
        let value_size = u32::from_le_bytes([buf[offset], buf[offset + 1], buf[offset + 2], buf[offset + 3]]) as usize;
        offset += 8;
        if offset >= tag_end { break; }
        let key_end = buf[offset..].iter().position(|&b| b == 0).unwrap_or(tag_end - offset);
        let key = String::from_utf8_lossy(&buf[offset..offset + key_end]).to_uppercase();
        offset += key_end + 1;
        if offset + value_size > tag_end { break; }
        let value = String::from_utf8_lossy(&buf[offset..offset + value_size]).trim_end_matches('\0').to_string();
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
    if *offset + 4 > buf.len() { return tags; }
    let vendor_len = u32::from_le_bytes([buf[*offset], buf[*offset + 1], buf[*offset + 2], buf[*offset + 3]]) as usize;
    *offset += 4 + vendor_len;
    if *offset + 4 > buf.len() { return tags; }
    let count = u32::from_le_bytes([buf[*offset], buf[*offset + 1], buf[*offset + 2], buf[*offset + 3]]) as usize;
    *offset += 4;

    for _ in 0..count.min(200) {
        if *offset + 4 > buf.len() { break; }
        let len = u32::from_le_bytes([buf[*offset], buf[*offset + 1], buf[*offset + 2], buf[*offset + 3]]) as usize;
        *offset += 4;
        if *offset + len > buf.len() { break; }
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
    let remain = fa.Remain() as usize;
    if remain < 20 { return None; }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec())?;

    let start = buf.windows(11).position(|w| w == b"LYRICSBEGIN")?;
    if start + 20 > buf.len() { return None; }

    let mut offset = start + 11;
    let mut end = offset;
    let mut tags: Vec<TagEntry> = Vec::new();

    while offset + 8 < buf.len() {
        if &buf[offset..offset + 5] == b"LYR20" { end = offset; break; }
        let size_str = std::str::from_utf8(&buf[offset + 3..offset + 8]).ok().unwrap_or("0");
        let size: usize = size_str.trim().parse().unwrap_or(0);
        offset += 8;
        if offset + size > buf.len() { break; }
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

pub fn parse_tags(fa: &mut FileAnalyze) -> bool {
    let _ = parse_id3v1(fa);
    let _ = parse_id3v2(fa);
    let _ = parse_ape_tag(fa);
    let _ = parse_lyrics3(fa);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id3v1_parses_basic_tag() {
        let mut buf = vec![0u8; 256];
        let start = 256 - 128;
        buf[start..start + 3].copy_from_slice(b"TAG");
        buf[start + 3..start + 33].copy_from_slice(b"Test Song\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
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
