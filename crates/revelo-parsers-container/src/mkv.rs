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

use revelo_core::{FileAnalyze, Reader, StreamKind};

const MKV_TEXT_VALUE_LIMIT: usize = 16 * 1024;
const MKV_CODEC_PRIVATE_LIMIT: usize = 256 * 1024;

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
const SIMPLE_BLOCK: u64 = 0xA3;
const BLOCK_GROUP: u64 = 0xA0;
const BLOCK: u64 = 0xA1;
const SEEK_HEAD: u64 = 0x114D9B74;
const TAGS: u64 = 0x1254C367;
const CHAPTERS: u64 = 0x1043A770;
const ATTACHMENTS: u64 = 0x1941A469;
const EDITION_ENTRY: u64 = 0x45B9;
const CHAPTER_ATOM: u64 = 0xB6;
const CHAPTER_TIME_START: u64 = 0x91;
#[allow(dead_code)]
const CHAPTER_TIME_END: u64 = 0x92;
const CHAPTER_DISPLAY: u64 = 0x80;
const CHAP_STRING: u64 = 0x85;
const CHAP_LANGUAGE: u64 = 0x437C;
const ATTACHED_FILE: u64 = 0x61A7;
const FILE_DESCRIPTION: u64 = 0x467E; // File description
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
const DATE_UTC: u64 = 0x4461;
const TITLE: u64 = 0x7BA9;
const MUXING_APP: u64 = 0x4D80;
const WRITING_APP: u64 = 0x5741;

// Tracks > TrackEntry.
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_UID: u64 = 0x73C5;
const TRACK_TYPE: u64 = 0x83;
const FLAG_DEFAULT: u64 = 0x88;
const FLAG_FORCED: u64 = 0x55AA;
const FLAG_HEARING_IMPAIRED: u64 = 0x55AB;
const FLAG_VISUAL_IMPAIRED: u64 = 0x55AC;
const FLAG_TEXT_DESCRIPTIONS: u64 = 0x55AD;
const FLAG_ORIGINAL: u64 = 0x55AE;
const FLAG_COMMENTARY: u64 = 0x55AF;
const CODEC_ID: u64 = 0x86;
const CODEC_PRIVATE: u64 = 0x63A2;
const AUDIO_BLOCK: u64 = 0xE1;
const SAMPLING_FREQUENCY: u64 = 0xB5;
const CHANNELS: u64 = 0x9F;
const BIT_DEPTH: u64 = 0x6264;

// Track entry — additional children.
const LANGUAGE: u64 = 0x22B59C;
const LANGUAGE_BCP47: u64 = 0x22B59D;
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

/// Parse Matroska / WebM container (EBML).
///
/// Detection: EBML header + DocType = `matroska`/`webm`.
/// Fills: Info/Tracks/Chapters/Tags, CodecPrivate for AVC/HEVC/AV1/Opus.
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
    // First VP9/VP8 video keyframe's frame bytes (uncompressed header
    // prefix), extracted from the first Cluster. VP9-in-Matroska carries
    // no CodecPrivate, so profile/bit-depth must come from the bitstream —
    // this mirrors what MediaInfoLib does when it demuxes the elementary
    // stream.
    let mut vpx_keyframe: Option<Vec<u8>> = None;

    walk_elements(fa, file_size, &mut |fa, id, size, _start| {
        match id {
            EBML_HEADER => {
                walk_elements(fa, size, &mut |fa, id, sz, _| match id {
                    DOC_TYPE => {
                        doc_type = read_string_limited(fa, sz, "doc_type");
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
                            // Only descend into clusters while we still
                            // need the VP9/VP8 keyframe; otherwise skip
                            // wholesale (block payloads are irrelevant to
                            // the rest of the metadata).
                            let vpx_track = tracks.iter().find(|t| {
                                t.track_type == Some(1)
                                    && matches!(
                                        t.codec_id.as_deref(),
                                        Some("V_VP9") | Some("V_VP8")
                                    )
                            });
                            match vpx_track.and_then(|t| t.number) {
                                Some(track_num) if vpx_keyframe.is_none() => {
                                    walk_elements(fa, sz, &mut |fa, bid, bsz, _| {
                                        capture_vpx_keyframe(
                                            fa,
                                            bid,
                                            bsz,
                                            track_num,
                                            &mut vpx_keyframe,
                                        );
                                    });
                                }
                                _ => fa.skip_hexa(sz, "cluster"),
                            }
                        }
                        TAGS => parse_tags(fa, sz, &mut tag_pairs),
                        CHAPTERS => {
                            movie.editions = collect_chapters(fa, sz);
                        }
                        ATTACHMENTS => {
                            movie.attachments = collect_attachments(fa, sz);
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
        &MkvContainerInfo {
            doc_type: doc_type.as_deref().unwrap_or("matroska"),
            doc_type_version,
            is_streamable: seekhead_seen && seekhead_before_cluster,
            crc32_at_level1,
        },
        &tag_pairs,
        file_size,
        vpx_keyframe.as_deref(),
    );
    true
}

#[derive(Default)]
struct MovieInfo {
    editions: Vec<Vec<ChapterEntry>>,
    attachments: Vec<AttachmentInfo>,
    segment_uuid: Option<Vec<u8>>,
    timecode_scale: Option<u64>,
    duration_units: Option<f64>,
    title: Option<String>,
    /// `YYYY-MM-DD HH:MM:SS UTC`, from DateUTC.
    encoded_date: Option<String>,
    muxing_app: Option<String>,
    writing_app: Option<String>,
}

struct ChapterEntry {
    /// ChapterTimeStart in nanoseconds.
    start_ns: u64,
    /// Display text. Empty when the atom has a time and no ChapString.
    text: String,
}

struct AttachmentInfo {
    name: Option<String>,
    description: Option<String>,
    mime: Option<String>,
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
    /// BCP-47 tag from LanguageBCP47 / LanguageIETF, e.g. "pt-BR".
    language_bcp47: Option<String>,
    flag_hearing_impaired: bool,
    flag_visual_impaired: bool,
    flag_text_descriptions: bool,
    flag_original: bool,
    flag_commentary: bool,
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
                            name = read_string_limited(fa, sz, "tag_name").unwrap_or_default();
                        }
                        TAG_STRING => {
                            value = read_string_limited(fa, sz, "tag_string").unwrap_or_default();
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
                tag_pairs.push(TagEntry { target_track_uid: target_uid, name: n, value: v });
            }
        }
        _ => fa.skip_hexa(sz, "tags_child"),
    });
}

fn parse_segment_info(fa: &mut FileAnalyze, size: usize, movie: &mut MovieInfo) {
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        SEGMENT_UUID => {
            movie.segment_uuid = read_bytes_limited(fa, sz, 16, "segment_uuid");
        }
        TIMECODE_SCALE => {
            movie.timecode_scale = Some(read_uint(fa, sz));
        }
        DURATION => {
            movie.duration_units = Some(read_float(fa, sz));
        }
        DATE_UTC => {
            movie.encoded_date = read_date_utc(fa, sz);
        }
        TITLE => {
            movie.title = read_string_limited(fa, sz, "title");
        }
        MUXING_APP => {
            movie.muxing_app = read_string_limited(fa, sz, "muxing_app");
        }
        WRITING_APP => {
            movie.writing_app = read_string_limited(fa, sz, "writing_app");
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
        FLAG_HEARING_IMPAIRED => entry.flag_hearing_impaired = read_uint(fa, sz) != 0,
        FLAG_VISUAL_IMPAIRED => entry.flag_visual_impaired = read_uint(fa, sz) != 0,
        FLAG_TEXT_DESCRIPTIONS => entry.flag_text_descriptions = read_uint(fa, sz) != 0,
        FLAG_ORIGINAL => entry.flag_original = read_uint(fa, sz) != 0,
        FLAG_COMMENTARY => entry.flag_commentary = read_uint(fa, sz) != 0,
        CODEC_ID => {
            entry.codec_id = read_string_limited(fa, sz, "codec_id");
        }
        LANGUAGE => {
            entry.language = read_string_limited(fa, sz, "language");
        }
        LANGUAGE_BCP47 => {
            entry.language_bcp47 = read_string_limited(fa, sz, "language_bcp47");
        }
        NAME => {
            entry.name = read_string_limited(fa, sz, "track_name");
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
        CODEC_PRIVATE => {
            entry.codec_private =
                read_bytes_limited(fa, sz, MKV_CODEC_PRIVATE_LIMIT, "codec_private")
        }
        _ => fa.skip_hexa(sz, "trackentry_child"),
    });
}

/// One Menu stream per edition. Each chapter with a start time becomes a
/// `HH:MM:SS.mmm` field on that stream.
fn collect_chapters(fa: &mut FileAnalyze, size: usize) -> Vec<Vec<ChapterEntry>> {
    let mut editions = Vec::new();
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        EDITION_ENTRY => editions.push(collect_edition_chapters(fa, sz)),
        _ => fa.skip_hexa(sz, "chapters_child"),
    });
    editions
}

fn collect_edition_chapters(fa: &mut FileAnalyze, size: usize) -> Vec<ChapterEntry> {
    let mut chapters = Vec::new();
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        CHAPTER_ATOM => chapters.extend(collect_chapter_atom(fa, sz)),
        _ => fa.skip_hexa(sz, "edition_child"),
    });
    chapters
}

fn collect_chapter_atom(fa: &mut FileAnalyze, size: usize) -> Vec<ChapterEntry> {
    let mut start_ns = None;
    let mut displays = Vec::new();
    let mut nested = Vec::new();
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        CHAPTER_TIME_START => start_ns = Some(read_uint(fa, sz)),
        CHAPTER_DISPLAY => {
            if let Some(text) = chapter_display_text(fa, sz) {
                displays.push(text);
            }
        }
        CHAPTER_ATOM => nested.extend(collect_chapter_atom(fa, sz)),
        _ => fa.skip_hexa(sz, "chapteratom_child"),
    });
    let mut out = Vec::new();
    if let Some(start_ns) = start_ns {
        out.push(ChapterEntry { start_ns, text: displays.join(" - ") });
    }
    out.extend(nested);
    out
}

/// ChapString, prefixed with the 2-letter language when ChapLanguage is set.
fn chapter_display_text(fa: &mut FileAnalyze, size: usize) -> Option<String> {
    let mut lang = None;
    let mut name = None;
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        CHAP_STRING => name = read_string_limited(fa, sz, "chapter_string"),
        CHAP_LANGUAGE => lang = read_string_limited(fa, sz, "chapter_language"),
        _ => fa.skip_hexa(sz, "chapterdisplay_child"),
    });
    let name = name.unwrap_or_default();
    let mut text = String::new();
    if let Some(code) = lang.as_deref()
        && !code.is_empty()
        && !code.eq_ignore_ascii_case("und")
    {
        text.push_str(iso639_emit(code).unwrap_or(code));
        text.push(':');
    }
    text.push_str(&name);
    if text.is_empty() { None } else { Some(text) }
}

