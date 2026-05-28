//! MPEG Audio (MP1/MP2/MP3) parser — sync-based frame stream.
//!
//! Mirrors the subset of MediaInfoLib's `File_Mpega.cpp` needed for
//! plain MP3 files. No chunked container — instead a stream of frames
//! each starting with an 11-bit sync word. May be preceded by an ID3v2
//! tag and/or contain a Xing/Info/VBRI metadata frame at the start.
//!
//! Layout:
//!   [ID3v2 header + tag bytes]?
//!   frame*
//!     4 bytes frame header (sync<11> + version<2> + layer<2> + crc<1>
//!                          + bitrate<4> + samplerate<2> + padding<1>
//!                          + private<1> + chan_mode<2> + mode_ext<2>
//!                          + copyright<1> + original<1> + emphasis<2>)
//!     side info (8/17/32 bytes depending on version/channel mode)
//!     frame data
//!
//! First frame may carry a Xing/Info magic right after side info ⇒ it's
//! the VBR/CBR-LAME info frame, not an encoded audio frame; we skip it
//! and parse the next frame for the audio params.

use revelio_core::{FileAnalyze, StreamKind};

/// Lookup: [version_index][layer_index][bitrate_index] → kbps. Indexed by
/// the *bitstream* values: version 0..=3 (2.5,X,2,1), layer 0..=3
/// (X,3,2,1), bitrate 0..=15.
const BITRATES_KBPS: [[[u16; 16]; 4]; 4] = [
    // MPEG 2.5
    [
        [0; 16],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0],
    ],
    // Reserved
    [[0; 16]; 4],
    // MPEG 2
    [
        [0; 16],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0],
    ],
    // MPEG 1
    [
        [0; 16],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0],
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0],
    ],
];

const SAMPLE_RATES: [[u32; 4]; 4] = [
    [11025, 12000, 8000, 0], // MPEG 2.5
    [0, 0, 0, 0],            // Reserved
    [22050, 24000, 16000, 0], // MPEG 2
    [44100, 48000, 32000, 0], // MPEG 1
];

const SAMPLES_PER_FRAME: [[u16; 4]; 4] = [
    [0, 576, 1152, 384], // MPEG 2.5: Layer X, III, II, I
    [0, 0, 0, 0],        // Reserved
    [0, 576, 1152, 384], // MPEG 2
    [0, 1152, 1152, 384], // MPEG 1
];

/// Coefficient × bitrate_kbps × 1000 / sample_rate = frame size in bytes
/// (before padding). MPEG-1 Layer I needs special handling.
fn frame_size_bytes(version: u8, layer: u8, bitrate_kbps: u16, sample_rate: u32, padding: u8) -> u32 {
    if sample_rate == 0 || bitrate_kbps == 0 {
        return 0;
    }
    let bitrate_bps = bitrate_kbps as u32 * 1000;
    match (version, layer) {
        // MPEG-1 Layer I
        (3, 3) => (12 * bitrate_bps / sample_rate + padding as u32) * 4,
        // MPEG-2/2.5 Layer I
        (_, 3) => (12 * bitrate_bps / sample_rate + padding as u32) * 4,
        // MPEG-1 Layer II/III
        (3, _) => 144 * bitrate_bps / sample_rate + padding as u32,
        // MPEG-2/2.5 Layer III
        (_, 1) => 72 * bitrate_bps / sample_rate + padding as u32,
        // MPEG-2/2.5 Layer II
        (_, _) => 144 * bitrate_bps / sample_rate + padding as u32,
    }
}

const VERSION_NAMES: [&str; 4] = ["2.5", "?", "2", "1"];
const LAYER_NAMES: [&str; 4] = ["", "Layer 3", "Layer 2", "Layer 1"];
const CHANNEL_MODE_NAMES: [&str; 4] = ["Stereo", "Joint stereo", "Dual channel", "Single channel"];

fn channel_mode_to_count(mode: u8) -> u16 {
    if mode == 3 { 1 } else { 2 }
}

