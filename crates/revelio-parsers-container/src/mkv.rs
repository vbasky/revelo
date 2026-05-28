//! Matroska / WebM parser — EBML-based container.
//!
//! Mirrors a subset of MediaInfoLib's `File_Mk.cpp` covering plain
//! single-audio-track Matroska files. The container is structurally
//! similar to MP4 boxes — a tree of `[id][size][payload]` elements —
//! but the headers use Variable-length Integer encoding (VINT) where
//! the byte length is signaled by the position of the leading 1-bit.
//!
//! VINT encoding:
//! - First byte's leading zeros count + 1 = total VINT length N (1..=8).
//! - For element IDs, the value includes the leading 1-bit (so IDs
//!   look like 0x1A45DFA3, 0x18538067, etc — distinguishable).
//! - For sizes, the leading 1-bit is stripped and the remaining bits
//!   plus N-1 subsequent bytes form the integer.

use revelio_core::{FileAnalyze, StreamKind};

// EBML root + segment.
const EBML_HEADER: u64 = 0x1A45DFA3;
const SEGMENT: u64 = 0x18538067;

// EBML header children. (EBMLVersion/ReadVersion declared but unused
// in this MVP — kept for future schema awareness.)
#[allow(dead_code)]
const EBML_VERSION: u64 = 0x4286;
#[allow(dead_code)]
const EBML_READ_VERSION: u64 = 0x42F7;
const DOC_TYPE: u64 = 0x4282;
const DOC_TYPE_VERSION: u64 = 0x4287;

// Segment top-level.
const SEGMENT_INFO: u64 = 0x1549A966;
const TRACKS: u64 = 0x1654AE6B;
const CLUSTER: u64 = 0x1F43B675;
const SEEK_HEAD: u64 = 0x114D9B74;
const TAGS: u64 = 0x1254C367;
const CHAPTERS: u64 = 0x1043A770;
const ATTACHMENTS: u64 = 0x1941A469;
const EDITION_ENTRY: u64 = 0x45B9;
const CHAPTER_ATOM: u64 = 0xB6;
#[allow(dead_code)]
const CHAPTER_TIME_START: u64 = 0x91;
#[allow(dead_code)]
const CHAPTER_TIME_END: u64 = 0x92;
const CHAPTER_DISPLAY: u64 = 0x80;
const CHAP_STRING: u64 = 0x85;
#[allow(dead_code)]
const CHAP_LANGUAGE: u64 = 0x437C;
const ATTACHED_FILE: u64 = 0x61A7;
const FILE_DESCRIPTION: u64 = 0x467E;  // File description
const FILE_NAME: u64 = 0x466E;
const FILE_MIME_TYPE: u64 = 0x4660;
#[allow(dead_code)]
const FILE_DATA: u64 = 0x465C;
#[allow(dead_code)]
const FILE_UID: u64 = 0x46AE;
const TAG: u64 = 0x7373;
const TAG_TARGETS: u64 = 0x63C0;
const TAG_TARGETS_TRACK_UID: u64 = 0x63C5;
const SIMPLE_TAG: u64 = 0x67C8;
const TAG_NAME: u64 = 0x45A3;
const TAG_STRING: u64 = 0x4487;
#[allow(dead_code)]
const CRC32: u64 = 0xBF;

// Info children.
const SEGMENT_UUID: u64 = 0x73A4;
const TIMECODE_SCALE: u64 = 0x2AD7B1;
const DURATION: u64 = 0x4489;
const MUXING_APP: u64 = 0x4D80;
const WRITING_APP: u64 = 0x5741;

// Tracks > TrackEntry.
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_UID: u64 = 0x73C5;
const TRACK_TYPE: u64 = 0x83;
const FLAG_DEFAULT: u64 = 0x88;
const FLAG_FORCED: u64 = 0x55AA;
const CODEC_ID: u64 = 0x86;
const CODEC_PRIVATE: u64 = 0x63A2;
const AUDIO_BLOCK: u64 = 0xE1;
const SAMPLING_FREQUENCY: u64 = 0xB5;
const CHANNELS: u64 = 0x9F;
const BIT_DEPTH: u64 = 0x6264;

// Track entry — additional children.
const LANGUAGE: u64 = 0x22B59C;
const NAME: u64 = 0x536E;
const DEFAULT_DURATION: u64 = 0x23E383;

// Video block + children.
const VIDEO_BLOCK: u64 = 0xE0;
const PIXEL_WIDTH: u64 = 0xB0;
const PIXEL_HEIGHT: u64 = 0xBA;
const DISPLAY_WIDTH: u64 = 0x54B0;
const DISPLAY_HEIGHT: u64 = 0x54BA;
const COLOUR: u64 = 0x55B0;

// Colour element children.
const MATRIX_COEFFICIENTS: u64 = 0x55B1;
const BITS_PER_CHANNEL: u64 = 0x55B2;
const RANGE: u64 = 0x55B9;
const TRANSFER_CHARACTERISTICS: u64 = 0x55BA;
const PRIMARIES: u64 = 0x55BB;

pub fn parse_mkv(fa: &mut FileAnalyze) -> bool {
    // Detect EBML header by leading 4 bytes.
    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };
    if h != [0x1A, 0x45, 0xDF, 0xA3] {
        return false;
    }

    let file_size = fa.remain();
    let mut doc_type: Option<String> = None;
    let mut doc_type_version: u64 = 0;
    let mut movie = MovieInfo::default();
    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut seekhead_seen = false;
    let mut cluster_seen = false;
    let mut seekhead_before_cluster = true;
    let mut crc32_at_level1 = false;
    let mut tag_pairs: Vec<TagEntry> = Vec::new();

    walk_elements(fa, file_size, &mut |fa, id, size, _start| {
        match id {
            EBML_HEADER => {
                walk_elements(fa, size, &mut |fa, id, sz, _| match id {
                    DOC_TYPE => {
                        let bytes = fa.read_raw(sz).to_vec();
                        doc_type = Some(strip_nuls(&bytes));
                    }
                    DOC_TYPE_VERSION => {
                        doc_type_version = read_uint(fa, sz);
                    }
                    _ => {
                        fa.skip_hexa(sz, "ebml_child");
                    }
                });
            }
            SEGMENT => {
                walk_elements(fa, size, &mut |fa, id, sz, _| {
                    // CRC-32 typically appears as the FIRST CHILD of a
                    // level-1 master element (SeekHead, Info, Tracks,
                    // Cluster). MediaInfoLib reports "Per level 1" when
                    // any level-1 container starts with a CRC-32.
                    let first_byte_is_crc32 = fa.peek_raw(1).map(|b| b[0] == 0xBF).unwrap_or(false);
                    if first_byte_is_crc32 {
                        crc32_at_level1 = true;
                    }
                    match id {
                        SEEK_HEAD => {
                            seekhead_seen = true;
                            if cluster_seen {
                                seekhead_before_cluster = false;
                            }
                            fa.skip_hexa(sz, "seekhead");
                        }
                        SEGMENT_INFO => parse_segment_info(fa, sz, &mut movie),
                        TRACKS => parse_tracks(fa, sz, &mut tracks),
                        CLUSTER => {
                            cluster_seen = true;
                            fa.skip_hexa(sz, "cluster");
                        }
                        TAGS => parse_tags(fa, sz, &mut tag_pairs),
                CHAPTERS => {
                    let (count, names) = count_chapters(fa, sz);
                    movie.chapter_names = names;
                    movie.chapter_count = count;
                }
                ATTACHMENTS => {
                    let (count, has_cover, cover_mime) = count_attachments(fa, sz);
                    movie.has_cover_art = has_cover;
                    movie.cover_mime_type = cover_mime;
                    movie.attachment_count = count;
                }
                        _ => fa.skip_hexa(sz, "segment_child"),
                    }
                });
            }
            _ => {
                fa.skip_hexa(size, "top_level");
            }
        }
    });

    if doc_type.as_deref() != Some("matroska") && doc_type.as_deref() != Some("webm") {
        return false;
    }

    fill_streams(
        fa,
        &movie,
        &tracks,
        doc_type.as_deref().unwrap_or("matroska"),
        doc_type_version,
        seekhead_seen && seekhead_before_cluster,
        crc32_at_level1,
        &tag_pairs,
        file_size,
    );
    true
}