fn collect_attachments(fa: &mut FileAnalyze, size: usize) -> Vec<AttachmentInfo> {
    let mut attachments = Vec::new();
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        ATTACHED_FILE => attachments.push(read_attachment(fa, sz)),
        _ => fa.skip_hexa(sz, "attachments_child"),
    });
    attachments
}

fn read_attachment(fa: &mut FileAnalyze, size: usize) -> AttachmentInfo {
    let mut info = AttachmentInfo { name: None, description: None, mime: None };
    walk_elements(fa, size, &mut |fa, id, sz, _| match id {
        FILE_NAME => info.name = read_string_limited(fa, sz, "file_name"),
        FILE_MIME_TYPE => info.mime = read_string_limited(fa, sz, "file_mime_type"),
        FILE_DESCRIPTION => info.description = read_string_limited(fa, sz, "file_description"),
        _ => fa.skip_hexa(sz, "attachedfile_child"),
    });
    info
}

fn attachment_is_cover(att: &AttachmentInfo) -> bool {
    if let Some(name) = att.name.as_deref() {
        let lower = name.to_lowercase();
        if lower.contains("cover")
            || lower.contains("folder")
            || lower.contains("album")
            || lower.contains("front")
            || lower.ends_with(".jpg")
            || lower.ends_with(".jpeg")
            || lower.ends_with(".png")
        {
            return true;
        }
    }
    if att.mime.as_deref().is_some_and(|mime| mime.starts_with("image/")) {
        return true;
    }
    if let Some(desc) = att.description.as_deref() {
        let lower = desc.to_lowercase();
        if lower.contains("cover") || lower.contains("poster") {
            return true;
        }
    }
    false
}

/// EBML header and segment-level flags describing the Matroska container.
struct MkvContainerInfo<'a> {
    doc_type: &'a str,
    doc_type_version: u64,
    is_streamable: bool,
    crc32_at_level1: bool,
}