/// Layer III mode extension semantics (only meaningful for joint stereo):
/// bit 4 = intensity stereo, bit 5 = MS stereo. The C++ side displays
/// using the same combinations.
fn mode_extension_name(layer: u8, mode_ext: u8) -> &'static str {
    if layer == 1 {
        // Layer 3
        match mode_ext & 0x3 {
            0 => "",
            1 => "Intensity Stereo",
            2 => "MS Stereo",
            3 => "Intensity Stereo + MS Stereo",
            _ => "",
        }
    } else {
        // Layer 1 / 2: bounds (sub-bands)
        match mode_ext & 0x3 {
            0 => "Bound 4",
            1 => "Bound 8",
            2 => "Bound 12",
            3 => "Bound 16",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FrameHeader {
    version: u8, // 0=2.5, 2=2, 3=1
    layer: u8,   // 1=L3, 2=L2, 3=L1
    bitrate_kbps: u16,
    sample_rate: u32,
    #[allow(dead_code)]
    padding: u8,
    channel_mode: u8,
    mode_ext: u8,
    samples_per_frame: u16,
    frame_size: u32,
}

fn parse_frame_header(bytes: &[u8]) -> Option<FrameHeader> {
    if bytes.len() < 4 {
        return None;
    }
    // Sync: 11 bits of 1s. Allow both 0xFFE (lax) and 0xFFF (strict).
    if bytes[0] != 0xFF || (bytes[1] & 0xE0) != 0xE0 {
        return None;
    }
    let version = (bytes[1] >> 3) & 0x3;
    let layer = (bytes[1] >> 1) & 0x3;
    let bitrate_idx = (bytes[2] >> 4) & 0xF;
    let sr_idx = (bytes[2] >> 2) & 0x3;
    let padding = (bytes[2] >> 1) & 0x1;
    let channel_mode = (bytes[3] >> 6) & 0x3;
    let mode_ext = (bytes[3] >> 4) & 0x3;

    if version == 1 || layer == 0 || bitrate_idx == 0 || bitrate_idx == 15 || sr_idx == 3 {
        return None;
    }

    let bitrate_kbps = BITRATES_KBPS[version as usize][layer as usize][bitrate_idx as usize];
    let sample_rate = SAMPLE_RATES[version as usize][sr_idx as usize];
    let samples_per_frame = SAMPLES_PER_FRAME[version as usize][layer as usize];
    let frame_size = frame_size_bytes(version, layer, bitrate_kbps, sample_rate, padding);

    Some(FrameHeader {
        version,
        layer,
        bitrate_kbps,
        sample_rate,
        padding,
        channel_mode,
        mode_ext,
        samples_per_frame,
        frame_size,
    })
}

/// Offset (from frame start) where Xing/Info/VBRI magic appears, if the
/// frame is an info frame. Depends on version + channel mode.
fn info_magic_offset(version: u8, channel_mode: u8) -> usize {
    let mono = channel_mode == 3;
    match (version, mono) {
        (3, false) => 36,
        (3, true) => 21,
        (_, false) => 21,
        (_, true) => 13,
    }
}

fn looks_like_info_frame(frame_bytes: &[u8], version: u8, channel_mode: u8) -> bool {
    let off = info_magic_offset(version, channel_mode);
    if frame_bytes.len() < off + 4 {
        return false;
    }
    let magic = &frame_bytes[off..off + 4];
    magic == b"Xing" || magic == b"Info" || magic == b"VBRI"
}

pub fn parse_mp3(fa: &mut FileAnalyze) -> bool {
    // Sync must be either at byte 0 OR immediately after an ID3v2
    // header. Scanning for sync further into the file produces false
    // positives on any container that happens to contain 0xFF 0xE/F
    // bytes (which is most of them). Container parsers run before
    // this so by the time we're here it's not been claimed.
    let (id3v2_size, id3_metadata) = parse_id3v2(fa);
    if id3v2_size > 0 {
        fa.Skip_Hexa(id3v2_size, "ID3v2");
    }

    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };
    if h[0] != 0xFF || (h[1] & 0xE0) != 0xE0 {
        return false;
    }
    if parse_frame_header(&h[..4]).is_none() {
        return false;
    }

    // Peek the first frame header.
    let first_header_bytes = fa.peek_raw(4);
    if first_header_bytes.is_none() {
        return false;
    }
    let first_header = match parse_frame_header(first_header_bytes.unwrap()) {
        Some(h) => h,
        None => return false,
    };

    fa.Element_Begin("MPEG Audio");

    // Read the first frame (info frame, if present) up front so we can
    // capture the magic + Xing/LAME fields without holding a borrow of
    // `fa` through the later mutable calls.
    let first_frame_owned: Option<Vec<u8>> = fa
        .peek_raw(first_header.frame_size as usize)
        .map(|b| b.to_vec());
    let info_magic: Option<[u8; 4]> = first_frame_owned.as_ref().and_then(|b| {
        let off = info_magic_offset(first_header.version, first_header.channel_mode);
        if b.len() >= off + 4 {
            Some([b[off], b[off + 1], b[off + 2], b[off + 3]])
        } else {
            None
        }
    });
    let is_info_frame = matches!(info_magic, Some(m) if &m == b"Xing" || &m == b"Info" || &m == b"VBRI");
    // LAME tag's nominal bitrate (1 byte at offset 20 from LAME magic,
    // which sits ~36 bytes after Xing/Info magic when all 4 flags set).
    // Also encoder delay+padding (3 bytes packed: 12 bits each).
    let (xing_nominal_bitrate, xing_delay, xing_padding) = first_frame_owned
        .as_ref()
        .map(|b| parse_xing_lame(b, first_header.version, first_header.channel_mode))
        .unwrap_or((None, None, None));

    // Extract the LAME encoder string ("LAME3.100"). Search up to the
    // first 32 KiB of the post-ID3v2 region: libmp3lame stamps the
    // version in the ancillary data of each audio frame, and FFmpeg
    // often overwrites the Info tag's Encoder field with its own
    // branding (e.g. "Lavc..."), so relying on the Info frame alone
    // misses it. The "LAME" magic + digit at byte 4 is distinctive.
    let mut lame_version: Option<String> = None;
    if let Some(scan_buf) = fa.peek_raw(fa.Remain().min(32 * 1024)) {
        for i in 0..scan_buf.len().saturating_sub(9) {
            if &scan_buf[i..i + 4] == b"LAME"
                && scan_buf[i + 4].is_ascii_digit()
                && scan_buf[i + 5] == b'.'
            {
                if let Ok(s) = std::str::from_utf8(&scan_buf[i..i + 9]) {
                    lame_version = Some(s.to_owned());
                }
                break;
            }
        }
    }

    let audio_frame_start;
    let audio_frame_header;
    if is_info_frame {
        fa.Skip_Hexa(first_header.frame_size as usize, "InfoFrame");
        audio_frame_start = fa.Element_Offset();
        let next_bytes = fa.peek_raw(4);
        let Some(b) = next_bytes else {
            fa.Element_End();
            return false;
        };
        let Some(h) = parse_frame_header(b) else {
            fa.Element_End();
            return false;
        };
        audio_frame_header = h;
    } else {
        audio_frame_start = fa.Element_Offset();
        audio_frame_header = first_header;
    }

    // Walk frames to count them + detect VBR.
    let (frame_count, audio_bytes, is_vbr_frames) = scan_frames(fa, audio_frame_start);
    fa.Element_End();

    // Xing magic = VBR header; Info magic = CBR-LAME header. VBRI also
    // = VBR. Treat the file as VBR if either the header says so OR
    // per-frame bitrates vary.
    let xing_says_vbr = matches!(info_magic, Some(m) if &m == b"Xing" || &m == b"VBRI");
    let is_vbr = is_vbr_frames || xing_says_vbr;

    fill_streams(
        fa,
        &audio_frame_header,
        frame_count,
        audio_bytes,
        is_info_frame,
        id3v2_size,
        lame_version.as_deref(),
        is_vbr,
        xing_nominal_bitrate,
        xing_delay,
        xing_padding,
    );
    true
}

/// Parse Xing/LAME fields from the info frame. Returns
/// (nominal_bitrate_kbps, encoder_delay_samples, encoder_padding_samples).
fn parse_xing_lame(
    frame: &[u8],
    version: u8,
    channel_mode: u8,
) -> (Option<u16>, Option<u32>, Option<u32>) {
    let magic_off = info_magic_offset(version, channel_mode);
    if frame.len() < magic_off + 8 {
        return (None, None, None);
    }
    let magic = &frame[magic_off..magic_off + 4];
    let flags = u32::from_be_bytes([
        frame[magic_off + 4],
        frame[magic_off + 5],
        frame[magic_off + 6],
        frame[magic_off + 7],
    ]);
    if magic != b"Xing" && magic != b"Info" {
        return (None, None, None);
    }
    // Walk past optional Xing fields to reach the LAME tag.
    let mut p = magic_off + 8;
    if (flags & 0x1) != 0 {
        p += 4; // frames
    }
    if (flags & 0x2) != 0 {
        p += 4; // bytes
    }
    if (flags & 0x4) != 0 {
        p += 100; // TOC
    }
    if (flags & 0x8) != 0 {
        p += 4; // quality
    }
    // LAME tag starts at `p`: 9 bytes encoder string, then a 36-byte
    // structure. Nominal bitrate = byte 20, delay/padding = bytes 21..24.
    if p + 24 > frame.len() {
        return (None, None, None);
    }
    let nominal_kbps = frame[p + 9 + 11] as u16; // p + 20
    let dp_byte0 = frame[p + 9 + 12] as u32;
    let dp_byte1 = frame[p + 9 + 13] as u32;
    let dp_byte2 = frame[p + 9 + 14] as u32;
    // delay = high 12 bits across bytes [0]+[1high4], padding = low 12 bits.
    let delay = (dp_byte0 << 4) | (dp_byte1 >> 4);
    let padding = ((dp_byte1 & 0x0F) << 8) | dp_byte2;
    (
        if nominal_kbps > 0 { Some(nominal_kbps) } else { None },
        Some(delay),
        Some(padding),
    )
}

#[derive(Default, Debug)]
struct Id3Metadata {
    /// COMM (Comments) frame content — typically free-form text, e.g.
    /// Suno embeds JSON-ish provenance ("made with suno; created=...").
    comment: Option<String>,
    /// TIT2 (Title) frame.
    title: Option<String>,
    /// TPE1 (Performer) frame.
    performer: Option<String>,
    /// TALB (Album) frame.
    album: Option<String>,
    /// TRCK (Track number) frame.
    track: Option<String>,
    /// TCON (Genre) frame.
    genre: Option<String>,
    /// TDRC / TYER (Year / Recording time) frame.
    year: Option<String>,
}

/// Parse the ID3v2 tag at the current position. Returns the total tag
/// size in bytes (header + payload) and any recognised frame values.
/// When the file isn't ID3v2 the size is zero and metadata is None.
fn parse_id3v2(fa: &mut FileAnalyze) -> (usize, Option<Id3Metadata>) {
    let header = fa.peek_raw(10);
    let Some(h) = header else { return (0, None) };
    if &h[0..3] != b"ID3" {
        return (0, None);
    }
    let major = h[3];
    let _minor = h[4];
    let flags = h[5];
    // Syncsafe size: 7 bits per byte, no high bit. Payload size only —
    // total tag = 10-byte header + size.
    let payload_size = ((h[6] as usize) << 21)
        | ((h[7] as usize) << 14)
        | ((h[8] as usize) << 7)
        | (h[9] as usize);
    let total_size = 10 + payload_size;

    // Read the whole tag for frame walking. We won't consume the bytes
    // from `fa` here — the caller still does Skip_Hexa(total_size) after.
    let full = match fa.peek_raw(total_size) {
        Some(b) if b.len() == total_size => b.to_vec(),
        _ => return (total_size, None),
    };

    // Extended-header bytes follow the main header when the flag is set.
    // Skip them — we don't use their content.
    let mut p = 10usize;
    if (flags & 0x40) != 0 && p + 4 <= full.len() {
        let ext_sz = ((full[p] as usize) << 21)
            | ((full[p + 1] as usize) << 14)
            | ((full[p + 2] as usize) << 7)
            | (full[p + 3] as usize);
        // ID3v2.4 ext header size includes its own 4 bytes; ID3v2.3 doesn't.
        p += if major >= 4 { ext_sz } else { 4 + ext_sz };
    }

    let mut md = Id3Metadata::default();
    // ID3v2.2 used 3-char frame IDs + 3-byte sizes; v2.3+ use 4-char IDs
    // + 4-byte sizes. We only handle v2.3 and v2.4 (the common cases).
    let frame_id_len = if major >= 3 { 4 } else { 3 };
    let frame_hdr_len = if major >= 3 { 10 } else { 6 };
    while p + frame_hdr_len <= full.len() {
        // A frame ID starting with 0x00 marks padding — stop.
        if full[p] == 0 {
            break;
        }
        let id_bytes = &full[p..p + frame_id_len];
        let id = std::str::from_utf8(id_bytes).unwrap_or("").to_owned();
        let size_pos = p + frame_id_len;
        let frame_size = if major >= 4 {
            // ID3v2.4: syncsafe 4-byte size.
            ((full[size_pos] as usize) << 21)
                | ((full[size_pos + 1] as usize) << 14)
                | ((full[size_pos + 2] as usize) << 7)
                | (full[size_pos + 3] as usize)
        } else if major == 3 {
            // ID3v2.3: plain 4-byte BE size.
            ((full[size_pos] as usize) << 24)
                | ((full[size_pos + 1] as usize) << 16)
                | ((full[size_pos + 2] as usize) << 8)
                | (full[size_pos + 3] as usize)
        } else {
            // ID3v2.2: 3-byte BE size.
            ((full[size_pos] as usize) << 16)
                | ((full[size_pos + 1] as usize) << 8)
                | (full[size_pos + 2] as usize)
        };
        let body_pos = p + frame_hdr_len;
        if body_pos + frame_size > full.len() {
            break;
        }
        let body = &full[body_pos..body_pos + frame_size];
        let value = decode_text_frame(id.as_str(), body);
        match id.as_str() {
            "COMM" | "COM" => {
                if md.comment.is_none() { md.comment = value; }
            }
            "TIT2" | "TT2" => {
                if md.title.is_none() { md.title = value; }
            }
            "TPE1" | "TP1" => {
                if md.performer.is_none() { md.performer = value; }
            }
            "TALB" | "TAL" => {
                if md.album.is_none() { md.album = value; }
            }
            "TRCK" | "TRK" => {
                if md.track.is_none() { md.track = value; }
            }
            "TCON" | "TCO" => {
                if md.genre.is_none() { md.genre = value; }
            }
            "TDRC" | "TYER" | "TYE" => {
                if md.year.is_none() { md.year = value; }
            }
            _ => {}
        }
        p = body_pos + frame_size;
    }
    (total_size, Some(md))
}

/// Decode a text-frame body. Layout: 1-byte encoding + bytes.
/// Encoding: 0=ISO-8859-1, 1=UTF-16 with BOM, 2=UTF-16BE (v2.4),
/// 3=UTF-8 (v2.4). COMM/COM frames add 3-byte language + null-terminated
/// short description before the actual text.
fn decode_text_frame(id: &str, body: &[u8]) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let encoding = body[0];
    let mut p = 1usize;
    // COMM/COM: skip 3 language bytes + null-terminated description.
    if id == "COMM" || id == "COM" {
        if p + 3 > body.len() {
            return None;
        }
        p += 3;
        // Find null terminator for description (1 or 2 bytes per char
        // depending on encoding). For our purposes a single 0x00 byte
        // is good enough for the simple case; multi-byte encodings
        // would require pair search.
        match encoding {
            1 | 2 => {
                // 2-byte 0x0000 terminator
                while p + 1 < body.len() {
                    if body[p] == 0 && body[p + 1] == 0 {
                        p += 2;
                        break;
                    }
                    p += 1;
                }
            }
            _ => {
                while p < body.len() && body[p] != 0 {
                    p += 1;
                }
                if p < body.len() {
                    p += 1;
                }
            }
        }
    }
    let text = &body[p..];
    decode_text_with_encoding(encoding, text)
}