#[derive(Default)]
struct MovieInfo {
    chapter_count: usize,
    chapter_names: Vec<String>,
    attachment_count: usize,
    has_cover_art: bool,
    cover_mime_type: Option<String>,
    segment_uuid: Option<Vec<u8>>,
    timecode_scale: Option<u64>,
    duration_units: Option<f64>,
    muxing_app: Option<String>,
    writing_app: Option<String>,
}

#[derive(Default)]
struct TrackInfo {
    number: Option<u64>,
    uid: Option<u64>,
    track_type: Option<u64>,
    flag_default: Option<bool>,
    flag_forced: Option<bool>,
    codec_id: Option<String>,
    /// ISO 639-2 three-letter code from Language element, e.g. "eng".
    language: Option<String>,
    /// Track Name element — track title.
    name: Option<String>,
    /// DefaultDuration in nanoseconds (per-frame for video, per-sample for audio).
    default_duration_ns: Option<u64>,
    audio_sampling_rate: Option<f64>,
    audio_channels: Option<u64>,
    audio_bit_depth: Option<u64>,
    /// Video PixelWidth/Height (post-crop, pre-display).
    video_width: Option<u64>,
    video_height: Option<u64>,
    /// DisplayWidth/Height — when present, oracle uses these for DAR.
    display_width: Option<u64>,
    display_height: Option<u64>,
    /// Colour element CICP-style indices and ranges.
    colour_primaries: Option<u64>,
    colour_transfer: Option<u64>,
    colour_matrix: Option<u64>,
    colour_range: Option<u64>,
    video_bit_depth: Option<u64>,
    // CodecPrivate element for this track (e.g., AV1 config, Vorbis headers).
    codec_private: Option<Vec<u8>>,
}

/// A (target track UID, tag name, tag value) tuple harvested from the
/// Tags section. A target UID of 0 means the tag applies globally.
struct TagEntry {
    target_track_uid: u64,
    name: String,
    value: String,
}

fn parse_tags(fa: &mut FileAnalyze, size: usize, tag_pairs: &mut Vec<TagEntry>) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        TAG => {
            let mut target_uid: u64 = 0;
            let mut pairs: Vec<(String, String)> = Vec::new();
            walk_elements(fa, sz, &mut |fa, id, sz, _| match id {
                TAG_TARGETS => {
                    walk_elements(fa, sz, &mut |fa, id, sz, _| match id {
                        TAG_TARGETS_TRACK_UID => target_uid = read_uint(fa, sz),
                        _ => fa.skip_hexa(sz, "tag_targets_child"),
                    });
                }
                SIMPLE_TAG => {
                    let mut name = String::new();
                    let mut value = String::new();
                    walk_elements(fa, sz, &mut |fa, id, sz, _| match id {
                        TAG_NAME => {
                            let bytes = fa.read_raw(sz).to_vec();
                            name = strip_nuls(&bytes);
                        }
                        TAG_STRING => {
                            let bytes = fa.read_raw(sz).to_vec();
                            value = strip_nuls(&bytes);
                        }
                        _ => fa.skip_hexa(sz, "simple_tag_child"),
                    });
                    if !name.is_empty() {
                        pairs.push((name, value));
                    }
                }
                _ => fa.skip_hexa(sz, "tag_child"),
            });
            for (n, v) in pairs {
                tag_pairs.push(TagEntry {
                    target_track_uid: target_uid,
                    name: n,
                    value: v,
                });
            }
        }
        _ => fa.skip_hexa(sz, "tags_child"),
    });
}

fn parse_segment_info(fa: &mut FileAnalyze, size: usize, movie: &mut MovieInfo) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        SEGMENT_UUID => {
            movie.segment_uuid = Some(fa.read_raw(sz).to_vec());
        }
        TIMECODE_SCALE => {
            movie.timecode_scale = Some(read_uint(fa, sz));
        }
        DURATION => {
            movie.duration_units = Some(read_float(fa, sz));
        }
        MUXING_APP => {
            let bytes = fa.read_raw(sz).to_vec();
            movie.muxing_app = Some(strip_nuls(&bytes));
        }
        WRITING_APP => {
            let bytes = fa.read_raw(sz).to_vec();
            movie.writing_app = Some(strip_nuls(&bytes));
        }
        _ => fa.skip_hexa(sz, "info_child"),
    });
}

fn parse_tracks(fa: &mut FileAnalyze, size: usize, tracks: &mut Vec<TrackInfo>) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        TRACK_ENTRY => {
            let mut entry = TrackInfo::default();
            parse_track_entry(fa, sz, &mut entry);
            tracks.push(entry);
        }
        _ => fa.skip_hexa(sz, "tracks_child"),
    });
}

fn parse_track_entry(fa: &mut FileAnalyze, size: usize, entry: &mut TrackInfo) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        TRACK_NUMBER => entry.number = Some(read_uint(fa, sz)),
        TRACK_UID => entry.uid = Some(read_uint(fa, sz)),
        TRACK_TYPE => entry.track_type = Some(read_uint(fa, sz)),
        FLAG_DEFAULT => entry.flag_default = Some(read_uint(fa, sz) != 0),
        FLAG_FORCED => entry.flag_forced = Some(read_uint(fa, sz) != 0),
        CODEC_ID => {
            let bytes = fa.read_raw(sz).to_vec();
            entry.codec_id = Some(strip_nuls(&bytes));
        }
        LANGUAGE => {
            let bytes = fa.read_raw(sz).to_vec();
            entry.language = Some(strip_nuls(&bytes));
        }
        NAME => {
            let bytes = fa.read_raw(sz).to_vec();
            entry.name = Some(strip_nuls(&bytes));
        }
        DEFAULT_DURATION => entry.default_duration_ns = Some(read_uint(fa, sz)),
        AUDIO_BLOCK => {
            walk_elements(fa, sz, &mut |fa, id, sz, _| match id {
                SAMPLING_FREQUENCY => entry.audio_sampling_rate = Some(read_float(fa, sz)),
                CHANNELS => entry.audio_channels = Some(read_uint(fa, sz)),
                BIT_DEPTH => entry.audio_bit_depth = Some(read_uint(fa, sz)),
                _ => fa.skip_hexa(sz, "audio_child"),
            });
        }
        VIDEO_BLOCK => {
            walk_elements(fa, sz, &mut |fa, id, sz, _| match id {
                PIXEL_WIDTH => entry.video_width = Some(read_uint(fa, sz)),
                PIXEL_HEIGHT => entry.video_height = Some(read_uint(fa, sz)),
                DISPLAY_WIDTH => entry.display_width = Some(read_uint(fa, sz)),
                DISPLAY_HEIGHT => entry.display_height = Some(read_uint(fa, sz)),
                COLOUR => {
                    walk_elements(fa, sz, &mut |fa, id, sz, _| match id {
                        MATRIX_COEFFICIENTS => entry.colour_matrix = Some(read_uint(fa, sz)),
                        BITS_PER_CHANNEL => entry.video_bit_depth = Some(read_uint(fa, sz)),
                        RANGE => entry.colour_range = Some(read_uint(fa, sz)),
                        TRANSFER_CHARACTERISTICS => entry.colour_transfer = Some(read_uint(fa, sz)),
                        PRIMARIES => entry.colour_primaries = Some(read_uint(fa, sz)),
                        _ => fa.skip_hexa(sz, "colour_child"),
                    });
                }
                _ => fa.skip_hexa(sz, "video_child"),
            });
        }
        CODEC_PRIVATE => entry.codec_private = Some(fa.read_raw(sz).to_vec()),
        _ => fa.skip_hexa(sz, "trackentry_child"),
    });
}