fn fill_streams(
    fa: &mut FileAnalyze,
    movie: &MovieInfo,
    tracks: &[TrackInfo],
    container: &MkvContainerInfo,
    tag_pairs: &[TagEntry],
    file_size: usize,
    vpx_keyframe: Option<&[u8]>,
) {
    let &MkvContainerInfo { doc_type, doc_type_version, is_streamable, crc32_at_level1 } =
        container;
    fa.stream_prepare(StreamKind::General);
    if let Some(uuid) = movie.segment_uuid.as_ref()
        && uuid.len() == 16
    {
        let mut v: u128 = 0;
        for b in uuid {
            v = (v << 8) | (*b as u128);
        }
        fa.set_field(StreamKind::General, 0, "UniqueID", v.to_string());
    }
    // DocType drives the Format string: webm files report "WebM",
    // matroska files report "Matroska".
    let fmt = if doc_type == "webm" { "WebM" } else { "Matroska" };
    fa.set_field(StreamKind::General, 0, "Format", fmt);
    // MediaInfoLib emits no InternetMediaType for Matroska/WebM.
    if doc_type_version > 0 {
        fa.set_field(StreamKind::General, 0, "Format_Version", doc_type_version.to_string());
    }
    // MuxingApp is the library (libmatroska). WritingApp is the application
    // (mkvmerge). MediaInfo stores them that way around.
    if let Some(app) = movie.muxing_app.as_deref() {
        fa.set_field(StreamKind::General, 0, "Encoded_Library", app);
    }
    if let Some(app) = movie.writing_app.as_deref() {
        fa.set_field(StreamKind::General, 0, "Encoded_Application", app);
    }
    if let Some(title) = movie.title.as_deref() {
        fa.set_field(StreamKind::General, 0, "Title", title);
    }
    if let Some(date) = movie.encoded_date.as_deref() {
        fa.set_field(StreamKind::General, 0, "Encoded_Date", date);
    }
    fa.set_field(StreamKind::General, 0, "IsStreamable", if is_streamable { "Yes" } else { "No" });
    if crc32_at_level1 {
        fa.set_field(StreamKind::General, 0, "ErrorDetectionType", "Per level 1");
    }
    // Global tag → General stream fields
    for tag in tag_pairs {
        if tag.target_track_uid == 0 {
            if tag.name.eq_ignore_ascii_case("TITLE")
                || tag.name.eq_ignore_ascii_case("HANDLER_NAME")
            {
                fa.set_field(StreamKind::General, 0, "Title", tag.value.as_str());
                break;
            }
        }
    }

    let timecode_scale_ns: f64 = movie.timecode_scale.unwrap_or(1_000_000) as f64;
    let duration_seconds: Option<f64> =
        movie.duration_units.map(|units| units * timecode_scale_ns / 1_000_000_000.0);
    let duration_ms: Option<u64> = duration_seconds.map(|s| (s * 1000.0).round() as u64);

    let mut audio_count: u32 = 0;
    let mut video_count: u32 = 0;
    let mut text_count: u32 = 0;
    let mut stream_order: u32 = 0;
    for track in tracks {
        match track.track_type {
            Some(2) => {
                let pos = fa.stream_prepare(StreamKind::Audio);
                fa.set_field(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string());
                if let Some(n) = track.number {
                    fa.set_field(StreamKind::Audio, pos, "ID", n.to_string());
                }
                if let Some(uid) = track.uid {
                    fa.set_field(StreamKind::Audio, pos, "UniqueID", uid.to_string());
                }
                stream_order += 1;
                if let Some(c) = track.codec_id.as_deref() {
                    if let Some(fmt) = mkv_codec_to_format(c) {
                        fa.set_field(StreamKind::Audio, pos, "Format", fmt);
                    }
                    fa.set_field(StreamKind::Audio, pos, "CodecID", c);
                }
                if let Some(ch) = track.audio_channels {
                    fa.set_field(StreamKind::Audio, pos, "Channels", ch.to_string());
                    let codec_id = track.codec_id.as_deref().unwrap_or("");
                    let (positions, layout) = channel_layout_for_codec(ch as u16, codec_id);
                    if let Some(p) = positions {
                        fa.set_field(StreamKind::Audio, pos, "ChannelPositions", p);
                    }
                    if let Some(l) = layout {
                        fa.set_field(StreamKind::Audio, pos, "ChannelLayout", l);
                    }
                }
                if let Some(sr) = track.audio_sampling_rate {
                    let sr_int = sr.round() as u64;
                    fa.set_field(StreamKind::Audio, pos, "SamplingRate", sr_int.to_string());
                    if let Some(ms) = duration_ms {
                        let sampling_count = (sr * ms as f64 / 1000.0).round() as u64;
                        fa.set_field(
                            StreamKind::Audio,
                            pos,
                            "SamplingCount",
                            sampling_count.to_string(),
                        );
                    }
                }
                // Opus has no PCM bit depth. Muxers still write Audio/BitDepth
                // as the decoder's float width (32). Copying it reports a
                // sample depth the codec does not have.
                if let Some(bd) = track.audio_bit_depth
                    && track.codec_id.as_deref() != Some("A_OPUS")
                {
                    fa.set_field(StreamKind::Audio, pos, "BitDepth", bd.to_string());
                }

                // Parse CodecPrivate for Opus to get accurate sample rate and channel mapping
                if track.codec_id.as_deref() == Some("A_OPUS")
                    && let Some(ref private) = track.codec_private
                    && private.len() >= 19
                    && &private[0..8] == b"OpusHead"
                {
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
                            fa.set_field(
                                StreamKind::Audio,
                                pos,
                                "SamplingRate",
                                sample_rate.to_string(),
                            );
                        }

                        // Set channels from header if not already set
                        if track.audio_channels.is_none() && channels > 0 {
                            fa.set_field(StreamKind::Audio, pos, "Channels", channels.to_string());
                            let (positions, layout) =
                                channel_layout_for_codec(channels as u16, "A_OPUS");
                            if let Some(p) = positions {
                                fa.set_field(StreamKind::Audio, pos, "ChannelPositions", p);
                            }
                            if let Some(l) = layout {
                                fa.set_field(StreamKind::Audio, pos, "ChannelLayout", l);
                            }
                        }
                    }
                }

                // Parse CodecPrivate for Vorbis to get sample rate and channels
                if track.codec_id.as_deref() == Some("A_VORBIS")
                    && let Some(ref private) = track.codec_private
                {
                    // Vorbis identification header starts with packet type (1) + "vorbis"
                    // But in MKV, CodecPrivate for Vorbis contains 3 Vorbis headers packed together
                    // Format: [id_header_length] [id_header] [comment_header_length] [comment_header] [setup_header_length] [setup_header]
                    // Lengths are 4-byte LE
                    if private.len() >= 4 + 7 + 11 {
                        let id_len = (private[0] as usize)
                            | ((private[1] as usize) << 8)
                            | ((private[2] as usize) << 16)
                            | ((private[3] as usize) << 24);

                        if id_len >= 7 && private.len() >= id_len.saturating_add(4) {
                            let id_header = &private[4..4 + id_len];

                            // Check for Vorbis identification header: packet type 1 + "vorbis"
                            if id_header.len() >= 7
                                && id_header[0] == 1
                                && &id_header[1..7] == b"vorbis"
                                && id_header.len() >= 11
                            {
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
                                        fa.set_field(
                                            StreamKind::Audio,
                                            pos,
                                            "SamplingRate",
                                            sample_rate.to_string(),
                                        );
                                    }

                                    if track.audio_channels.is_none() && channels > 0 {
                                        fa.set_field(
                                            StreamKind::Audio,
                                            pos,
                                            "Channels",
                                            channels.to_string(),
                                        );
                                        let (positions, layout) =
                                            channel_layout_for_codec(channels as u16, "A_VORBIS");
                                        if let Some(p) = positions {
                                            fa.set_field(
                                                StreamKind::Audio,
                                                pos,
                                                "ChannelPositions",
                                                p,
                                            );
                                        }
                                        if let Some(l) = layout {
                                            fa.set_field(
                                                StreamKind::Audio,
                                                pos,
                                                "ChannelLayout",
                                                l,
                                            );
                                        }
                                    }

                                    // bit_rate_maximum (4 bytes LE, signed) - offset 16
                                    if id_header.len() >= 20 {
                                        let bitrate_max = (id_header[16] as i32)
                                            | ((id_header[17] as i32) << 8)
                                            | ((id_header[18] as i32) << 16)
                                            | ((id_header[19] as i32) << 24);
                                        if bitrate_max > 0 {
                                            fa.set_field(
                                                StreamKind::Audio,
                                                pos,
                                                "BitRate_Maximum",
                                                bitrate_max.to_string(),
                                            );
                                        }
                                    }

                                    // bit_rate_nominal (4 bytes LE, signed) - offset 20
                                    if id_header.len() >= 24 {
                                        let bitrate_nominal = (id_header[20] as i32)
                                            | ((id_header[21] as i32) << 8)
                                            | ((id_header[22] as i32) << 16)
                                            | ((id_header[23] as i32) << 24);
                                        if bitrate_nominal > 0 {
                                            fa.set_field(
                                                StreamKind::Audio,
                                                pos,
                                                "BitRate",
                                                bitrate_nominal.abs().to_string(),
                                            );
                                            fa.set_field(
                                                StreamKind::Audio,
                                                pos,
                                                "BitRate_Mode",
                                                "CBR",
                                            );
                                        } else {
                                            fa.set_field(
                                                StreamKind::Audio,
                                                pos,
                                                "BitRate_Mode",
                                                "VBR",
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(c) = track.codec_id.as_deref() {
                    if codec_is_lossy(c) {
                        fa.set_field(StreamKind::Audio, pos, "Compression_Mode", "Lossy");
                    } else if codec_is_lossless(c) {
                        fa.set_field(StreamKind::Audio, pos, "Compression_Mode", "Lossless");
                    }
                }
                if let Some(d) = track.flag_default {
                    fa.set_field(StreamKind::Audio, pos, "Default", if d { "Yes" } else { "No" });
                }
                // FlagForced defaults to 0 in MKV; oracle always
                // emits this field for audio tracks, so default
                // missing values to "No".
                let forced = track.flag_forced.unwrap_or(false);
                fa.set_field(StreamKind::Audio, pos, "Forced", if forced { "Yes" } else { "No" });
                // For MKV, the presented timeline is already compensated
                // for any codec pre-roll (Opus pre-skip / CodecDelay), so
                // MediaInfoLib reports Delay = 0.000 with Delay_Source =
                // "Container" for every audio track. It also emits
                // Video_Delay = 0.000 (audio-relative-to-video offset).
                fa.set_field(StreamKind::Audio, pos, "Delay", "0.000");
                fa.set_field(StreamKind::Audio, pos, "Delay_Source", "Container");
                fa.set_field(StreamKind::Audio, pos, "Video_Delay", "0.000");
                // Same unit as General.Duration: whole milliseconds. The
                // exporter turns that into seconds. A pre-formatted seconds
                // string would skip that conversion and disagree with General.
                if let Some(ms) = duration_ms {
                    fa.set_field(StreamKind::Audio, pos, "Duration", ms.to_string());
                }
                emit_language(fa, StreamKind::Audio, pos, track);
                if let Some(name) = track.name.as_deref() {
                    fa.set_field(StreamKind::Audio, pos, "Title", name);
                } else if let Some(tag_title) = track_name_from_tags(tag_pairs, track.uid) {
                    fa.set_field(StreamKind::Audio, pos, "Title", tag_title.as_str());
                }
                let audio_default = track.flag_default.unwrap_or(true);
                fa.set_field(
                    StreamKind::Audio,
                    pos,
                    "Default",
                    if audio_default { "Yes" } else { "No" },
                );
                if let Some(f) = track.flag_forced {
                    fa.set_field(StreamKind::Audio, pos, "Forced", if f { "Yes" } else { "No" });
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
                        fa.set_field(StreamKind::Audio, pos, "Encoded_Library", v.to_string());
                    }
                }
                if fa.retrieve(StreamKind::Audio, pos, "Channels").is_none() {
                    fa.set_field(StreamKind::Audio, pos, "Channels", "1");
                    let (positions, layout) = channel_layout(1);
                    if let Some(p) = positions {
                        fa.set_field(StreamKind::Audio, pos, "ChannelPositions", p);
                    }
                    if let Some(l) = layout {
                        fa.set_field(StreamKind::Audio, pos, "ChannelLayout", l);
                    }
                }
                apply_service_kind(fa, StreamKind::Audio, pos, track);
                apply_statistics(fa, StreamKind::Audio, pos, track.uid, tag_pairs);
                audio_count += 1;
            }
            Some(1) => {
                let pos = fa.stream_prepare(StreamKind::Video);
                fa.set_field(StreamKind::Video, pos, "StreamOrder", stream_order.to_string());
                stream_order += 1;
                if let Some(n) = track.number {
                    fa.set_field(StreamKind::Video, pos, "ID", n.to_string());
                }
                if let Some(uid) = track.uid {
                    fa.set_field(StreamKind::Video, pos, "UniqueID", uid.to_string());
                }
                if let Some(c) = track.codec_id.as_deref() {
                    if let Some(fmt) = mkv_codec_to_format(c) {
                        fa.set_field(StreamKind::Video, pos, "Format", fmt);
                    }
                    fa.set_field(StreamKind::Video, pos, "CodecID", c);
                }
                if let Some(w) = track.video_width {
                    fa.set_field(StreamKind::Video, pos, "Width", w.to_string());
                    let display_w = track.display_width.unwrap_or(w);
                    fa.set_field(StreamKind::Video, pos, "Sampled_Width", display_w.to_string());
                }
                if let Some(h) = track.video_height {
                    fa.set_field(StreamKind::Video, pos, "Height", h.to_string());
                    let display_h = track.display_height.unwrap_or(h);
                    fa.set_field(StreamKind::Video, pos, "Sampled_Height", display_h.to_string());
                }
                if let (Some(w), Some(h)) = (track.video_width, track.video_height) {
                    let dw = track.display_width.unwrap_or(w) as f64;
                    let dh = track.display_height.unwrap_or(h) as f64;
                    if dw > 0.0 && dh > 0.0 {
                        let par = (dw / w as f64) / (dh / h as f64);
                        let dar = dw / dh;
                        fa.set_field(
                            StreamKind::Video,
                            pos,
                            "PixelAspectRatio",
                            format!("{:.3}", par),
                        );
                        fa.set_field(
                            StreamKind::Video,
                            pos,
                            "DisplayAspectRatio",
                            format!("{:.3}", dar),
                        );
                    }
                }
                // Video Duration. With a per-frame DefaultDuration known,
                // MediaInfoLib derives it from the frame timeline
                // (FrameCount × DefaultDuration), which can be shorter than
                // the segment Duration (the longest track). Fall back to
                // the segment Duration otherwise.
                let frame_dur_ns = track.default_duration_ns.filter(|&ns| ns > 0);
                let frame_count = match (duration_seconds, frame_dur_ns) {
                    (Some(s), Some(ns)) => Some((s * 1_000_000_000.0 / ns as f64).round() as u64),
                    _ => None,
                };
                match (frame_count, frame_dur_ns) {
                    (Some(fc), Some(ns)) => {
                        let video_ms = (fc as f64 * ns as f64 / 1_000_000.0).round() as u64;
                        fa.set_field(StreamKind::Video, pos, "Duration", video_ms.to_string());
                    }
                    _ => {
                        if let Some(ms) = duration_ms {
                            fa.set_field(StreamKind::Video, pos, "Duration", ms.to_string());
                        }
                    }
                }
                // FrameRate from DefaultDuration. 1 frame per ns →
                // frame_rate = 1e9 / default_duration_ns. CFR when
                // DefaultDuration is set (MKV doesn't expose per-frame
                // deltas without walking clusters). FrameRate_Num/_Den are
                // the exact rational 1e9 / ns reduced by their GCD.
                if let Some(ns) = frame_dur_ns {
                    let fr = 1_000_000_000.0 / ns as f64;
                    fa.set_field(StreamKind::Video, pos, "FrameRate_Mode", "CFR");
                    fa.set_field(StreamKind::Video, pos, "FrameRate", format!("{fr:.3}"));
                    let g = gcd_u64(1_000_000_000, ns);
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "FrameRate_Num",
                        (1_000_000_000 / g).to_string(),
                    );
                    fa.set_field(StreamKind::Video, pos, "FrameRate_Den", (ns / g).to_string());
                    if let Some(fc) = frame_count {
                        fa.set_field(StreamKind::Video, pos, "FrameCount", fc.to_string());
                    }
                }
                // MKV presents a compensated timeline, so the video track
                // has zero delay (Delay_Source = Container).
                fa.set_field(StreamKind::Video, pos, "Delay", "0.000");
                fa.set_field(StreamKind::Video, pos, "Delay_Source", "Container");
                if let Some(bd) = track.video_bit_depth {
                    fa.set_field(StreamKind::Video, pos, "BitDepth", bd.to_string());
                }
                // ColorSpace=YUV is the universal default for all formats
                // we recognize in MKV (AVC/HEVC/VP9/AV1).

                // For AV1, try to parse codec config for more accurate info.
                if track.codec_id.as_deref() == Some("V_AV1")
                    && let Some(ref private) = track.codec_private
                    && let Some(info) = revelo_parsers_video::parse_av1_from_codec_config(private)
                {
                    // MKV's container PixelWidth/PixelHeight and
                    // BitsPerChannel are authoritative — oracle
                    // reports those, not the AV1 sequence header.
                    // Only fill from the codec config when the
                    // container didn't supply them (avoids the
                    // AV1-OBU parse clobbering 1080x1920 with the
                    // partially-decoded sequence-header dims).
                    if info.width > 0 && track.video_width.is_none() {
                        fa.set_field(StreamKind::Video, pos, "Width", info.width.to_string());
                    }
                    if info.height > 0 && track.video_height.is_none() {
                        fa.set_field(StreamKind::Video, pos, "Height", info.height.to_string());
                    }
                    if track.video_bit_depth.is_none() {
                        fa.set_field(
                            StreamKind::Video,
                            pos,
                            "BitDepth",
                            info.bit_depth.to_string(),
                        );
                    }
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "ChromaSubsampling",
                        info.chroma_subsampling,
                    );
                    let profile_name = match info.profile {
                        0 => "Main",
                        1 => "High",
                        2 => "Professional",
                        _ => "Unknown",
                    };
                    fa.set_field(StreamKind::Video, pos, "Format_Profile", profile_name);
                    if let Some(level) = revelo_parsers_video::av1_level_name(info.level) {
                        fa.set_field(StreamKind::Video, pos, "Format_Level", level);
                    }
                    fa.set_field(StreamKind::Video, pos, "Format_Info", "AOMedia Video 1");
                }

                // For VP9, try to parse CodecPrivate for profile, level, bit depth.
                if track.codec_id.as_deref() == Some("V_VP9")
                    && let Some(ref private) = track.codec_private
                    && private.len() >= 4
                {
                    let profile = private[1];
                    let level = private[2];
                    let config_byte = private[3];
                    let bit_depth = (config_byte >> 4) & 0x0F;
                    let chroma_subsampling = (config_byte >> 1) & 0x07;
                    let video_full_range = config_byte & 1;

                    fa.set_field(StreamKind::Video, pos, "Format_Profile", profile.to_string());
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "Format_Level",
                        format!("{:.1}", level as f64 / 10.0),
                    );
                    if track.video_bit_depth.is_none() {
                        fa.set_field(StreamKind::Video, pos, "BitDepth", bit_depth.to_string());
                    }
                    let chroma = match chroma_subsampling {
                        1 => "4:2:0",
                        2 => "4:2:2",
                        3 => "4:4:0",
                        _ => "4:2:0",
                    };
                    fa.set_field(StreamKind::Video, pos, "ChromaSubsampling", chroma);
                    let range = if video_full_range != 0 { "Full" } else { "Limited" };
                    fa.set_field(StreamKind::Video, pos, "colour_range", range);
                }

                // VP9-in-Matroska usually carries no CodecPrivate, so
                // profile and bit depth come from the first keyframe's
                // uncompressed header — reuse the VP9 frame-header decoder
                // rather than duplicating its bit-parsing here. Only fill
                // fields the container didn't already provide.
                if matches!(track.codec_id.as_deref(), Some("V_VP9"))
                    && fa.retrieve(StreamKind::Video, pos, "Format_Profile").is_none()
                    && let Some(frame) = vpx_keyframe
                {
                    let mut tmp = FileAnalyze::new(frame);
                    if revelo_parsers_video::parse_vp9(&mut tmp) {
                        let profile = tmp
                            .retrieve(StreamKind::Video, 0, "Format_Profile")
                            .map(|z| z.as_str().to_owned());
                        let bit_depth = tmp
                            .retrieve(StreamKind::Video, 0, "BitDepth")
                            .map(|z| z.as_str().to_owned());
                        if let Some(profile) = profile {
                            fa.set_field(StreamKind::Video, pos, "Format_Profile", profile);
                        }
                        if track.video_bit_depth.is_none()
                            && let Some(bd) = bit_depth
                        {
                            fa.set_field(StreamKind::Video, pos, "BitDepth", bd);
                        }
                        // The colour info (matrix_coefficients) was read
                        // from the bitstream. When the container's Colour
                        // element didn't also carry a matrix, MediaInfoLib
                        // labels the source "Stream". A CS_UNKNOWN
                        // colorspace maps to "unspecified", whose value is
                        // suppressed, but the source is still reported.
                        if track.colour_matrix.is_none() {
                            fa.set_field(
                                StreamKind::Video,
                                pos,
                                "matrix_coefficients_Source",
                                "Stream",
                            );
                        }
                    }
                }

                // For AVC tracks with CodecPrivate, parse avcC to get profile/level
                if track.codec_id.as_deref() == Some("V_DOLBYVISION/AVC") {
                    fa.set_field(StreamKind::Video, pos, "HDR_Format", "Dolby Vision");
                    fa.set_field(StreamKind::Video, pos, "HDR_Format_Compatibility", "AVC");
                }

                if track.codec_id.as_deref() == Some("V_MPEG4/ISO/AVC")
                    && let Some(ref private) = track.codec_private
                    && private.len() >= 4
                    && private[0] == 1
                {
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
                        fa.set_field(StreamKind::Video, pos, "Format_Profile", profile_name);
                    }
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "Format_Level",
                        format!("{}.{:02}", level_idc / 10, level_idc % 10),
                    );

                    // Parse the first SPS for dimensions and colour info.
                    fill_avc_first_sps(fa, pos, private);
                }

                // For HEVC tracks with CodecPrivate, parse hvcC to get profile/tier/level
                // Dolby Vision tracks with HEVC base layer
                if track.codec_id.as_deref() == Some("V_DOLBYVISION/HEVC")
                    || track.codec_id.as_deref() == Some("V_DOLBYVISION")
                {
                    fa.set_field(StreamKind::Video, pos, "HDR_Format", "Dolby Vision");
                    fa.set_field(StreamKind::Video, pos, "HDR_Format_Compatibility", "HDR10");
                    // If we have CodecPrivate with hvcC, also parse HEVC details
                    if let Some(ref private) = track.codec_private
                        && let Some(info) = revelo_parsers_video::parse_hevc_sps(private)
                    {
                        fa.set_field(StreamKind::Video, pos, "Width", info.width.to_string());
                        fa.set_field(StreamKind::Video, pos, "Height", info.height.to_string());
                        fa.set_field(
                            StreamKind::Video,
                            pos,
                            "BitDepth",
                            info.bit_depth.to_string(),
                        );
                    }
                }

                if track.codec_id.as_deref() == Some("V_MPEGH/ISO/HEVC")
                    && let Some(ref private) = track.codec_private
                    && private.len() >= 23
                    && private[0] == 1
                {
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
                        fa.set_field(StreamKind::Video, pos, "Format_Profile", profile_name);
                    }

                    let tier_name = if tier_flag == 0 { "Main" } else { "High" };
                    fa.set_field(StreamKind::Video, pos, "Format_Tier", tier_name);

                    let level_str = if level_idc % 10 == 0 {
                        format!("{}.0", level_idc / 30)
                    } else {
                        format!("{}.{}", level_idc / 30, (level_idc % 30) / 3)
                    };
                    fa.set_field(StreamKind::Video, pos, "Format_Level", level_str);

                    // Parse NAL arrays for SPS and SEI
                    if private.len() > 23 {
                        let num_arrays = private[22] as usize;
                        let mut offset = 23usize;
                        let mut sps_data: Option<Vec<u8>> = None;
                        let mut sei_nalus: Vec<Vec<u8>> = Vec::new();

                        for _ in 0..num_arrays {
                            if offset >= private.len() {
                                break;
                            }
                            let array_header = private[offset];
                            let nal_type = array_header & 0x3F;
                            offset += 1;

                            if offset + 2 > private.len() {
                                break;
                            }
                            let num_nalus =
                                ((private[offset] as usize) << 8) | (private[offset + 1] as usize);
                            offset += 2;

                            for _ in 0..num_nalus {
                                if offset + 2 > private.len() {
                                    break;
                                }
                                let nal_len = ((private[offset] as usize) << 8)
                                    | (private[offset + 1] as usize);
                                offset += 2;

                                if offset + nal_len > private.len() {
                                    break;
                                }
                                let nal_data = private[offset..offset + nal_len].to_vec();

                                match nal_type {
                                    33 if sps_data.is_none() => {
                                        sps_data = Some(nal_data);
                                    }
                                    39 | 40 => sei_nalus.push(nal_data),
                                    _ => {}
                                }
                                offset += nal_len;
                            }
                        }

                        // Parse SPS for dimensions and colour
                        if let Some(sps) = sps_data
                            && let Some(sps_info) = revelo_parsers_video::parse_hevc_sps(&sps)
                        {
                            if sps_info.width > 0 {
                                fa.set_field(
                                    StreamKind::Video,
                                    pos,
                                    "Width",
                                    sps_info.width.to_string(),
                                );
                            }
                            if sps_info.height > 0 {
                                fa.set_field(
                                    StreamKind::Video,
                                    pos,
                                    "Height",
                                    sps_info.height.to_string(),
                                );
                            }
                            if sps_info.colour_description_present {
                                if let Some(cp) =
                                    sps_info.colour_primaries.and_then(|v| cicp_primaries(v as u16))
                                {
                                    fa.set_field(StreamKind::Video, pos, "colour_primaries", cp);
                                }
                                if let Some(tc) = sps_info
                                    .transfer_characteristics
                                    .and_then(|v| cicp_transfer(v as u16))
                                {
                                    fa.set_field(
                                        StreamKind::Video,
                                        pos,
                                        "transfer_characteristics",
                                        tc,
                                    );
                                }
                                if let Some(mc) =
                                    sps_info.matrix_coefficients.and_then(|v| cicp_matrix(v as u16))
                                {
                                    fa.set_field(StreamKind::Video, pos, "matrix_coefficients", mc);
                                }
                            }
                            if let Some(vfr) = sps_info.video_full_range {
                                fa.set_field(
                                    StreamKind::Video,
                                    pos,
                                    "colour_range",
                                    if vfr { "Full" } else { "Limited" },
                                );
                            }
                        }

                        // Extract encoder from SEI
                        if !sei_nalus.is_empty() {
                            let refs: Vec<&[u8]> = sei_nalus.iter().map(|v| v.as_slice()).collect();
                            if let Some(encoder) =
                                revelo_parsers_video::extract_encoder_from_sei_nalus(&refs)
                            {
                                fa.set_field(
                                    StreamKind::Video,
                                    pos,
                                    "Encoded_Library",
                                    encoder.library.as_str(),
                                );
                            }
                        }
                    }
                }

                // MediaInfoLib reports its colour info for VP9 from the
                // bitstream (matrix_coefficients_Source = Stream) and emits
                // no ColorSpace for it; other codecs get the YUV default.
                let is_vpx = matches!(track.codec_id.as_deref(), Some("V_VP9") | Some("V_VP8"));
                if !is_vpx {
                    fa.set_field(StreamKind::Video, pos, "ColorSpace", "YUV");
                }
                // ChromaSubsampling default: 4:2:0 (no Colour element in
                // MKV explicitly carries this; the codec implies it).
                fa.set_field(StreamKind::Video, pos, "ChromaSubsampling", "4:2:0");
                // Colour element → colour_* fields. The presence flag's
                // source is the container's Colour element ("Container").
                // For codecs whose bitstream also carries a VUI colour
                // description that rust parses (AVC/HEVC), MediaInfoLib
                // merges the sources into "Container / Stream"; VP9's
                // bitstream carries no such presence flag, so it stays
                // "Container".
                let has_colour = track.colour_primaries.is_some()
                    || track.colour_transfer.is_some()
                    || track.colour_matrix.is_some()
                    || track.colour_range.is_some();
                if has_colour {
                    fa.set_field(StreamKind::Video, pos, "colour_description_present", "Yes");
                    let desc_source = if is_vpx { "Container" } else { "Container / Stream" };
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "colour_description_present_Source",
                        desc_source,
                    );
                }
                if let Some(r) = track.colour_range {
                    let s = match r {
                        1 => "Limited",
                        2 => "Full",
                        _ => "",
                    };
                    if !s.is_empty() {
                        fa.set_field(StreamKind::Video, pos, "colour_range", s);
                        fa.set_field(
                            StreamKind::Video,
                            pos,
                            "colour_range_Source",
                            "Container / Stream",
                        );
                    }
                }
                if let Some(p) = track.colour_primaries.and_then(|v| cicp_primaries(v as u16)) {
                    fa.set_field(StreamKind::Video, pos, "colour_primaries", p);
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "colour_primaries_Source",
                        "Container / Stream",
                    );
                }
                if let Some(t) = track.colour_transfer.and_then(|v| cicp_transfer(v as u16)) {
                    fa.set_field(StreamKind::Video, pos, "transfer_characteristics", t);
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "transfer_characteristics_Source",
                        "Container / Stream",
                    );
                    // Set HDR_Format for HLG and PQ when transfer characteristics indicate HDR
                    match t {
                        "PQ" if fa.retrieve(StreamKind::Video, pos, "HDR_Format").is_none() => {
                            fa.set_field(StreamKind::Video, pos, "HDR_Format", "SMPTE ST 2084");
                            fa.set_field(StreamKind::Video, pos, "HDR_Format_Compatibility", "PQ");
                        }
                        "HLG" if fa.retrieve(StreamKind::Video, pos, "HDR_Format").is_none() => {
                            fa.set_field(StreamKind::Video, pos, "HDR_Format", "ARIB STD-B67");
                            fa.set_field(StreamKind::Video, pos, "HDR_Format_Compatibility", "HLG");
                        }
                        _ => {}
                    }
                }
                if let Some(m) = track.colour_matrix.and_then(|v| cicp_matrix(v as u16)) {
                    fa.set_field(StreamKind::Video, pos, "matrix_coefficients", m);
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "matrix_coefficients_Source",
                        "Container / Stream",
                    );
                }
                if let Some(name) = track.name.as_deref() {
                    fa.set_field(StreamKind::Video, pos, "Title", name);
                } else if let Some(tag_title) = track_name_from_tags(tag_pairs, track.uid) {
                    fa.set_field(StreamKind::Video, pos, "Title", tag_title.as_str());
                }
                emit_language(fa, StreamKind::Video, pos, track);
                let video_default = track.flag_default.unwrap_or(true);
                fa.set_field(
                    StreamKind::Video,
                    pos,
                    "Default",
                    if video_default { "Yes" } else { "No" },
                );
                let forced = track.flag_forced.unwrap_or(false);
                fa.set_field(StreamKind::Video, pos, "Forced", if forced { "Yes" } else { "No" });
                // Encoded_Library from a matching ENCODER tag (e.g.
                // "Lavc62.28.102 libvpx-vp9"), preferring a track-targeted
                // tag over a global (muxer-level) one.
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
                        fa.set_field(StreamKind::Video, pos, "Encoded_Library", v.to_string());
                    }
                }
                apply_service_kind(fa, StreamKind::Video, pos, track);
                apply_statistics(fa, StreamKind::Video, pos, track.uid, tag_pairs);
                video_count += 1;
            }
            Some(0x11) => {
                let pos = fa.stream_prepare(StreamKind::Text);
                fa.set_field(StreamKind::Text, pos, "StreamOrder", stream_order.to_string());
                stream_order += 1;
                if let Some(n) = track.number {
                    fa.set_field(StreamKind::Text, pos, "ID", n.to_string());
                }
                if let Some(uid) = track.uid {
                    fa.set_field(StreamKind::Text, pos, "UniqueID", uid.to_string());
                }
                if let Some(c) = track.codec_id.as_deref() {
                    if let Some(fmt) = mkv_text_format(c) {
                        fa.set_field(StreamKind::Text, pos, "Format", fmt);
                    }
                    fa.set_field(StreamKind::Text, pos, "CodecID", c);
                }
                if let Some(ms) = duration_ms {
                    fa.set_field(StreamKind::Text, pos, "Duration", ms.to_string());
                }
                emit_language(fa, StreamKind::Text, pos, track);
                if let Some(name) = track.name.as_deref() {
                    fa.set_field(StreamKind::Text, pos, "Title", name);
                } else if let Some(tag_title) = track_name_from_tags(tag_pairs, track.uid) {
                    fa.set_field(StreamKind::Text, pos, "Title", tag_title.as_str());
                }
                let text_default = track.flag_default.unwrap_or(true);
                fa.set_field(
                    StreamKind::Text,
                    pos,
                    "Default",
                    if text_default { "Yes" } else { "No" },
                );
                let forced = track.flag_forced.unwrap_or(false);
                fa.set_field(StreamKind::Text, pos, "Forced", if forced { "Yes" } else { "No" });
                apply_service_kind(fa, StreamKind::Text, pos, track);
                apply_statistics(fa, StreamKind::Text, pos, track.uid, tag_pairs);
                text_count += 1;
            }
            _ => {}
        }
    }

    if audio_count > 0 {
        fa.set_field(StreamKind::General, 0, "AudioCount", audio_count.to_string());
    }
    if video_count > 0 {
        fa.set_field(StreamKind::General, 0, "VideoCount", video_count.to_string());
    }
    if text_count > 0 {
        fa.set_field(StreamKind::General, 0, "TextCount", text_count.to_string());
    }
    let menu_count = fill_menus(fa, movie);
    if menu_count > 0 {
        fa.set_field(StreamKind::General, 0, "MenuCount", menu_count.to_string());
    }
    let image_count = fill_images(fa, movie);
    if image_count > 0 {
        fa.set_field(StreamKind::General, 0, "ImageCount", image_count.to_string());
    }
    if let Some(cover) = movie.attachments.iter().find(|att| attachment_is_cover(att)) {
        fa.set_field(StreamKind::General, 0, "Cover", "Yes");
        if let Some(mime) = cover.mime.as_deref() {
            let cover_type = if mime.contains("png") {
                "PNG"
            } else if mime.contains("jpeg") || mime.contains("jpg") {
                "JPG"
            } else {
                "Unknown"
            };
            fa.set_field(StreamKind::General, 0, "Cover_Type", cover_type);
            fa.set_field(StreamKind::General, 0, "Cover_Mime", mime);
        }
    }
    if let Some(ms) = duration_ms {
        fa.set_field(StreamKind::General, 0, "Duration", ms.to_string());

        // Calculate OverallBitRate = FileSize * 8 / Duration_ms * 1000
        if ms > 0 && file_size > 0 {
            // Round (not truncate) to match MediaInfoLib: it computes
            // FileSize * 8 / Duration_s and rounds to the nearest bit/s.
            let overall_bitrate = (file_size as f64 * 8000.0 / ms as f64).round() as u64;
            fa.set_field(StreamKind::General, 0, "OverallBitRate", overall_bitrate.to_string());
        }
    }
}