fn decode_text_with_encoding(encoding: u8, bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    let s = match encoding {
        // ISO-8859-1
        0 => bytes.iter().map(|&b| b as char).collect::<String>(),
        // UTF-16 with BOM
        1 => {
            if bytes.len() < 2 {
                return None;
            }
            let (le, payload) = match (bytes[0], bytes[1]) {
                (0xFF, 0xFE) => (true, &bytes[2..]),
                (0xFE, 0xFF) => (false, &bytes[2..]),
                _ => (false, bytes),
            };
            let u16s: Vec<u16> = payload
                .chunks_exact(2)
                .map(|c| {
                    if le {
                        u16::from_le_bytes([c[0], c[1]])
                    } else {
                        u16::from_be_bytes([c[0], c[1]])
                    }
                })
                .collect();
            String::from_utf16_lossy(&u16s)
        }
        // UTF-16BE (v2.4)
        2 => {
            let u16s: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16s)
        }
        // UTF-8 (v2.4)
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    let trimmed = s.trim_end_matches('\0').trim().to_owned();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}


/// Scan frames sequentially from the current position, returning
/// (frame_count, total_bytes_consumed, is_vbr). VBR is detected by
/// observing any per-frame bitrate change vs. the first frame.
fn scan_frames(fa: &mut FileAnalyze, audio_frame_start: usize) -> (u32, u64, bool) {
    let mut frame_count: u32 = 0;
    let mut first_bitrate: Option<u16> = None;
    let mut is_vbr = false;
    let starting_remain = fa.Remain();
    loop {
        let header_bytes = fa.peek_raw(4);
        let Some(b) = header_bytes else { break };
        let Some(h) = parse_frame_header(b) else {
            // Allow ID3v1 (128 bytes ending the file, magic "TAG") to
            // terminate the scan cleanly.
            break;
        };
        if h.frame_size == 0 || fa.Remain() < h.frame_size as usize {
            break;
        }
        if let Some(fb) = first_bitrate {
            if h.bitrate_kbps != fb {
                is_vbr = true;
            }
        } else {
            first_bitrate = Some(h.bitrate_kbps);
        }
        fa.Skip_Hexa(h.frame_size as usize, "Frame");
        frame_count += 1;
    }
    let consumed = starting_remain.saturating_sub(fa.Remain()) as u64;
    let _ = audio_frame_start;
    (frame_count, consumed, is_vbr)
}