/// Count chapters by walking the Chapters element.
fn count_chapters(fa: &mut FileAnalyze, size: usize) -> (usize, Vec<String>) {
    let mut count = 0usize;
    let mut names = Vec::new();
    walk_elements(fa, size, &mut |fa, id, sz, _| {
        match id {
            EDITION_ENTRY => {
                // Walk edition entry to count chapter atoms and extract names
                let (edition_count, edition_names) = count_edition_chapters(fa, sz);
                count += edition_count;
                names.extend(edition_names);
            }
            _ => fa.skip_hexa(sz, "chapters_child"),
        }
    });
    (count, names)
}

fn count_edition_chapters(fa: &mut FileAnalyze, size: usize) -> (usize, Vec<String>) {
    let mut count = 0usize;
    let mut names = Vec::new();
    walk_elements(fa, size, &mut |fa, id, sz, _| {
        match id {
            CHAPTER_ATOM => {
                count += 1;
                // Extract chapter name from ChapterDisplay
                if let Some(name) = extract_chapter_name(fa, sz) {
                    names.push(name);
                }
            }
            _ => fa.skip_hexa(sz, "edition_child"),
        }
    });
    (count, names)
}

/// Extract chapter name from ChapterAtom
fn extract_chapter_name(fa: &mut FileAnalyze, size: usize) -> Option<String> {
    let mut name = None;
    walk_elements(fa, size, &mut |fa, id, sz, _| {
        match id {
            CHAPTER_DISPLAY => {
                // Look for ChapString inside ChapterDisplay
                walk_elements(fa, sz, &mut |fa, id2, sz2, _| {
                    match id2 {
                        CHAP_STRING => {
                            let data = fa.read_raw(sz2);
                            if let Ok(s) = std::str::from_utf8(&data) {
                                name = Some(s.to_string());
                            }
                        }
                        _ => fa.skip_hexa(sz2, "chapterdisplay_child"),
                    }
                });
            }
            _ => fa.skip_hexa(sz, "chapteratom_child"),
        }
    });
    name
}

/// Count attachments by walking the Attachments element.
fn count_attachments(fa: &mut FileAnalyze, size: usize) -> (usize, bool, Option<String>) {
    let mut count = 0usize;
    let mut has_cover = false;
    let mut cover_mime = None;
    walk_elements(fa, size, &mut |fa, id, sz, _| {
        match id {
            ATTACHED_FILE => {
                count += 1;
                // Check if this attachment is cover art
                let (is_cover, mime) = check_attachment_cover(fa, sz);
                if is_cover {
                    has_cover = true;
                    if cover_mime.is_none() {
                        cover_mime = mime;
                    }
                }
            }
            _ => fa.skip_hexa(sz, "attachments_child"),
        }
    });
    (count, has_cover, cover_mime)
}

/// Check if attachment is cover art by examining FileName and FileMimeType
fn check_attachment_cover(fa: &mut FileAnalyze, size: usize) -> (bool, Option<String>) {
    let mut is_cover = false;
    let mut mime_type = None;
    walk_elements(fa, size, &mut |fa, id, sz, _| {
        match id {
            FILE_NAME => {
                let data = fa.read_raw(sz);
                if let Ok(name) = std::str::from_utf8(&data) {
                    let lower = name.to_lowercase();
                    // Common cover art filenames
                    if lower.contains("cover") || lower.contains("folder") || 
                       lower.contains("album") || lower.contains("front") ||
                       lower.ends_with(".jpg") || lower.ends_with(".jpeg") ||
                       lower.ends_with(".png") {
                        is_cover = true;
                    }
                }
            }
            FILE_MIME_TYPE => {
                let data = fa.read_raw(sz);
                if let Ok(mime) = std::str::from_utf8(&data) {
                    mime_type = Some(mime.to_string());
                    // Image MIME types indicate potential cover
                    if mime.starts_with("image/") {
                        is_cover = true;
                    }
                }
            }
            FILE_DESCRIPTION => {
                let data = fa.read_raw(sz);
                if let Ok(desc) = std::str::from_utf8(&data) {
                    let lower = desc.to_lowercase();
                    if lower.contains("cover") || lower.contains("poster") {
                        is_cover = true;
                    }
                }
            }
            _ => fa.skip_hexa(sz, "attachedfile_child"),
        }
    });
    (is_cover, mime_type)
}