fn emit_language(fa: &mut FileAnalyze, kind: StreamKind, pos: usize, track: &TrackInfo) {
    if let Some(bcp) = track.language_bcp47.as_deref() {
        let bcp = bcp.trim();
        if !bcp.is_empty() && !bcp.eq_ignore_ascii_case("und") {
            fa.set_field(kind, pos, "Language", bcp);
            return;
        }
    }
    if let Some(lang) = track.language.as_deref().and_then(iso639_emit) {
        fa.set_field(kind, pos, "Language", lang);
    }
}

fn apply_service_kind(fa: &mut FileAnalyze, kind: StreamKind, pos: usize, track: &TrackInfo) {
    let flags = [
        (track.flag_hearing_impaired, "HI", "Hearing Impaired"),
        (track.flag_visual_impaired, "VI", "Visually Impaired"),
        (track.flag_text_descriptions, "TD", "Text Descriptions"),
        (track.flag_original, "O", "Original"),
        (track.flag_commentary, "C", "Commentary"),
    ];
    let mut codes = Vec::new();
    let mut names = Vec::new();
    for (on, code, name) in flags {
        if on {
            codes.push(code);
            names.push(name);
        }
    }
    if codes.is_empty() {
        return;
    }
    fa.set_field(kind, pos, "ServiceKind", codes.join(" / "));
    fa.set_field(kind, pos, "ServiceKind_String", names.join(" / "));
}