fn fill_streams(
    fa: &mut FileAnalyze,
    h: &FrameHeader,
    audio_frame_count: u32,
    audio_bytes_consumed: u64,
    had_info_frame: bool,
    id3v2_size: usize,
    lame_version: Option<&str>,
    is_vbr: bool,
    xing_nominal_kbps: Option<u16>,
    xing_delay: Option<u32>,
    xing_padding: Option<u32>,
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MPEG Audio", false);
    if let Some(lv) = lame_version {
        fa.Fill(StreamKind::General, 0, "Encoded_Library", lv, false);
    }

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "MPEG Audio", false);
    if let Some(lv) = lame_version {
        fa.Fill(StreamKind::Audio, 0, "Encoded_Library", lv, false);
    }
    fa.Fill(StreamKind::Audio, 0, "Format_Version", VERSION_NAMES[h.version as usize], false);
    fa.Fill(StreamKind::Audio, 0, "Format_Profile", LAYER_NAMES[h.layer as usize], false);
    // Oracle suppresses Format_Settings_Mode for single-channel (mono)
    // MP3 — the redundant "Single channel" string is omitted when
    // Channels=1 already implies mono.
    if h.channel_mode != 3 {
        fa.Fill(
            StreamKind::Audio,
            0,
            "Format_Settings_Mode",
            CHANNEL_MODE_NAMES[h.channel_mode as usize],
            false,
        );
    }
    if h.channel_mode == 1 {
        let ext = mode_extension_name(h.layer, h.mode_ext);
        if !ext.is_empty() {
            fa.Fill(StreamKind::Audio, 0, "Format_Settings_ModeExtension", ext, false);
        }
    }
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", if is_vbr { "VBR" } else { "CBR" }, false);
    // BitRate selection:
    //   CBR: use the (constant) frame bitrate.
    //   VBR: prefer the Xing/LAME nominal-bitrate byte (matches oracle
    //   exactly for LAME-encoded files); fall back to a computed average
    //   when LAME isn't present.
    let bitrate_bps: u32 = if is_vbr {
        if let Some(nom) = xing_nominal_kbps {
            nom as u32 * 1000
        } else if audio_frame_count > 0 && h.sample_rate > 0 {
            let frames = audio_frame_count as u64;
            let dur_sec = (frames * h.samples_per_frame as u64) as f64 / h.sample_rate as f64;
            if dur_sec > 0.0 {
                let avg = (audio_bytes_consumed as f64 * 8.0 / dur_sec).round() as u32;
                ((avg + 500) / 1000) * 1000
            } else {
                (h.bitrate_kbps as u32) * 1000
            }
        } else {
            (h.bitrate_kbps as u32) * 1000
        }
    } else {
        (h.bitrate_kbps as u32) * 1000
    };
    fa.Fill(StreamKind::Audio, 0, "BitRate", bitrate_bps.to_string(), false);
    fa.Fill(
        StreamKind::Audio,
        0,
        "Channels",
        channel_mode_to_count(h.channel_mode).to_string(),
        false,
    );
    fa.Fill(StreamKind::Audio, 0, "SamplesPerFrame", h.samples_per_frame.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", h.sample_rate.to_string(), false);

    // VBR (Xing/VBRI): info frame is a TOC carrier with no audio samples,
    // so it doesn't count toward SamplingCount/FrameCount.
    // CBR (Info magic): oracle includes the info frame's nominal samples
    // in the count.
    let info_frame_addend = if had_info_frame && !is_vbr { 1 } else { 0 };
    let total_frame_count = audio_frame_count + info_frame_addend;
    let sampling_count = (total_frame_count as u64) * (h.samples_per_frame as u64);
    if sampling_count > 0 {
        fa.Fill(StreamKind::Audio, 0, "SamplingCount", sampling_count.to_string(), false);
    }

    // FrameRate = sample_rate / samples_per_frame, 3 decimal places.
    if h.samples_per_frame > 0 {
        let frame_rate = (h.sample_rate as f64) / (h.samples_per_frame as f64);
        fa.Fill(StreamKind::Audio, 0, "FrameRate", format!("{:.3}", frame_rate), false);
    }
    fa.Fill(StreamKind::Audio, 0, "FrameCount", total_frame_count.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.Fill(StreamKind::Audio, 0, "StreamSize", audio_bytes_consumed.to_string(), false);

    // Audio Duration: frames-based.
    if h.sample_rate > 0 && sampling_count > 0 {
        let duration_ms = (sampling_count * 1000) / (h.sample_rate as u64);
        fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
    }

    // General-stream fields that aren't file-level but need the parser's
    // knowledge of ID3v2/audio_bytes vs the harness's FileSize-Audio.StreamSize
    // fallback. Use replace=true so the harness can't overwrite.
    fa.Fill(StreamKind::General, 0, "StreamSize", id3v2_size.to_string(), true);
    if bitrate_bps > 0 {
        // General Duration = Audio.StreamSize * 8000 / BitRate_bps — gives a
        // round duration like 1.536s for a 24576-byte/128kbps stream, which
        // is what the oracle reports.
        let general_duration_ms =
            (audio_bytes_consumed * 8 * 1000) / (bitrate_bps as u64);
        fa.Fill(
            StreamKind::General,
            0,
            "Duration",
            general_duration_ms.to_string(),
            true,
        );
        // OverallBitRate for CBR MPEG Audio is the audio bitrate itself —
        // the C++ side bypasses the FileSize/Duration computation in this
        // case. replace=true to override the harness fallback.
        fa.Fill(
            StreamKind::General,
            0,
            "OverallBitRate",
            bitrate_bps.to_string(),
            true,
        );
    }
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical_mpeg1_layer3_header() {
        // 0xFF 0xFB 0x94 0x44 — MPEG-1 Layer III, 128kbps, 48kHz, Joint
        // stereo, padding off, no protection.
        let h = parse_frame_header(&[0xFF, 0xFB, 0x94, 0x44]).expect("parse");
        assert_eq!(h.version, 3);
        assert_eq!(h.layer, 1);
        assert_eq!(h.bitrate_kbps, 128);
        assert_eq!(h.sample_rate, 48000);
        assert_eq!(h.channel_mode, 1);
        assert_eq!(h.samples_per_frame, 1152);
        // frame_size = 144 * 128000 / 48000 = 384
        assert_eq!(h.frame_size, 384);
    }

    #[test]
    fn rejects_non_sync_buffer() {
        assert!(parse_frame_header(&[0x00, 0x00, 0x00, 0x00]).is_none());
        assert!(parse_frame_header(&[0xFF, 0x1F, 0x94, 0x44]).is_none());
    }

    #[test]
    fn rejects_reserved_or_invalid_fields() {
        // version=1 (reserved): byte1 bits[4:3] = 01 ⇒ byte1 = 0b11101011 = 0xEB
        assert!(parse_frame_header(&[0xFF, 0xEB, 0x94, 0x44]).is_none());
        // layer=0 (reserved): byte1 bits[2:1] = 00 ⇒ byte1 = 0b11111001 = 0xF9
        assert!(parse_frame_header(&[0xFF, 0xF9, 0x94, 0x44]).is_none());
        // bitrate=15 (bad)
        assert!(parse_frame_header(&[0xFF, 0xFB, 0xF4, 0x44]).is_none());
        // bitrate=0 (free) — also reject; we don't compute frame size for free format
        assert!(parse_frame_header(&[0xFF, 0xFB, 0x04, 0x44]).is_none());
    }

    #[test]
    fn channel_count_derivation() {
        assert_eq!(channel_mode_to_count(0), 2);
        assert_eq!(channel_mode_to_count(1), 2);
        assert_eq!(channel_mode_to_count(2), 2);
        assert_eq!(channel_mode_to_count(3), 1);
    }
}