fn fill_streams(
    fa: &mut FileAnalyze,
    movie: &MovieInfo,
    tracks: &[TrackInfo],
    doc_type: &str,
    doc_type_version: u64,
    is_streamable: bool,
    crc32_at_level1: bool,
    tag_pairs: &[TagEntry],
    file_size: usize,
) {
    fa.stream_prepare(StreamKind::General);
    if let Some(uuid) = movie.segment_uuid.as_ref() {
        if uuid.len() == 16 {
            let mut v: u128 = 0;
            for b in uuid {
                v = (v << 8) | (*b as u128);
            }
            fa.fill(StreamKind::General, 0, "UniqueID", v.to_string(), false);
        }
    }
    // DocType drives the Format string: webm files report "WebM",
    // matroska files report "Matroska".
    let fmt = if doc_type == "webm" { "WebM" } else { "Matroska" };
    fa.fill(StreamKind::General, 0, "Format", fmt, false);
    if doc_type_version > 0 {
        fa.fill(
            StreamKind::General,
            0,
            "Format_Version",
            doc_type_version.to_string(),
            false,
        );
    }
    if let Some(app) = movie.muxing_app.as_deref() {
        fa.fill(StreamKind::General, 0, "Encoded_Application", app, false);
    }
    if let Some(app) = movie.writing_app.as_deref() {
        fa.fill(StreamKind::General, 0, "Encoded_Library", app, false);
    }
    fa.fill(
        StreamKind::General,
        0,
        "IsStreamable",
        if is_streamable { "Yes" } else { "No" },
        false,
    );
    if crc32_at_level1 {
        fa.fill(
            StreamKind::General,
            0,
            "ErrorDetectionType",
            "Per level 1",
            false,
        );
    }

    let timecode_scale_ns: f64 = movie.timecode_scale.unwrap_or(1_000_000) as f64;
    let duration_seconds: Option<f64> = movie
        .duration_units
        .map(|units| units * timecode_scale_ns / 1_000_000_000.0);
    let duration_ms: Option<u64> = duration_seconds.map(|s| (s * 1000.0).round() as u64);

    let mut audio_count: u32 = 0;
    let mut video_count: u32 = 0;
    let mut stream_order: u32 = 0;
    for track in tracks {
        match track.track_type {
            Some(2) => {
                let pos = fa.stream_prepare(StreamKind::Audio);
                fa.fill(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string(), false);
                if let Some(n) = track.number {
                    fa.fill(StreamKind::Audio, pos, "ID", n.to_string(), false);
                }
                if let Some(uid) = track.uid {
                    fa.fill(StreamKind::Audio, pos, "UniqueID", uid.to_string(), false);
                }
                stream_order += 1;
                if let Some(c) = track.codec_id.as_deref() {
                    if let Some(fmt) = mkv_codec_to_format(c) {
                        fa.fill(StreamKind::Audio, pos, "Format", fmt, false);
                    }
                    fa.fill(StreamKind::Audio, pos, "CodecID", c, false);
                }
                if let Some(ch) = track.audio_channels {
                    fa.fill(StreamKind::Audio, pos, "Channels", ch.to_string(), false);
                    let codec_id = track.codec_id.as_deref().unwrap_or("");
                    let (positions, layout) = channel_layout_for_codec(ch as u16, codec_id);
                    if let Some(p) = positions {
                        fa.fill(StreamKind::Audio, pos, "ChannelPositions", p, false);
                    }
                    if let Some(l) = layout {
                        fa.fill(StreamKind::Audio, pos, "ChannelLayout", l, false);
                    }
                }
                if let Some(sr) = track.audio_sampling_rate {
                    let sr_int = sr.round() as u64;
                    fa.fill(StreamKind::Audio, pos, "SamplingRate", sr_int.to_string(), false);
                    if let Some(ms) = duration_ms {
                        let sampling_count = (sr * ms as f64 / 1000.0).round() as u64;
                        fa.fill(
                            StreamKind::Audio,
                            pos,
                            "SamplingCount",
                            sampling_count.to_string(),
                            false,
                        );
                    }
                }
                if let Some(bd) = track.audio_bit_depth {
                    fa.fill(StreamKind::Audio, pos, "BitDepth", bd.to_string(), false);
                }
                
                // Parse CodecPrivate for Opus to get accurate sample rate and channel mapping
                if track.codec_id.as_deref() == Some("A_OPUS") {
                    if let Some(ref private) = track.codec_private {
                        if private.len() >= 19 && &private[0..8] == b"OpusHead" {
                            // OpusHead format:
                            // 0-7: "OpusHead" magic
                            // 8: version (should be 1)
                            // 9: channel count
                            // 10-11: pre-skip (u16le)
                            // 12-15: sample rate (u32le)
                            // 16-17: output gain (s16le)
                            // 18: channel mapping family
                            let version = private[8];
                            if version == 1 {
                                let channels = private[9];
                                let sample_rate = (private[12] as u32) 
                                    | ((private[13] as u32) << 8)
                                    | ((private[14] as u32) << 16)
                                    | ((private[15] as u32) << 24);
                                let _channel_mapping = private[18];
                                
                                // Opus internally uses 48kHz, but declares output rate here
                                if sample_rate > 0 {
                                    fa.fill(StreamKind::Audio, pos, "SamplingRate", sample_rate.to_string(), true);
                                }
                                
                                // Set channels from header if not already set
                                if track.audio_channels.is_none() && channels > 0 {
                                    fa.fill(StreamKind::Audio, pos, "Channels", channels.to_string(), false);
                                    let (positions, layout) = channel_layout_for_codec(channels as u16, "A_OPUS");
                                    if let Some(p) = positions {
                                        fa.fill(StreamKind::Audio, pos, "ChannelPositions", p, false);
                                    }
                                    if let Some(l) = layout {
                                        fa.fill(StreamKind::Audio, pos, "ChannelLayout", l, false);
                                    }
                                }
                            }
                        }
                    }
                }

                // Parse CodecPrivate for Vorbis to get sample rate and channels
                if track.codec_id.as_deref() == Some("A_VORBIS") {
                    if let Some(ref private) = track.codec_private {
                        // Vorbis identification header starts with packet type (1) + "vorbis"
                        // But in MKV, CodecPrivate for Vorbis contains 3 Vorbis headers packed together
                        // Format: [id_header_length] [id_header] [comment_header_length] [comment_header] [setup_header_length] [setup_header]
                        // Lengths are 4-byte LE
                        if private.len() >= 4 + 7 + 11 {
                            let id_len = (private[0] as usize) 
                                | ((private[1] as usize) << 8)
                                | ((private[2] as usize) << 16)
                                | ((private[3] as usize) << 24);
                            
                            if id_len >= 7 && private.len() >= 4 + id_len {
                                let id_header = &private[4..4 + id_len];
                                
                                // Check for Vorbis identification header: packet type 1 + "vorbis"
                                if id_header.len() >= 7 && id_header[0] == 1 && &id_header[1..7] == b"vorbis" {
                                    if id_header.len() >= 11 {
                                        // vorbis_version (4 bytes LE, should be 0)
                                        let vorbis_version = (id_header[7] as u32)
                                            | ((id_header[8] as u32) << 8)
                                            | ((id_header[9] as u32) << 16)
                                            | ((id_header[10] as u32) << 24);
                                        
                                        if vorbis_version == 0 && id_header.len() >= 14 {
                                            // audio_channels (1 byte)
                                            let channels = id_header[11];
                                            // sample_rate (4 bytes LE)
                                            let sample_rate = (id_header[12] as u32)
                                                | ((id_header[13] as u32) << 8)
                                                | ((id_header[14] as u32) << 16)
                                                | ((id_header[15] as u32) << 24);
                                            
                                            if sample_rate > 0 {
                                                fa.fill(StreamKind::Audio, pos, "SamplingRate", sample_rate.to_string(), true);
                                            }
                                            
                                            if track.audio_channels.is_none() && channels > 0 {
                                                fa.fill(StreamKind::Audio, pos, "Channels", channels.to_string(), false);
                                                let (positions, layout) = channel_layout_for_codec(channels as u16, "A_VORBIS");
                                                if let Some(p) = positions {
                                                    fa.fill(StreamKind::Audio, pos, "ChannelPositions", p, false);
                                                }
                                                if let Some(l) = layout {
                                                    fa.fill(StreamKind::Audio, pos, "ChannelLayout", l, false);
                                                }
                                            }
                                            
                                            // bit_rate_maximum (4 bytes LE, signed) - offset 16
                                            if id_header.len() >= 20 {
                                                let bitrate_max = (id_header[16] as i32)
                                                    | ((id_header[17] as i32) << 8)
                                                    | ((id_header[18] as i32) << 16)
                                                    | ((id_header[19] as i32) << 24);
                                                if bitrate_max > 0 {
                                                    fa.fill(StreamKind::Audio, pos, "BitRate_Maximum", bitrate_max.to_string(), false);
                                                }
                                            }
                                            
                                            // bit_rate_nominal (4 bytes LE, signed) - offset 20
                                            if id_header.len() >= 24 {
                                                let bitrate_nominal = (id_header[20] as i32)
                                                    | ((id_header[21] as i32) << 8)
                                                    | ((id_header[22] as i32) << 16)
                                                    | ((id_header[23] as i32) << 24);
                                                if bitrate_nominal > 0 {
                                                    fa.fill(StreamKind::Audio, pos, "BitRate", bitrate_nominal.abs().to_string(), false);
                                                    fa.fill(StreamKind::Audio, pos, "BitRate_Mode", "CBR", false);
                                                } else {
                                                    fa.fill(StreamKind::Audio, pos, "BitRate_Mode", "VBR", false);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                if let Some(c) = track.codec_id.as_deref() {
                    if codec_is_lossy(c) {
                        fa.fill(StreamKind::Audio, pos, "Compression_Mode", "Lossy", false);
                    } else if codec_is_lossless(c) {
                        fa.fill(StreamKind::Audio, pos, "Compression_Mode", "Lossless", false);
                    }
                }
                if let Some(d) = track.flag_default {
                    fa.fill(StreamKind::Audio, pos, "Default", if d { "Yes" } else { "No" }, false);
                }
                // FlagForced defaults to 0 in MKV; oracle always
                // emits this field for audio tracks, so default
                // missing values to "No".
                let forced = track.flag_forced.unwrap_or(false);
                fa.fill(StreamKind::Audio, pos, "Forced", if forced { "Yes" } else { "No" }, false);
                // For MKV, Delay defaults to 0.000s and Delay_Source is
                // "Container" — oracle emits these for every audio
                // track even when no explicit CodecDelay element is
                // present. For Opus, use the pre-skip from CodecPrivate
                // (samples at 48 kHz → delay_secs = preskip / 48000).
                let delay_secs = if track.codec_id.as_deref() == Some("A_OPUS") {
                    track.codec_private.as_ref()
                        .filter(|p| p.len() >= 12 && &p[0..8] == b"OpusHead")
                        .map(|p| {
                            let preskip = u16::from_le_bytes([p[10], p[11]]);
                            preskip as f64 / 48000.0
                        })
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                fa.fill(StreamKind::Audio, pos, "Delay", format!("{:.3}", delay_secs), false);
                fa.fill(StreamKind::Audio, pos, "Delay_Source", "Container", false);
                // MKV oracle emits Audio.Duration with 9 fractional
                // digits (the file's float precision). Store the
                // pre-formatted string here so the exporter's
                // ms-to-seconds conversion doesn't touch it.
                if let Some(s) = duration_seconds {
                    fa.fill(
                        StreamKind::Audio,
                        pos,
                        "Duration",
                        format!("{:.9}", s),
                        false,
                    );
                }
                if let Some(lang) = track.language.as_deref().and_then(iso639_emit) {
                    fa.fill(StreamKind::Audio, pos, "Language", lang, false);
                }
                let audio_default = track.flag_default.unwrap_or(true);
                fa.fill(
                    StreamKind::Audio,
                    pos,
                    "Default",
                    if audio_default { "Yes" } else { "No" },
                    false,
                );
                if let Some(f) = track.flag_forced {
                    fa.fill(StreamKind::Audio, pos, "Forced", if f { "Yes" } else { "No" }, false);
                }
                // Pull Encoded_Library from a matching ENCODER tag.
                // Prefer a track-targeted tag over a global one (files
                // can carry both a muxer-level ENCODER like "Lavf..."
                // and a codec-level ENCODER like "Lavc... libopus").
                if let Some(track_uid) = track.uid {
                    let mut track_value: Option<&str> = None;
                    let mut global_value: Option<&str> = None;
                    for tag in tag_pairs {
                        if !tag.name.eq_ignore_ascii_case("ENCODER") {
                            continue;
                        }
                        if tag.target_track_uid == track_uid {
                            track_value = Some(&tag.value);
                            break;
                        }
                        if tag.target_track_uid == 0 && global_value.is_none() {
                            global_value = Some(&tag.value);
                        }
                    }
                    if let Some(v) = track_value.or(global_value) {
                        fa.fill(
                            StreamKind::Audio,
                            pos,
                            "Encoded_Library",
                            v.to_string(),
                            false,
                        );
                    }
                }
                audio_count += 1;
            }
            Some(1) => {
                let pos = fa.stream_prepare(StreamKind::Video);
                fa.fill(StreamKind::Video, pos, "StreamOrder", stream_order.to_string(), false);
                stream_order += 1;
                if let Some(n) = track.number {
                    fa.fill(StreamKind::Video, pos, "ID", n.to_string(), false);
                }
                if let Some(uid) = track.uid {
                    fa.fill(StreamKind::Video, pos, "UniqueID", uid.to_string(), false);
                }
                if let Some(c) = track.codec_id.as_deref() {
                    if let Some(fmt) = mkv_codec_to_format(c) {
                        fa.fill(StreamKind::Video, pos, "Format", fmt, false);
                    }
                    fa.fill(StreamKind::Video, pos, "CodecID", c, false);
                }
                if let Some(w) = track.video_width {
                    fa.fill(StreamKind::Video, pos, "Width", w.to_string(), false);
                    let display_w = track.display_width.unwrap_or(w);
                    fa.fill(StreamKind::Video, pos, "Sampled_Width", display_w.to_string(), false);
                }
                if let Some(h) = track.video_height {
                    fa.fill(StreamKind::Video, pos, "Height", h.to_string(), false);
                    let display_h = track.display_height.unwrap_or(h);
                    fa.fill(StreamKind::Video, pos, "Sampled_Height", display_h.to_string(), false);
                }
                if let (Some(w), Some(h)) = (track.video_width, track.video_height) {
                    let dw = track.display_width.unwrap_or(w) as f64;
                    let dh = track.display_height.unwrap_or(h) as f64;
                    if dw > 0.0 && dh > 0.0 {
                        let par = (dw / w as f64) / (dh / h as f64);
                        let dar = dw / dh;
                        fa.fill(StreamKind::Video, pos, "PixelAspectRatio", format!("{:.3}", par), false);
                        fa.fill(StreamKind::Video, pos, "DisplayAspectRatio", format!("{:.3}", dar), false);
                    }
                }
                // Container-level Duration → Video.
                if let Some(s) = duration_seconds {
                    fa.fill(StreamKind::Video, pos, "Duration", format!("{:.9}", s), false);
                }
                // FrameRate from DefaultDuration. 1 frame per ns →
                // frame_rate = 1e9 / default_duration_ns. CFR when
                // DefaultDuration is set (MKV doesn't expose per-frame
                // deltas without walking clusters).
                if let Some(ns) = track.default_duration_ns {
                    if ns > 0 {
                        let fr = 1_000_000_000.0 / ns as f64;
                        fa.fill(StreamKind::Video, pos, "FrameRate_Mode", "CFR", false);
                        fa.fill(StreamKind::Video, pos, "FrameRate", format!("{fr:.3}"), false);
                        // FrameCount = Duration / DefaultDuration.
                        if let Some(s) = duration_seconds {
                            let fc = (s * 1_000_000_000.0 / ns as f64).round() as u64;
                            fa.fill(StreamKind::Video, pos, "FrameCount", fc.to_string(), false);
                        }
                    }
                }
                if let Some(bd) = track.video_bit_depth {
                    fa.fill(StreamKind::Video, pos, "BitDepth", bd.to_string(), false);
                }
                // ColorSpace=YUV is the universal default for all formats
                // we recognize in MKV (AVC/HEVC/VP9/AV1).
                
                // For AV1, try to parse codec config for more accurate info.
                if track.codec_id.as_deref() == Some("V_AV1") {
                    if let Some(ref private) = track.codec_private {
                        if let Some(info) = revelio_parsers_video::parse_av1_from_codec_config(private) {
                            // MKV's container PixelWidth/PixelHeight and
                            // BitsPerChannel are authoritative — oracle
                            // reports those, not the AV1 sequence header.
                            // Only fill from the codec config when the
                            // container didn't supply them (avoids the
                            // AV1-OBU parse clobbering 1080x1920 with the
                            // partially-decoded sequence-header dims).
                            if info.width > 0 && track.video_width.is_none() {
                                fa.fill(StreamKind::Video, pos, "Width", info.width.to_string(), true);
                            }
                            if info.height > 0 && track.video_height.is_none() {
                                fa.fill(StreamKind::Video, pos, "Height", info.height.to_string(), true);
                            }
                            if track.video_bit_depth.is_none() {
                                fa.fill(StreamKind::Video, pos, "BitDepth", info.bit_depth.to_string(), true);
                            }
                            fa.fill(StreamKind::Video, pos, "ChromaSubsampling", info.chroma_subsampling, false);
                            let profile_name = match info.profile {
                                0 => "Main",
                                1 => "High",
                                2 => "Professional",
                                _ => "Unknown",
                            };
                            fa.fill(StreamKind::Video, pos, "Format_Profile", profile_name, false);
                        }
                    }
                }

                // For VP9, try to parse CodecPrivate for profile, level, bit depth.
                if track.codec_id.as_deref() == Some("V_VP9") {
                    if let Some(ref private) = track.codec_private {
                        if private.len() >= 4 {
                            let profile = private[1];
                            let level = private[2];
                            let config_byte = private[3];
                            let bit_depth = (config_byte >> 4) & 0x0F;
                            let chroma_subsampling = (config_byte >> 1) & 0x07;
                            let video_full_range = config_byte & 1;
                            
                            fa.fill(StreamKind::Video, pos, "Format_Profile", profile.to_string(), false);
                            fa.fill(StreamKind::Video, pos, "Format_Level", format!("{:.1}", level as f64 / 10.0), false);
                            if track.video_bit_depth.is_none() {
                                fa.fill(StreamKind::Video, pos, "BitDepth", bit_depth.to_string(), true);
                            }
                            let chroma = match chroma_subsampling {
                                1 => "4:2:0",
                                2 => "4:2:2",
                                3 => "4:4:0",
                                _ => "4:2:0",
                            };
                            fa.fill(StreamKind::Video, pos, "ChromaSubsampling", chroma, false);
                            let range = if video_full_range != 0 { "Full" } else { "Limited" };
                            fa.fill(StreamKind::Video, pos, "colour_range", range, false);
                        }
                    }
                }
                
                // For AVC tracks with CodecPrivate, parse avcC to get profile/level
                if track.codec_id.as_deref() == Some("V_DOLBYVISION/AVC") {
                    fa.fill(StreamKind::Video, pos, "HDR_Format", "Dolby Vision", false);
                    fa.fill(StreamKind::Video, pos, "HDR_Format_Compatibility", "AVC", false);
                }

                if track.codec_id.as_deref() == Some("V_MPEG4/ISO/AVC") {
                    if let Some(ref private) = track.codec_private {
                        if private.len() >= 4 && private[0] == 1 {
                            let profile_idc = private[1];
                            let level_idc = private[3];
                            
                            let profile_name = match profile_idc {
                                66 => "Baseline",
                                77 => "Main",
                                88 => "Extended",
                                100 => "High",
                                110 => "High 10",
                                122 => "High 4:2:2",
                                244 => "High 4:4:4",
                                44 => "CAVLC 4:4:4",
                                _ => "Unknown",
                            };
                            if profile_name != "Unknown" {
                                fa.fill(StreamKind::Video, pos, "Format_Profile", profile_name, false);
                            }
                            fa.fill(StreamKind::Video, pos, "Format_Level", format!("{}.{:02}", level_idc / 10, level_idc % 10), false);
                            
                            // Parse SPS for dimensions and colour info
                            if private.len() >= 8 {
                                let num_sps = (private[5] & 0x1F) as usize;
                                let mut sps_offset = 6;
                                for _ in 0..num_sps {
                                    if sps_offset + 2 > private.len() { break; }
                                    let sps_len = ((private[sps_offset] as usize) << 8) | (private[sps_offset + 1] as usize);
                                    sps_offset += 2;
                                    if sps_offset + sps_len > private.len() { break; }
                                    let sps_data = &private[sps_offset..sps_offset + sps_len];
                                    if let Some(sps_info) = revelio_parsers_video::parse_avc_sps(sps_data) {
                                        if sps_info.width > 0 {
                                            fa.fill(StreamKind::Video, pos, "Width", sps_info.width.to_string(), true);
                                        }
                                        if sps_info.height > 0 {
                                            fa.fill(StreamKind::Video, pos, "Height", sps_info.height.to_string(), true);
                                        }
                                        if sps_info.colour_description_present {
                                            if let Some(cp) = sps_info.colour_primaries.and_then(|v| cicp_primaries(v as u16)) {
                                                fa.fill(StreamKind::Video, pos, "colour_primaries", cp, false);
                                            }
                                            if let Some(tc) = sps_info.transfer_characteristics.and_then(|v| cicp_transfer(v as u16)) {
                                                fa.fill(StreamKind::Video, pos, "transfer_characteristics", tc, false);
                                            }
                                            if let Some(mc) = sps_info.matrix_coefficients.and_then(|v| cicp_matrix(v as u16)) {
                                                fa.fill(StreamKind::Video, pos, "matrix_coefficients", mc, false);
                                            }
                                            if let Some(vfr) = sps_info.video_full_range {
                                                fa.fill(StreamKind::Video, pos, "colour_range", if vfr { "Full" } else { "Limited" }, false);
                                            }
                                        }
                                    }
                                    let _ = sps_offset;
                                    break; // Only parse first SPS
                                }
                            }
                        }
                    }
                }

                // For HEVC tracks with CodecPrivate, parse hvcC to get profile/tier/level
                // Dolby Vision tracks with HEVC base layer
                if track.codec_id.as_deref() == Some("V_DOLBYVISION/HEVC") || track.codec_id.as_deref() == Some("V_DOLBYVISION") {
                    fa.fill(StreamKind::Video, pos, "HDR_Format", "Dolby Vision", false);
                    fa.fill(StreamKind::Video, pos, "HDR_Format_Compatibility", "HDR10", false);
                    // If we have CodecPrivate with hvcC, also parse HEVC details
                    if let Some(ref private) = track.codec_private {
                        if let Some(info) = revelio_parsers_video::parse_hevc_sps(private) {
                            fa.fill(StreamKind::Video, pos, "Width", info.width.to_string(), false);
                            fa.fill(StreamKind::Video, pos, "Height", info.height.to_string(), false);
                            fa.fill(StreamKind::Video, pos, "BitDepth", info.bit_depth.to_string(), false);
                        }
                    }
                }

                if track.codec_id.as_deref() == Some("V_MPEGH/ISO/HEVC") {
                    if let Some(ref private) = track.codec_private {
                        if private.len() >= 23 && private[0] == 1 {
                            let profile_idc = private[1] & 0x1F;
                            let tier_flag = (private[1] >> 5) & 1;
                            let level_idc = private[20];
                            
                            let profile_name = match profile_idc {
                                1 => "Main",
                                2 => "Main 10",
                                3 => "Main Still Picture",
                                4 => "Main 12",
                                5 => "Main 4:2:2 10",
                                6 => "Main 4:2:2 12",
                                7 => "Main 4:4:4",
                                8 => "Main 4:4:4 10",
                                9 => "Main 4:4:4 12",
                                _ => "Unknown",
                            };
                            if profile_name != "Unknown" {
                                fa.fill(StreamKind::Video, pos, "Format_Profile", profile_name, false);
                            }
                            
                            let tier_name = if tier_flag == 0 { "Main" } else { "High" };
                            fa.fill(StreamKind::Video, pos, "Format_Tier", tier_name, false);
                            
                            let level_str = if level_idc % 10 == 0 {
                                format!("{}.0", level_idc / 30)
                            } else {
                                format!("{}.{}", level_idc / 30, (level_idc % 30) / 3)
                            };
                            fa.fill(StreamKind::Video, pos, "Format_Level", level_str, false);
                            
                            // Parse NAL arrays for SPS and SEI
                            if private.len() > 23 {
                                let num_arrays = private[22] as usize;
                                let mut offset = 23usize;
                                let mut sps_data: Option<Vec<u8>> = None;
                                let mut sei_nalus: Vec<Vec<u8>> = Vec::new();
                                
                                for _ in 0..num_arrays {
                                    if offset >= private.len() { break; }
                                    let array_header = private[offset];
                                    let nal_type = array_header & 0x3F;
                                    offset += 1;
                                    
                                    if offset + 2 > private.len() { break; }
                                    let num_nalus = ((private[offset] as usize) << 8) | (private[offset + 1] as usize);
                                    offset += 2;
                                    
                                    for _ in 0..num_nalus {
                                        if offset + 2 > private.len() { break; }
                                        let nal_len = ((private[offset] as usize) << 8) | (private[offset + 1] as usize);
                                        offset += 2;
                                        
                                        if offset + nal_len > private.len() { break; }
                                        let nal_data = private[offset..offset + nal_len].to_vec();
                                        
                                        match nal_type {
                                            33 => if sps_data.is_none() { sps_data = Some(nal_data); },
                                            39 | 40 => sei_nalus.push(nal_data),
                                            _ => {}
                                        }
                                        offset += nal_len;
                                    }
                                }
                                
                                // Parse SPS for dimensions and colour
                                if let Some(sps) = sps_data {
                                    if let Some(sps_info) = revelio_parsers_video::parse_hevc_sps(&sps) {
                                        if sps_info.width > 0 {
                                            fa.fill(StreamKind::Video, pos, "Width", sps_info.width.to_string(), true);
                                        }
                                        if sps_info.height > 0 {
                                            fa.fill(StreamKind::Video, pos, "Height", sps_info.height.to_string(), true);
                                        }
                                        if sps_info.colour_description_present {
                                            if let Some(cp) = sps_info.colour_primaries.and_then(|v| cicp_primaries(v as u16)) {
                                                fa.fill(StreamKind::Video, pos, "colour_primaries", cp, false);
                                            }
                                            if let Some(tc) = sps_info.transfer_characteristics.and_then(|v| cicp_transfer(v as u16)) {
                                                fa.fill(StreamKind::Video, pos, "transfer_characteristics", tc, false);
                                            }
                                            if let Some(mc) = sps_info.matrix_coefficients.and_then(|v| cicp_matrix(v as u16)) {
                                                fa.fill(StreamKind::Video, pos, "matrix_coefficients", mc, false);
                                            }
                                        }
                                        if let Some(vfr) = sps_info.video_full_range {
                                            fa.fill(StreamKind::Video, pos, "colour_range", if vfr { "Full" } else { "Limited" }, false);
                                        }
                                    }
                                }
                                
                                // Extract encoder from SEI
                                if !sei_nalus.is_empty() {
                                    let refs: Vec<&[u8]> = sei_nalus.iter().map(|v| v.as_slice()).collect();
                                    if let Some(encoder) = revelio_parsers_video::extract_encoder_from_sei_nalus(&refs) {
                                        fa.fill(StreamKind::Video, pos, "Encoded_Library", encoder.library.as_str(), false);
                                    }
                                }
                            }
                        }
                    }
                }
                
                fa.fill(StreamKind::Video, pos, "ColorSpace", "YUV", false);
                // ChromaSubsampling default: 4:2:0 (no Colour element in
                // MKV explicitly carries this; the codec implies it).
                fa.fill(StreamKind::Video, pos, "ChromaSubsampling", "4:2:0", false);
                // Colour element → colour_* fields, marked _Source=
                // "Container / Stream" (oracle's label when present in
                // both container Colour and codec VUI).
                let has_colour = track.colour_primaries.is_some()
                    || track.colour_transfer.is_some()
                    || track.colour_matrix.is_some()
                    || track.colour_range.is_some();
                if has_colour {
                    fa.fill(StreamKind::Video, pos, "colour_description_present", "Yes", false);
                    fa.fill(StreamKind::Video, pos, "colour_description_present_Source", "Container / Stream", false);
                }
                if let Some(r) = track.colour_range {
                    let s = match r {
                        1 => "Limited",
                        2 => "Full",
                        _ => "",
                    };
                    if !s.is_empty() {
                        fa.fill(StreamKind::Video, pos, "colour_range", s, false);
                        fa.fill(StreamKind::Video, pos, "colour_range_Source", "Container / Stream", false);
                    }
                }
                if let Some(p) = track.colour_primaries.and_then(|v| cicp_primaries(v as u16)) {
                    fa.fill(StreamKind::Video, pos, "colour_primaries", p, false);
                    fa.fill(StreamKind::Video, pos, "colour_primaries_Source", "Container / Stream", false);
                }
                if let Some(t) = track.colour_transfer.and_then(|v| cicp_transfer(v as u16)) {
                    fa.fill(StreamKind::Video, pos, "transfer_characteristics", t, false);
                    fa.fill(StreamKind::Video, pos, "transfer_characteristics_Source", "Container / Stream", false);
                }
                if let Some(m) = track.colour_matrix.and_then(|v| cicp_matrix(v as u16)) {
                    fa.fill(StreamKind::Video, pos, "matrix_coefficients", m, false);
                    fa.fill(StreamKind::Video, pos, "matrix_coefficients_Source", "Container / Stream", false);
                }
                if let Some(name) = track.name.as_deref() {
                    fa.fill(StreamKind::Video, pos, "Title", name, false);
                }
                if let Some(lang) = track.language.as_deref().and_then(iso639_emit) {
                    fa.fill(StreamKind::Video, pos, "Language", lang, false);
                }
                let video_default = track.flag_default.unwrap_or(true);
                fa.fill(
                    StreamKind::Video,
                    pos,
                    "Default",
                    if video_default { "Yes" } else { "No" },
                    false,
                );
                let forced = track.flag_forced.unwrap_or(false);
                fa.fill(StreamKind::Video, pos, "Forced", if forced { "Yes" } else { "No" }, false);
                video_count += 1;
            }
            _ => {}
        }
    }

    if audio_count > 0 {
        fa.fill(StreamKind::General, 0, "AudioCount", audio_count.to_string(), false);
    }
    if video_count > 0 {
        fa.fill(StreamKind::General, 0, "VideoCount", video_count.to_string(), false);
    }
    // Chapter/Menu count
    if movie.chapter_count > 0 {
        fa.fill(StreamKind::General, 0, "MenuCount", "1", false);
    }
    // Cover art detection
    if movie.has_cover_art {
        fa.fill(StreamKind::General, 0, "Cover", "Yes", false);
        if let Some(ref mime) = movie.cover_mime_type {
            let cover_type = if mime.contains("png") {
                "PNG"
            } else if mime.contains("jpeg") || mime.contains("jpg") {
                "JPG"
            } else {
                "Unknown"
            };
            fa.fill(StreamKind::General, 0, "Cover_Type", cover_type, false);
            fa.fill(StreamKind::General, 0, "Cover_Mime", mime.clone(), false);
        }
    }
    if let Some(ms) = duration_ms {
        fa.fill(StreamKind::General, 0, "Duration", ms.to_string(), false);
        
        // Calculate OverallBitRate = FileSize * 8 / Duration_ms * 1000
        if ms > 0 && file_size > 0 {
            let overall_bitrate = (file_size as u64 * 8 * 1000) / ms;
            fa.fill(StreamKind::General, 0, "OverallBitRate", overall_bitrate.to_string(), false);
            fa.fill(StreamKind::General, 0, "OverallBitRate_Mode", "VBR", false);
        }
    }
}

/// Map ISO 639-2 three-letter code to oracle's emitted language form.
/// MKV `Language` element follows the same scheme as MP4's mdhd.
fn iso639_emit(code: &str) -> Option<&'static str> {
    match code {
        "und" => None,
        "eng" => Some("en"),
        "spa" => Some("es"),
        "fre" | "fra" => Some("fr"),
        "ger" | "deu" => Some("de"),
        "ita" => Some("it"),
        "jpn" => Some("ja"),
        "kor" => Some("ko"),
        "chi" | "zho" => Some("zh"),
        "rus" => Some("ru"),
        "por" => Some("pt"),
        "dut" | "nld" => Some("nl"),
        "ara" => Some("ar"),
        "hin" => Some("hi"),
        _ => None,
    }
}

/// CICP color_primaries → MediaInfo string.
fn cicp_primaries(idc: u16) -> Option<&'static str> {
    match idc {
        1 => Some("BT.709"),
        4 => Some("BT.470 System M"),
        5 => Some("BT.601 PAL"),
        6 => Some("BT.601 NTSC"),
        7 => Some("SMPTE 240M"),
        9 => Some("BT.2020"),
        _ => None,
    }
}

fn cicp_transfer(idc: u16) -> Option<&'static str> {
    match idc {
        1 => Some("BT.709"),
        6 => Some("BT.601"),
        7 => Some("SMPTE 240M"),
        13 => Some("IEC 61966-2-1"),
        14 => Some("BT.2020 (10-bit)"),
        15 => Some("BT.2020 (12-bit)"),
        16 => Some("PQ"),
        18 => Some("HLG"),
        _ => None,
    }
}

fn cicp_matrix(idc: u16) -> Option<&'static str> {
    match idc {
        0 => Some("Identity"),
        1 => Some("BT.709"),
        6 => Some("BT.601"),
        7 => Some("SMPTE 240M"),
        9 => Some("BT.2020 non-constant"),
        10 => Some("BT.2020 constant"),
        _ => None,
    }
}

fn channel_layout(channels: u16) -> (Option<&'static str>, Option<&'static str>) {
    match channels {
        1 => (Some("Front: C"), Some("C")),
        2 => (Some("Front: L R"), Some("L R")),
        _ => (None, None),
    }
}

/// Channel-layout strings vary by codec. Vorbis/Opus mono is "M"
/// (matches oracle for Ogg/WebM); AC-3-style mono uses "C".
fn channel_layout_for_codec(
    channels: u16,
    codec_id: &str,
) -> (Option<&'static str>, Option<&'static str>) {
    if matches!(codec_id, "A_OPUS" | "A_VORBIS") && channels == 1 {
        return (Some("Front: C"), Some("M"));
    }
    channel_layout(channels)
}

fn mkv_codec_to_format(codec_id: &str) -> Option<&'static str> {
    match codec_id {
        "A_OPUS" => Some("Opus"),
        "A_VORBIS" => Some("Vorbis"),
        "A_AAC" | "A_AAC/MPEG2/MAIN" | "A_AAC/MPEG2/LC" | "A_AAC/MPEG4/LC" | "A_AAC/MPEG4/LC/SBR" => {
            Some("AAC")
        }
        "A_AC3" => Some("AC-3"),
        "A_EAC3" => Some("E-AC-3"),
        "A_DTS" => Some("DTS"),
        "A_FLAC" => Some("FLAC"),
        "A_MPEG/L3" => Some("MPEG Audio"),
        "A_MPEG/L2" => Some("MPEG Audio"),
        "A_PCM/INT/LIT" | "A_PCM/INT/BIG" | "A_PCM/FLOAT/IEEE" => Some("PCM"),
        "V_MPEG4/ISO/AVC" => Some("AVC"),
        "V_MPEGH/ISO/HEVC" => Some("HEVC"),
        "V_VP9" => Some("VP9"),
        "V_AV1" => Some("AV1"),
        "V_DOLBYVISION" => Some("Dolby Vision"),
        "V_DOLBYVISION/AVC" => Some("Dolby Vision"),
        "V_DOLBYVISION/HEVC" => Some("Dolby Vision"),
        _ => None,
    }
}

fn codec_is_lossy(codec_id: &str) -> bool {
    matches!(
        codec_id,
        "A_OPUS"
            | "A_VORBIS"
            | "A_AAC"
            | "A_AAC/MPEG2/MAIN"
            | "A_AAC/MPEG2/LC"
            | "A_AAC/MPEG4/LC"
            | "A_AAC/MPEG4/LC/SBR"
            | "A_AC3"
            | "A_EAC3"
            | "A_DTS"
            | "A_MPEG/L3"
            | "A_MPEG/L2"
    )
}

fn codec_is_lossless(codec_id: &str) -> bool {
    matches!(
        codec_id,
        "A_FLAC" | "A_PCM/INT/LIT" | "A_PCM/INT/BIG" | "A_PCM/FLOAT/IEEE"
    )
}

fn strip_nuls(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).into_owned();
    s.trim_end_matches('\0').to_string()
}

fn read_uint(fa: &mut FileAnalyze, size: usize) -> u64 {
    let bytes = fa.read_raw(size);
    let mut v: u64 = 0;
    for b in bytes {
        v = (v << 8) | (*b as u64);
    }
    v
}

fn read_float(fa: &mut FileAnalyze, size: usize) -> f64 {
    match size {
        4 => {
            let mut v: zenlib::float32 = 0.0;
            fa.get_bf4(&mut v, "f32");
            v as f64
        }
        8 => {
            let mut v: zenlib::float64 = 0.0;
            fa.get_bf8(&mut v, "f64");
            v
        }
        _ => {
            fa.skip_hexa(size, "unknown_float_size");
            0.0
        }
    }
}

/// Read an EBML VINT representing an element ID — keeps the leading
/// 1-bit so the returned value matches the canonical IDs like
/// 0x1A45DFA3.
fn read_vint_id(fa: &mut FileAnalyze) -> Option<u64> {
    let first_bytes = fa.peek_raw(1)?;
    let first = first_bytes[0];
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let bytes = fa.read_raw(len);
    if bytes.len() < len {
        return None;
    }
    let mut v: u64 = 0;
    for b in bytes {
        v = (v << 8) | (*b as u64);
    }
    Some(v)
}

/// Read an EBML VINT representing a size — strips the leading 1-bit
/// to return the actual numeric value.
fn read_vint_size(fa: &mut FileAnalyze) -> Option<u64> {
    let first_bytes = fa.peek_raw(1)?;
    let first = first_bytes[0];
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let bytes = fa.read_raw(len);
    if bytes.len() < len {
        return None;
    }
    // Strip the leading 1 in the first byte (at bit position 8-len).
    let marker_mask: u8 = if len == 8 { 0 } else { !(0xFF << (8 - len)) };
    let mut v: u64 = (bytes[0] & marker_mask) as u64;
    for b in &bytes[1..] {
        v = (v << 8) | (*b as u64);
    }
    Some(v)
}

/// Walk EBML elements within a region. Visitor receives
/// (fa, element_id, body_size, element_start_offset).
fn walk_elements(
    fa: &mut FileAnalyze,
    region_size: usize,
    visit: &mut dyn FnMut(&mut FileAnalyze, u64, usize, usize),
) {
    let region_end = fa.element_offset() + region_size;
    while fa.element_offset() < region_end && fa.remain() > 0 {
        let elem_start = fa.element_offset();
        let Some(id) = read_vint_id(fa) else { break };
        let Some(size) = read_vint_size(fa) else { break };
        let body_size = size as usize;
        if fa.element_offset() + body_size > region_end {
            // Truncated — bail.
            break;
        }
        let body_end = fa.element_offset() + body_size;
        visit(fa, id, body_size, elem_start);
        if fa.element_offset() < body_end {
            fa.skip_hexa(body_end - fa.element_offset(), "element_tail");
        } else if fa.element_offset() > body_end {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vint_single_byte_id() {
        let buf = [0x82];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_id(&mut fa), Some(0x82));
    }

    #[test]
    fn vint_two_byte_id() {
        let buf = [0x42, 0x86];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_id(&mut fa), Some(0x4286));
    }

    #[test]
    fn vint_four_byte_id() {
        let buf = [0x1A, 0x45, 0xDF, 0xA3];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_id(&mut fa), Some(0x1A45DFA3));
    }

    #[test]
    fn vint_size_strips_marker_bit() {
        // 0x82 = 10000010, len=1, value with marker stripped = 0b0000010 = 2
        let buf = [0x82];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_size(&mut fa), Some(2));

        // 0x40 0x05 = 01000000 00000101, len=2, value = 5
        let buf = [0x40, 0x05];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(read_vint_size(&mut fa), Some(5));
    }

    #[test]
    fn rejects_non_ebml_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a Matroska file at all");
        assert!(!parse_mkv(&mut fa));
    }
}