fn apply_statistics(
    fa: &mut FileAnalyze,
    kind: StreamKind,
    pos: usize,
    uid: Option<u64>,
    tags: &[TagEntry],
) {
    let Some(uid) = uid else { return };
    for tag in tags {
        if tag.target_track_uid != uid {
            continue;
        }
        let value = tag.value.trim();
        if value.is_empty() {
            continue;
        }
        if tag.name.eq_ignore_ascii_case("BPS") {
            fa.force_field(kind, pos, "BitRate", value);
        } else if tag.name.eq_ignore_ascii_case("NUMBER_OF_BYTES") {
            fa.force_field(kind, pos, "StreamSize", value);
        } else if tag.name.eq_ignore_ascii_case("NUMBER_OF_FRAMES") {
            fa.force_field(kind, pos, "FrameCount", value);
        } else if tag.name.eq_ignore_ascii_case("DURATION")
            && let Some(ms) = statistics_duration_ms(value)
        {
            fa.force_field(kind, pos, "Duration", ms.to_string());
        }
    }
}

/// mkvmerge statistics duration `HH:MM:SS.nnnnnnnnn` → whole milliseconds.
fn statistics_duration_ms(value: &str) -> Option<u64> {
    let mut parts = value.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !hours.is_finite() || !minutes.is_finite() || !seconds.is_finite()
    {
        return None;
    }
    let ms = hours * 3_600_000.0 + minutes * 60_000.0 + seconds * 1000.0;
    if ms < 0.0 || ms > u64::MAX as f64 {
        return None;
    }
    Some(ms.round() as u64)
}

fn fill_menus(fa: &mut FileAnalyze, movie: &MovieInfo) -> u32 {
    let mut count = 0u32;
    for edition in &movie.editions {
        if edition.is_empty() {
            continue;
        }
        let pos = fa.stream_prepare(StreamKind::Menu);
        for chapter in edition {
            let stamp = chapter_timestamp(chapter.start_ns);
            let value = if chapter.text.is_empty() { stamp.clone() } else { chapter.text.clone() };
            fa.set_field(StreamKind::Menu, pos, stamp.as_str(), value);
        }
        count += 1;
    }
    count
}

fn chapter_timestamp(ns: u64) -> String {
    let ms = ns / 1_000_000;
    let hours = ms / 3_600_000;
    let minutes = (ms / 60_000) % 60;
    let seconds = (ms / 1_000) % 60;
    let millis = ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn fill_images(fa: &mut FileAnalyze, movie: &MovieInfo) -> u32 {
    let mut count = 0u32;
    for att in &movie.attachments {
        let Some(fmt) = att.mime.as_deref().and_then(image_format_from_mime) else {
            continue;
        };
        let pos = fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, pos, "Format", fmt);
        if let Some(name) = att.name.as_deref().filter(|n| !n.is_empty()) {
            fa.set_field(StreamKind::Image, pos, "Title", name);
        } else if let Some(desc) = att.description.as_deref().filter(|n| !n.is_empty()) {
            fa.set_field(StreamKind::Image, pos, "Title", desc);
        }
        count += 1;
    }
    count
}

fn image_format_from_mime(mime: &str) -> Option<&'static str> {
    let mime = mime.to_ascii_lowercase();
    if !mime.starts_with("image/") {
        return None;
    }
    if mime.contains("jpeg") || mime.contains("jpg") {
        Some("JPEG")
    } else if mime.contains("png") {
        Some("PNG")
    } else if mime.contains("gif") {
        Some("GIF")
    } else if mime.contains("bmp") {
        Some("BMP")
    } else if mime.contains("webp") {
        Some("WebP")
    } else {
        Some("Image")
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

/// Parse the first AVC SPS out of an avcC `codec_private` blob and fill the
/// video dimensions and colour description. Only the first SPS is inspected.
fn fill_avc_first_sps(fa: &mut FileAnalyze, pos: usize, private: &[u8]) {
    if private.len() < 8 {
        return;
    }
    let num_sps = (private[5] & 0x1F) as usize;
    if num_sps == 0 {
        return;
    }
    let sps_offset = 6;
    if sps_offset + 2 > private.len() {
        return;
    }
    let sps_len = ((private[sps_offset] as usize) << 8) | (private[sps_offset + 1] as usize);
    let sps_offset = sps_offset + 2;
    if sps_offset + sps_len > private.len() {
        return;
    }
    let Some(sps_info) =
        revelo_parsers_video::parse_avc_sps(&private[sps_offset..sps_offset + sps_len])
    else {
        return;
    };
    if sps_info.width > 0 {
        fa.force_field(StreamKind::Video, pos, "Width", sps_info.width.to_string());
    }
    if sps_info.height > 0 {
        fa.force_field(StreamKind::Video, pos, "Height", sps_info.height.to_string());
    }
    if !sps_info.colour_description_present {
        return;
    }
    if let Some(cp) = sps_info.colour_primaries.and_then(|v| cicp_primaries(v as u16)) {
        fa.set_field(StreamKind::Video, pos, "colour_primaries", cp);
    }
    if let Some(tc) = sps_info.transfer_characteristics.and_then(|v| cicp_transfer(v as u16)) {
        fa.set_field(StreamKind::Video, pos, "transfer_characteristics", tc);
    }
    if let Some(mc) = sps_info.matrix_coefficients.and_then(|v| cicp_matrix(v as u16)) {
        fa.set_field(StreamKind::Video, pos, "matrix_coefficients", mc);
    }
    if let Some(vfr) = sps_info.video_full_range {
        fa.set_field(StreamKind::Video, pos, "colour_range", if vfr { "Full" } else { "Limited" });
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
        // Matroska mono is "M". Positions stay "Front: C".
        1 => (Some("Front: C"), Some("M")),
        2 => (Some("Front: L R"), Some("L R")),
        6 => (Some("Front: L C R, Side: L R, LFE"), Some("L R C LFE Ls Rs")),
        _ => (None, None),
    }
}

fn channel_layout_for_codec(
    channels: u16,
    _codec_id: &str,
) -> (Option<&'static str>, Option<&'static str>) {
    channel_layout(channels)
}

/// Look up a track's name from Tags by its track UID.
/// Checks for TITLE and HANDLER_NAME tags targeted at the track.
fn track_name_from_tags(tag_pairs: &[TagEntry], track_uid: Option<u64>) -> Option<String> {
    let uid = track_uid?;
    for tag in tag_pairs {
        if tag.target_track_uid == uid
            && (tag.name.eq_ignore_ascii_case("TITLE")
                || tag.name.eq_ignore_ascii_case("HANDLER_NAME"))
        {
            return Some(tag.value.clone());
        }
    }
    None
}

fn mkv_codec_to_format(codec_id: &str) -> Option<&'static str> {
    match codec_id {
        "A_OPUS" => Some("Opus"),
        "A_VORBIS" => Some("Vorbis"),
        "A_AAC" | "A_AAC/MPEG2/MAIN" | "A_AAC/MPEG2/LC" | "A_AAC/MPEG4/LC"
        | "A_AAC/MPEG4/LC/SBR" => Some("AAC"),
        "A_AC3" => Some("AC-3"),
        "A_EAC3" => Some("E-AC-3"),
        "A_DTS" => Some("DTS"),
        "A_FLAC" => Some("FLAC"),
        "A_TRUEHD" => Some("TrueHD"),
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

fn mkv_text_format(codec_id: &str) -> Option<&'static str> {
    match codec_id {
        "S_TEXT/UTF8" => Some("UTF-8"),
        "S_TEXT/ASCII" => Some("ASCII"),
        "S_TEXT/ASS" | "S_ASS" => Some("ASS"),
        "S_TEXT/SSA" | "S_SSA" => Some("SSA"),
        "S_TEXT/WEBVTT" => Some("WebVTT"),
        "S_TEXT/USF" => Some("USF"),
        "S_HDMV/PGS" => Some("PGS"),
        "S_HDMV/TEXTST" => Some("TextST"),
        "S_DVBSUB" => Some("DVB Subtitle"),
        "S_VOBSUB" => Some("VobSub"),
        "S_KATE" => Some("Kate"),
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
    matches!(codec_id, "A_FLAC" | "A_PCM/INT/LIT" | "A_PCM/INT/BIG" | "A_PCM/FLOAT/IEEE")
}

fn strip_nuls(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).into_owned();
    s.trim_end_matches('\0').to_string()
}

fn read_bytes_limited(
    fa: &mut FileAnalyze,
    size: usize,
    limit: usize,
    skip_label: &str,
) -> Option<Vec<u8>> {
    if size > limit {
        fa.skip_hexa(size, skip_label);
        return None;
    }
    Some(fa.read_raw(size).to_vec())
}

fn read_string_limited(fa: &mut FileAnalyze, size: usize, skip_label: &str) -> Option<String> {
    let bytes = read_bytes_limited(fa, size, MKV_TEXT_VALUE_LIMIT, skip_label)?;
    Some(strip_nuls(&bytes))
}

/// DateUTC is a signed big-endian nanosecond offset from 2001-01-01 UTC.
fn read_date_utc(fa: &mut FileAnalyze, size: usize) -> Option<String> {
    if size == 0 || size > 8 {
        fa.skip_hexa(size, "date_utc");
        return None;
    }
    let raw = read_uint(fa, size);
    let shift = (8 - size) * 8;
    let ns = ((raw << shift) as i64) >> shift;
    let secs = ns.div_euclid(1_000_000_000);
    let unix = secs.checked_add(978_307_200)?;
    Some(format_unix_utc(unix))
}

fn format_unix_utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let sod = unix.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_unix_days(days);
    let hours = sod / 3600;
    let minutes = (sod % 3600) / 60;
    let seconds = sod % 60;
    format!("{year:04}-{month:02}-{day:02} {hours:02}:{minutes:02}:{seconds:02} UTC")
}

/// Howard Hinnant's `civil_from_days`. `days` is days since 1970-01-01.
fn civil_from_unix_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

fn read_uint(fa: &mut FileAnalyze, size: usize) -> u64 {
    if size == 0 || size > 8 {
        fa.skip_hexa(size, "uint");
        return 0;
    }
    let bytes = fa.read_raw(size);
    let mut v: u64 = 0;
    for b in bytes {
        v = (v << 8) | (*b as u64);
    }
    v
}

fn read_float(fa: &mut FileAnalyze, size: usize) -> f64 {
    match size {
        4 => Reader::wrap(fa).be_f32("f32").unwrap_or(0.0) as f64,
        8 => Reader::wrap(fa).be_f64("f64").unwrap_or(0.0),
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

/// Euclidean GCD, used to reduce FrameRate_Num/FrameRate_Den.
fn gcd_u64(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

/// Extract the first VP9/VP8 keyframe's frame bytes from a Cluster child
/// (a `SimpleBlock`, or a `Block` inside a `BlockGroup`). Only the leading
/// bytes — enough for the VP9 uncompressed header — are captured, and only
/// for the requested track. Once `out` is populated the remaining blocks
/// are skipped cheaply.
fn capture_vpx_keyframe(
    fa: &mut FileAnalyze,
    id: u64,
    size: usize,
    track_num: u64,
    out: &mut Option<Vec<u8>>,
) {
    if out.is_some() {
        fa.skip_hexa(size, "block");
        return;
    }
    // Enough to cover the block header (track VINT + 2-byte timecode +
    // flags) plus the VP9 uncompressed header.
    const PROBE: usize = 64;
    match id {
        SIMPLE_BLOCK => {
            if let Some(prefix) = fa.peek_raw(size.min(PROBE))
                && let Some(frame) = vpx_frame_from_block(prefix, track_num, true)
            {
                *out = Some(frame.to_vec());
            }
            fa.skip_hexa(size, "simpleblock");
        }
        BLOCK_GROUP => {
            walk_elements(fa, size, &mut |fa, bid, bsz, _| {
                if bid == BLOCK
                    && out.is_none()
                    && let Some(prefix) = fa.peek_raw(bsz.min(PROBE))
                    && let Some(frame) = vpx_frame_from_block(prefix, track_num, false)
                {
                    *out = Some(frame.to_vec());
                }
                fa.skip_hexa(bsz, "blockgroup_child");
            });
        }
        _ => fa.skip_hexa(size, "cluster_child"),
    }
}

/// Parse a Matroska (Simple)Block header and return the elementary-stream
/// frame bytes when it belongs to `track_num`, is unlaced, and — for a
/// SimpleBlock — is flagged as a keyframe. `payload` may be a truncated
/// prefix; the returned slice is likewise a prefix of the frame.
fn vpx_frame_from_block(payload: &[u8], track_num: u64, is_simpleblock: bool) -> Option<&[u8]> {
    let first = *payload.first()?;
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 || payload.len() < len + 3 {
        return None;
    }
    let marker_mask: u8 = if len == 8 { 0 } else { !(0xFF << (8 - len)) };
    let mut tn = (first & marker_mask) as u64;
    for &b in &payload[1..len] {
        tn = (tn << 8) | b as u64;
    }
    if tn != track_num {
        return None;
    }
    // payload[len..len+2] = int16 relative timecode, payload[len+2] = flags.
    let flags = payload[len + 2];
    if (flags >> 1) & 0x03 != 0 {
        return None; // laced block — skip
    }
    if is_simpleblock && flags & 0x80 == 0 {
        return None; // not a keyframe
    }
    let frame = &payload[len + 3..];
    if frame.len() < 6 {
        return None;
    }
    Some(frame)
}

/// Walk EBML elements within a region. Visitor receives
/// (fa, element_id, body_size, element_start_offset).
fn walk_elements(
    fa: &mut FileAnalyze,
    region_size: usize,
    visit: &mut dyn FnMut(&mut FileAnalyze, u64, usize, usize),
) {
    let Some(region_end) = fa.element_offset().checked_add(region_size) else {
        return;
    };
    while fa.element_offset() < region_end && fa.remain() > 0 {
        let elem_start = fa.element_offset();
        let Some(id) = read_vint_id(fa) else { break };
        let Some(size) = read_vint_size(fa) else { break };
        let Ok(body_size) = usize::try_from(size) else { break };
        let Some(body_end) = fa.element_offset().checked_add(body_size) else {
            break;
        };
        if body_end > region_end {
            // Truncated — bail.
            break;
        }
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

    const MKV_METADATA_ONLY_BUDGET: u64 = 8 * 1024 * 1024;
    const TEST_LARGE_ELEMENT_SIZE: usize = 9 * 1024 * 1024;
    const TEST_TAG_VALUE_SIZE: usize = MKV_TEXT_VALUE_LIMIT + 1024;
    const CUES_ID: &[u8; 4] = &[0x1C, 0x53, 0xBB, 0x6B];

    fn ebml_size(size: usize) -> Vec<u8> {
        if size <= 0x7f {
            vec![0x80 | size as u8]
        } else if size <= 0x3fff {
            vec![0x40 | ((size >> 8) as u8), (size & 0xff) as u8]
        } else if size <= 0x1f_ffff {
            vec![0x20 | ((size >> 16) as u8), ((size >> 8) & 0xff) as u8, (size & 0xff) as u8]
        } else {
            vec![
                0x10 | ((size >> 24) as u8),
                ((size >> 16) & 0xff) as u8,
                ((size >> 8) & 0xff) as u8,
                (size & 0xff) as u8,
            ]
        }
    }

    fn element(id: &[u8], payload: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&ebml_size(payload.len()));
        out.extend_from_slice(&payload);
        out
    }

    fn ebml_header(doc_type: &[u8]) -> Vec<u8> {
        element(&[0x1A, 0x45, 0xDF, 0xA3], element(&[0x42, 0x82], doc_type.to_vec()))
    }

    fn minimal_track() -> Vec<u8> {
        let mut track = Vec::new();
        track.extend(element(&[0xD7], vec![1]));
        track.extend(element(&[0x83], vec![2]));
        track.extend(element(&[0x86], b"A_OPUS".to_vec()));
        element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track))
    }

    fn large_tags() -> Vec<u8> {
        let mut simple_tag = Vec::new();
        simple_tag.extend(element(&[0x45, 0xA3], b"TITLE".to_vec()));
        simple_tag.extend(element(&[0x44, 0x87], vec![b'x'; TEST_TAG_VALUE_SIZE]));
        element(
            &[0x12, 0x54, 0xC3, 0x67],
            element(&[0x73, 0x73], element(&[0x67, 0xC8], simple_tag)),
        )
    }

    fn large_attachment() -> Vec<u8> {
        let mut attached_file = Vec::new();
        attached_file.extend(element(&[0x46, 0x6E], b"cover.jpg".to_vec()));
        attached_file.extend(element(&[0x46, 0x60], b"image/jpeg".to_vec()));
        attached_file.extend(element(&[0x46, 0x5C], vec![0; TEST_LARGE_ELEMENT_SIZE]));
        element(&[0x19, 0x41, 0xA4, 0x69], element(&[0x61, 0xA7], attached_file))
    }

    fn large_sparse_webm() -> Vec<u8> {
        let mut segment_payload = Vec::new();
        segment_payload.extend(minimal_track());
        segment_payload.extend(large_tags());
        segment_payload.extend(large_attachment());
        segment_payload.extend(element(CUES_ID, vec![0; TEST_LARGE_ELEMENT_SIZE]));
        segment_payload
            .extend(element(&[0x1F, 0x43, 0xB6, 0x75], vec![0; TEST_LARGE_ELEMENT_SIZE]));

        let mut buf = Vec::new();
        buf.extend(ebml_header(b"webm"));
        buf.extend(element(&[0x18, 0x53, 0x80, 0x67], segment_payload));
        buf
    }

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
    fn opus_does_not_copy_container_bit_depth() {
        let mut opus = Vec::new();
        opus.extend(element(&[0xD7], vec![1]));
        opus.extend(element(&[0x83], vec![2]));
        opus.extend(element(&[0x86], b"A_OPUS".to_vec()));
        opus.extend(element(&[0xE1], element(&[0x62, 0x64], vec![32])));

        let mut pcm = Vec::new();
        pcm.extend(element(&[0xD7], vec![1]));
        pcm.extend(element(&[0x83], vec![2]));
        pcm.extend(element(&[0x86], b"A_PCM/INT/LIT".to_vec()));
        pcm.extend(element(&[0xE1], element(&[0x62, 0x64], vec![24])));

        for (track, expect) in [(opus, None), (pcm, Some("24"))] {
            let buf = {
                let mut buf = Vec::new();
                buf.extend(ebml_header(b"webm"));
                buf.extend(element(
                    &[0x18, 0x53, 0x80, 0x67],
                    element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track)),
                ));
                buf
            };
            let mut fa = FileAnalyze::new(&buf);
            assert!(parse_mkv(&mut fa));
            assert_eq!(
                fa.retrieve(StreamKind::Audio, 0, "BitDepth")
                    .map(|z| z.as_str().to_owned())
                    .as_deref(),
                expect
            );
        }
    }

    #[test]
    fn subtitle_track_is_text() {
        let mut track = Vec::new();
        track.extend(element(&[0xD7], vec![3]));
        track.extend(element(&[0x83], vec![0x11]));
        track.extend(element(&[0x86], b"S_TEXT/UTF8".to_vec()));

        let mut buf = Vec::new();
        buf.extend(ebml_header(b"matroska"));
        buf.extend(element(
            &[0x18, 0x53, 0x80, 0x67],
            element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track)),
        ));

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mkv(&mut fa));
        let field = |kind, name: &str| fa.retrieve(kind, 0, name).map(|z| z.as_str().to_owned());
        assert_eq!(field(StreamKind::General, "TextCount").as_deref(), Some("1"));
        assert_eq!(field(StreamKind::Text, "Format").as_deref(), Some("UTF-8"));
        assert_eq!(field(StreamKind::Text, "CodecID").as_deref(), Some("S_TEXT/UTF8"));
        assert_eq!(field(StreamKind::Text, "ID").as_deref(), Some("3"));
        assert_eq!(fa.stream_count(StreamKind::Audio), 0);
        assert_eq!(fa.stream_count(StreamKind::Video), 0);
    }

    #[test]
    fn segment_info_chapters_attachments_and_statistics() {
        let mut info = Vec::new();
        info.extend(element(&[0x7B, 0xA9], b"Demo".to_vec()));
        info.extend(element(&[0x44, 0x61], vec![0; 8]));
        info.extend(element(&[0x4D, 0x80], b"libebml".to_vec()));
        info.extend(element(&[0x57, 0x41], b"mkvmerge".to_vec()));

        let mut track = Vec::new();
        track.extend(element(&[0xD7], vec![1]));
        track.extend(element(&[0x73, 0xC5], vec![42]));
        track.extend(element(&[0x83], vec![2]));
        track.extend(element(&[0x86], b"A_AAC".to_vec()));
        track.extend(element(&[0x22, 0xB5, 0x9C], b"eng".to_vec()));
        track.extend(element(&[0x22, 0xB5, 0x9D], b"pt-BR".to_vec()));
        track.extend(element(&[0x55, 0xAB], vec![1]));
        track.extend(element(&[0x55, 0xAF], vec![1]));

        let mut simple = Vec::new();
        for (name, value) in [
            ("BPS", "96000"),
            ("NUMBER_OF_BYTES", "12000"),
            ("NUMBER_OF_FRAMES", "50"),
            ("DURATION", "00:00:01.500000000"),
        ] {
            let mut tag = Vec::new();
            tag.extend(element(&[0x45, 0xA3], name.as_bytes().to_vec()));
            tag.extend(element(&[0x44, 0x87], value.as_bytes().to_vec()));
            simple.extend(element(&[0x67, 0xC8], tag));
        }
        let mut tag = element(&[0x63, 0xC0], element(&[0x63, 0xC5], vec![42]));
        tag.extend(simple);
        let tags = element(&[0x12, 0x54, 0xC3, 0x67], element(&[0x73, 0x73], tag));

        let chapter = |ns: u64, name: &[u8]| {
            let mut atom = Vec::new();
            atom.extend(element(&[0x91], ns.to_be_bytes().to_vec()));
            atom.extend(element(&[0x80], element(&[0x85], name.to_vec())));
            element(&[0xB6], atom)
        };
        let mut edition = Vec::new();
        edition.extend(chapter(0, b"Intro"));
        edition.extend(chapter(1_500_000_000, b"Next"));
        let chapters = element(&[0x10, 0x43, 0xA7, 0x70], element(&[0x45, 0xB9], edition));

        let mut attached = Vec::new();
        attached.extend(element(&[0x46, 0x6E], b"cover.jpg".to_vec()));
        attached.extend(element(&[0x46, 0x60], b"image/jpeg".to_vec()));
        let attachments = element(&[0x19, 0x41, 0xA4, 0x69], element(&[0x61, 0xA7], attached));

        let mut segment = Vec::new();
        segment.extend(element(&[0x15, 0x49, 0xA9, 0x66], info));
        segment.extend(element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track)));
        segment.extend(tags);
        segment.extend(chapters);
        segment.extend(attachments);

        let mut buf = Vec::new();
        buf.extend(ebml_header(b"matroska"));
        buf.extend(element(&[0x18, 0x53, 0x80, 0x67], segment));

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mkv(&mut fa));
        let field =
            |kind, pos, name: &str| fa.retrieve(kind, pos, name).map(|z| z.as_str().to_owned());
        assert_eq!(field(StreamKind::General, 0, "Title").as_deref(), Some("Demo"));
        assert_eq!(
            field(StreamKind::General, 0, "Encoded_Date").as_deref(),
            Some("2001-01-01 00:00:00 UTC")
        );
        assert_eq!(field(StreamKind::General, 0, "Encoded_Library").as_deref(), Some("libebml"));
        assert_eq!(
            field(StreamKind::General, 0, "Encoded_Application").as_deref(),
            Some("mkvmerge")
        );
        assert_eq!(field(StreamKind::Audio, 0, "Language").as_deref(), Some("pt-BR"));
        assert_eq!(field(StreamKind::Audio, 0, "Channels").as_deref(), Some("1"));
        assert_eq!(field(StreamKind::Audio, 0, "ChannelLayout").as_deref(), Some("M"));
        assert_eq!(field(StreamKind::Audio, 0, "ServiceKind").as_deref(), Some("HI / C"));
        assert_eq!(
            field(StreamKind::Audio, 0, "ServiceKind_String").as_deref(),
            Some("Hearing Impaired / Commentary")
        );
        assert_eq!(field(StreamKind::Audio, 0, "BitRate").as_deref(), Some("96000"));
        assert_eq!(field(StreamKind::Audio, 0, "StreamSize").as_deref(), Some("12000"));
        assert_eq!(field(StreamKind::Audio, 0, "FrameCount").as_deref(), Some("50"));
        assert_eq!(field(StreamKind::Audio, 0, "Duration").as_deref(), Some("1500"));
        assert_eq!(field(StreamKind::General, 0, "MenuCount").as_deref(), Some("1"));
        assert_eq!(field(StreamKind::Menu, 0, "00:00:00.000").as_deref(), Some("Intro"));
        assert_eq!(field(StreamKind::Menu, 0, "00:00:01.500").as_deref(), Some("Next"));
        assert_eq!(field(StreamKind::General, 0, "ImageCount").as_deref(), Some("1"));
        assert_eq!(field(StreamKind::Image, 0, "Format").as_deref(), Some("JPEG"));
        assert_eq!(field(StreamKind::Image, 0, "Title").as_deref(), Some("cover.jpg"));
        assert_eq!(field(StreamKind::General, 0, "Cover").as_deref(), Some("Yes"));
    }

    #[test]
    fn truehd_track_is_audio() {
        let mut track = Vec::new();
        track.extend(element(&[0xD7], vec![1]));
        track.extend(element(&[0x83], vec![0x02]));
        track.extend(element(&[0x86], b"A_TRUEHD".to_vec()));

        let mut buf = Vec::new();
        buf.extend(ebml_header(b"matroska"));
        buf.extend(element(
            &[0x18, 0x53, 0x80, 0x67],
            element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track)),
        ));

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mkv(&mut fa));
        let field = |kind, name: &str| fa.retrieve(kind, 0, name).map(|z| z.as_str().to_owned());
        assert_eq!(field(StreamKind::General, "AudioCount").as_deref(), Some("1"));
        assert_eq!(field(StreamKind::Audio, "Format").as_deref(), Some("TrueHD"));
        assert_eq!(field(StreamKind::Audio, "CodecID").as_deref(), Some("A_TRUEHD"));
    }

    #[test]
    fn rejects_non_ebml_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a Matroska file at all");
        assert!(!parse_mkv(&mut fa));
    }

    #[test]
    fn skips_oversized_codec_private_payload() {
        let ebml = element(&[0x1A, 0x45, 0xDF, 0xA3], element(&[0x42, 0x82], b"matroska".to_vec()));

        let mut track = Vec::new();
        track.extend(element(&[0xD7], vec![1]));
        track.extend(element(&[0x83], vec![2]));
        track.extend(element(&[0x86], b"A_OPUS".to_vec()));
        track.extend(element(&[0x63, 0xA2], vec![0; MKV_CODEC_PRIVATE_LIMIT + 1024]));

        let segment = element(
            &[0x18, 0x53, 0x80, 0x67],
            element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track)),
        );

        let mut buf = Vec::new();
        buf.extend(ebml);
        buf.extend(segment);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mkv(&mut fa));
        assert!(fa.access_stats().max_request_len <= MKV_CODEC_PRIVATE_LIMIT);
    }

    #[test]
    fn large_webm_metadata_only_access_stays_bounded() {
        let buf = large_sparse_webm();
        assert!(buf.len() as u64 > MKV_METADATA_ONLY_BUDGET);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mkv(&mut fa));

        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("WebM")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Cover").map(|z| z.as_str().to_owned()).as_deref(),
            Some("Yes")
        );
        let stats = fa.access_stats();
        assert!(stats.bytes_requested < MKV_METADATA_ONLY_BUDGET, "{stats:?}");
        assert!(stats.bytes_returned < MKV_METADATA_ONLY_BUDGET, "{stats:?}");
        assert!(stats.max_request_len <= MKV_TEXT_VALUE_LIMIT, "{stats:?}");
    }

    /// Build a profile-0, 8-bit VP9 key-frame uncompressed header (MSB-first).
    fn vp9_keyframe_profile0(width: u16, height: u16) -> Vec<u8> {
        fn push(bytes: &mut Vec<u8>, bitpos: &mut usize, val: u32, n: usize) {
            for k in (0..n).rev() {
                let bit = ((val >> k) & 1) as u8;
                let idx = *bitpos / 8;
                if idx >= bytes.len() {
                    bytes.push(0);
                }
                bytes[idx] |= bit << (7 - (*bitpos % 8));
                *bitpos += 1;
            }
        }
        let mut b = Vec::new();
        let mut p = 0usize;
        push(&mut b, &mut p, 0b10, 2); // frame_marker
        push(&mut b, &mut p, 0, 1); // version0
        push(&mut b, &mut p, 0, 1); // version1 -> profile 0
        push(&mut b, &mut p, 0, 1); // show_existing_frame
        push(&mut b, &mut p, 0, 1); // frame_type = key
        push(&mut b, &mut p, 1, 1); // show_frame
        push(&mut b, &mut p, 0, 1); // error_resilient_mode
        push(&mut b, &mut p, 0x498342, 24); // sync code
        push(&mut b, &mut p, 0, 3); // colorspace = CS_UNKNOWN
        push(&mut b, &mut p, 0, 1); // reserved (colorspace == 0)
        push(&mut b, &mut p, (width - 1) as u32, 16);
        push(&mut b, &mut p, (height - 1) as u32, 16);
        push(&mut b, &mut p, 0, 1); // has_scaling
        b
    }

    fn vp9_mkv_sample() -> Vec<u8> {
        // Cluster with a Timestamp + a keyframe SimpleBlock for track 1.
        let frame = vp9_keyframe_profile0(320, 240);
        let mut block = Vec::new();
        block.push(0x81); // track number VINT = 1
        block.extend_from_slice(&[0x00, 0x00]); // relative timecode
        block.push(0x80); // flags: keyframe, no lacing
        block.extend_from_slice(&frame);
        let mut cluster_children = Vec::new();
        cluster_children.extend(element(&[0xE7], vec![0])); // Timestamp
        cluster_children.extend(element(&[0xA3], block)); // SimpleBlock
        let cluster = element(&[0x1F, 0x43, 0xB6, 0x75], cluster_children);

        // Video track (V_VP9) with DefaultDuration = 40 ms (25 fps) and a
        // Colour element carrying only Range = Limited.
        let colour = element(&[0x55, 0xB9], vec![1]);
        let mut video = Vec::new();
        video.extend(element(&[0xB0], vec![0x01, 0x40])); // PixelWidth 320
        video.extend(element(&[0xBA], vec![0x00, 0xF0])); // PixelHeight 240
        video.extend(element(&[0x55, 0xB0], colour));
        let mut track = Vec::new();
        track.extend(element(&[0xD7], vec![1])); // TrackNumber
        track.extend(element(&[0x73, 0xC5], vec![0x2A])); // TrackUID = 42
        track.extend(element(&[0x83], vec![1])); // TrackType = video
        track.extend(element(&[0x86], b"V_VP9".to_vec())); // CodecID
        track.extend(element(&[0x23, 0xE3, 0x83], vec![0x02, 0x62, 0x5A, 0x00])); // DefaultDuration 40000000ns
        track.extend(element(&[0xE0], video));
        let tracks = element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track));

        // Segment info: TimecodeScale 1e6, Duration 2000 units (2.0 s).
        let mut info = Vec::new();
        info.extend(element(&[0x2A, 0xD7, 0xB1], vec![0x0F, 0x42, 0x40]));
        info.extend(element(&[0x44, 0x89], 2000.0f64.to_be_bytes().to_vec()));
        let seg_info = element(&[0x15, 0x49, 0xA9, 0x66], info);

        // Track-targeted ENCODER tag.
        let mut simple_tag = Vec::new();
        simple_tag.extend(element(&[0x45, 0xA3], b"ENCODER".to_vec()));
        simple_tag.extend(element(&[0x44, 0x87], b"Lavc-test libvpx-vp9".to_vec()));
        let mut tag = Vec::new();
        tag.extend(element(&[0x63, 0xC0], element(&[0x63, 0xC5], vec![0x2A])));
        tag.extend(element(&[0x67, 0xC8], simple_tag));
        let tags = element(&[0x12, 0x54, 0xC3, 0x67], element(&[0x73, 0x73], tag));

        let mut segment_payload = Vec::new();
        segment_payload.extend(seg_info);
        segment_payload.extend(tracks);
        segment_payload.extend(tags);
        segment_payload.extend(cluster);

        let mut buf = Vec::new();
        buf.extend(ebml_header(b"matroska"));
        buf.extend(element(&[0x18, 0x53, 0x80, 0x67], segment_payload));
        buf
    }

    #[test]
    fn vp9_track_derives_profile_and_bitdepth_from_bitstream() {
        let buf = vp9_mkv_sample();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mkv(&mut fa));

        let v = |name: &str| fa.retrieve(StreamKind::Video, 0, name).map(|z| z.as_str().to_owned());
        assert_eq!(v("Format").as_deref(), Some("VP9"));
        // No CodecPrivate: profile & bit depth come from the keyframe.
        assert_eq!(v("Format_Profile").as_deref(), Some("0"));
        assert_eq!(v("BitDepth").as_deref(), Some("8"));
        // FrameRate derived from DefaultDuration (40 ms → 25 fps).
        assert_eq!(v("FrameRate_Mode").as_deref(), Some("CFR"));
        assert_eq!(v("FrameRate").as_deref(), Some("25.000"));
        assert_eq!(v("FrameRate_Num").as_deref(), Some("25"));
        assert_eq!(v("FrameRate_Den").as_deref(), Some("1"));
        assert_eq!(v("Duration").as_deref(), Some("2000"));
        assert_eq!(v("Delay").as_deref(), Some("0.000"));
        // Colour source labels match MediaInfoLib for VP9.
        assert_eq!(v("colour_description_present_Source").as_deref(), Some("Container"));
        assert_eq!(v("matrix_coefficients_Source").as_deref(), Some("Stream"));
        // ColorSpace is omitted for VP9.
        assert_eq!(v("ColorSpace"), None);
        // Encoded_Library from the track-targeted ENCODER tag.
        assert_eq!(v("Encoded_Library").as_deref(), Some("Lavc-test libvpx-vp9"));
    }
}
