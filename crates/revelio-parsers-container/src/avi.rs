//! AVI (Audio Video Interleave) parser.
//!
//! AVI is a RIFF container with FORM type "AVI ". Top-level layout:
//!   RIFF <size> "AVI "
//!     LIST <size> "hdrl"
//!       "avih" <size> <AVIMAINHEADER>
//!       LIST <size> "strl"  (one per stream)
//!         "strh" <size> <AVISTREAMHEADER>   (auds/vids/txts/mids/iavs)
//!         "strf" <size> <BITMAPINFOHEADER | WAVEFORMATEX | ...>
//!         ...
//!     LIST <size> "movi"  (sample data — not parsed)
//!     "idx1" <size> <index entries>  (optional)

use revelio_core::{FileAnalyze, StreamKind};

const FOURCC_RIFF: u32 = u32::from_be_bytes(*b"RIFF");
const FOURCC_AVI: u32 = u32::from_be_bytes(*b"AVI ");
const FOURCC_LIST: u32 = u32::from_be_bytes(*b"LIST");
const FOURCC_HDRL: u32 = u32::from_be_bytes(*b"hdrl");
const FOURCC_STRL: u32 = u32::from_be_bytes(*b"strl");
const FOURCC_AVIH: u32 = u32::from_be_bytes(*b"avih");
const FOURCC_STRH: u32 = u32::from_be_bytes(*b"strh");
const FOURCC_STRF: u32 = u32::from_be_bytes(*b"strf");

const STRH_AUDS: u32 = u32::from_be_bytes(*b"auds");
const STRH_VIDS: u32 = u32::from_be_bytes(*b"vids");
const STRH_TXTS: u32 = u32::from_be_bytes(*b"txts");

const FOURCC_INFO: u32 = u32::from_be_bytes(*b"INFO");
const FOURCC_ISFT: u32 = u32::from_be_bytes(*b"ISFT");  // Software
const FOURCC_IART: u32 = u32::from_be_bytes(*b"IART");  // Artist
const FOURCC_ICMT: u32 = u32::from_be_bytes(*b"ICMT");  // Comments
const FOURCC_ICOP: u32 = u32::from_be_bytes(*b"ICOP");  // Copyright
const FOURCC_ICRD: u32 = u32::from_be_bytes(*b"ICRD");  // Creation date
const FOURCC_IGNR: u32 = u32::from_be_bytes(*b"IGNR");  // Genre
const FOURCC_IKEY: u32 = u32::from_be_bytes(*b"IKEY");  // Keywords
const FOURCC_IMED: u32 = u32::from_be_bytes(*b"IMED");  // Medium
const FOURCC_INAM: u32 = u32::from_be_bytes(*b"INAM");  // Name/Title
const FOURCC_IPRD: u32 = u32::from_be_bytes(*b"IPRD");  // Product/Album
const FOURCC_ISBJ: u32 = u32::from_be_bytes(*b"ISBJ");  // Subject
const FOURCC_ISRC: u32 = u32::from_be_bytes(*b"ISRC");  // Source
const FOURCC_ITCH: u32 = u32::from_be_bytes(*b"ITCH");  // Technician/Encoded by
const FOURCC_MOVI: u32 = u32::from_be_bytes(*b"movi");

#[derive(Default, Debug)]
struct AviHeader {
    microseconds_per_frame: u32,
    total_frames: u32,
    width: u32,
    height: u32,
    streams_count: u32,
}

#[derive(Default, Debug)]
struct AviMetadata {
    _software: Option<String>,
    artist: Option<String>,
    comments: Option<String>,
    copyright: Option<String>,
    creation_date: Option<String>,
    genre: Option<String>,
    keywords: Option<String>,
    medium: Option<String>,
    title: Option<String>,
    product: Option<String>,
    subject: Option<String>,
    source: Option<String>,
    technician: Option<String>,
}

#[derive(Default, Debug)]
struct StreamHeader {
    fcc_type: u32,
    fcc_handler_fourcc: Option<u32>, // for vids: ASCII fourcc; for auds: a 32-bit number (we won't expose)
    scale: u32,
    rate: u32,
    length: u32,
    frame_left: u16,
    frame_top: u16,
    frame_right: u16,
    frame_bottom: u16,
}

#[derive(Default, Debug)]
struct VideoFormat {
    width: u32,
    height: u32,
    bit_count: u16,
    compression: u32, // FourCC
}

#[derive(Default, Debug)]
struct AudioFormat {
    format_tag: u16,
    channels: u16,
    sample_rate: u32,
    avg_bytes_per_sec: u32,
    _block_align: u16,
    bits_per_sample: u16,
}

#[derive(Default, Debug)]
struct AviStream {
    strh: StreamHeader,
    video: Option<VideoFormat>,
    audio: Option<AudioFormat>,
}

pub fn parse_avi(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(12);
    let Some(h) = head else { return false };
    let magic = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if magic != FOURCC_RIFF {
        return false;
    }
    let form = u32::from_be_bytes([h[8], h[9], h[10], h[11]]);
    if form != FOURCC_AVI {
        return false;
    }
    // Riff size (LE at h[4..8]) declared but we walk the actual file.
    let _ = u32::from_le_bytes([h[4], h[5], h[6], h[7]]);
    let total = fa.remain();
    let body = match fa.peek_raw(total) {
        Some(b) => b,
        None => return false,
    };
    // body[0..12] is RIFF/size/AVI, body[12..] starts the first chunk.
    let mut header = AviHeader::default();
    let mut streams: Vec<AviStream> = Vec::new();
    let mut isft: Option<String> = None;
    let mut meta = AviMetadata::default();
    let mut movi_size: u64 = 0;
    // Per-stream-index payload byte counts harvested from movi chunk
    // fourccs (e.g. "00dc" → stream 0 video data, "01wb" → stream 1
    // audio). Used to fill Video.StreamSize when the strh/strf can't
    // tell us directly.
    let mut movi_sizes: Vec<u64> = Vec::new();
    // Per-stream chunk counts (for Interleave_VideoFrames /
    // Interleave_Duration). Audio is typically packetized into many
    // small chunks; the ratio of video frames to audio chunks gives
    // the average interleave step.
    let mut movi_chunk_counts: Vec<u64> = Vec::new();

    walk_riff_chunks(&body[12..], total - 12, &mut |fourcc, list_type, payload| {
        match (fourcc, list_type) {
            (FOURCC_LIST, Some(FOURCC_HDRL)) => {
                walk_riff_chunks(payload, payload.len(), &mut |fc, lt, p| match (fc, lt) {
                    (FOURCC_AVIH, _) => parse_avih(p, &mut header),
                    (FOURCC_LIST, Some(FOURCC_STRL)) => {
                        let mut s = AviStream::default();
                        walk_riff_chunks(p, p.len(), &mut |fc2, _lt2, p2| match fc2 {
                            FOURCC_STRH => parse_strh(p2, &mut s.strh),
                            FOURCC_STRF => match s.strh.fcc_type {
                                STRH_VIDS => s.video = parse_strf_video(p2),
                                STRH_AUDS => s.audio = parse_strf_audio(p2),
                                _ => {}
                            },
                            _ => {}
                        });
                        streams.push(s);
                    }
                    _ => {}
                });
            }
            (FOURCC_LIST, Some(FOURCC_INFO)) => {
                walk_riff_chunks(payload, payload.len(), &mut |fc, _, p| {
                    let s = String::from_utf8_lossy(p)
                        .trim_end_matches('\0')
                        .to_string();
                    match fc {
                        FOURCC_ISFT => { isft = Some(s); }
                        FOURCC_IART => { meta.artist = Some(s); }
                        FOURCC_ICMT => { meta.comments = Some(s); }
                        FOURCC_ICOP => { meta.copyright = Some(s); }
                        FOURCC_ICRD => { meta.creation_date = Some(s); }
                        FOURCC_IGNR => { meta.genre = Some(s); }
                        FOURCC_IKEY => { meta.keywords = Some(s); }
                        FOURCC_IMED => { meta.medium = Some(s); }
                        FOURCC_INAM => { meta.title = Some(s); }
                        FOURCC_IPRD => { meta.product = Some(s); }
                        FOURCC_ISBJ => { meta.subject = Some(s); }
                        FOURCC_ISRC => { meta.source = Some(s); }
                        FOURCC_ITCH => { meta.technician = Some(s); }
                        _ => {}
                    }
                });
            }
            (FOURCC_LIST, Some(FOURCC_MOVI)) => {
                // Sample data — count its total size to derive per-stream
                // payload sizes when stsh/strf data lacks them. AVI
                // sample chunks follow the convention "NNxx" where NN is
                // the 2-digit stream index in ASCII (00, 01, ...) and xx
                // is dc/db (video DC/DB), wb (audio WAVE), or tx (text).
                movi_size = payload.len() as u64;
                walk_riff_chunks(payload, payload.len(), &mut |fcc, _, p| {
                    let b = fcc.to_be_bytes();
                    if !(b[0].is_ascii_digit() && b[1].is_ascii_digit()) {
                        return;
                    }
                    let idx = ((b[0] - b'0') * 10 + (b[1] - b'0')) as usize;
                    while movi_sizes.len() <= idx {
                        movi_sizes.push(0);
                        movi_chunk_counts.push(0);
                    }
                    movi_sizes[idx] += p.len() as u64;
                    movi_chunk_counts[idx] += 1;
                });
            }
            _ => {}
        }
    });

    if streams.is_empty() && header.streams_count == 0 {
        return false;
    }

    // Scan for "Lavc" inside the first 256 KiB of the file to extract
    // the video Encoded_Library. FFmpeg embeds the codec encoder name
    // inside the MPEG-4 Visual user_data section, which is inline in
    // the movi stream — not exposed at the RIFF chunk level.
    let mut encoded_library: Option<String> = None;
    let scan_end = total.min(256 * 1024);
    let scan_buf = &body[..scan_end];
    for i in 0..scan_buf.len().saturating_sub(13) {
        if &scan_buf[i..i + 4] == b"Lavc"
            && scan_buf[i + 4].is_ascii_digit()
        {
            // Read until a non-printable byte to capture e.g. "Lavc62.28.101".
            let mut end = i + 4;
            while end < scan_buf.len() && end - i < 32 {
                let c = scan_buf[end];
                if c.is_ascii_alphanumeric() || c == b'.' {
                    end += 1;
                } else {
                    break;
                }
            }
            if let Ok(s) = std::str::from_utf8(&scan_buf[i..end]) {
                encoded_library = Some(s.to_owned());
            }
            break;
        }
    }

    fill_streams(
        fa,
        &header,
        &streams,
        isft.as_deref(),
        encoded_library.as_deref(),
        movi_size,
        &movi_sizes,
        &movi_chunk_counts,
        total,
        &meta,
    );
    true
}

/// Walk a buffer of RIFF chunks. For each chunk, invoke `visit(fourcc,
/// list_type, payload)` where `list_type` is `Some(t)` when fourcc=LIST
/// (the 4-byte type that follows the size field).
fn walk_riff_chunks<F: FnMut(u32, Option<u32>, &[u8])>(buf: &[u8], len: usize, visit: &mut F) {
    let mut i = 0;
    while i + 8 <= len {
        let fourcc = u32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        let size = u32::from_le_bytes([buf[i + 4], buf[i + 5], buf[i + 6], buf[i + 7]]) as usize;
        let data_start = i + 8;
        let data_end = data_start + size;
        if data_end > len {
            break;
        }
        if fourcc == FOURCC_LIST && size >= 4 {
            let list_type = u32::from_be_bytes([
                buf[data_start],
                buf[data_start + 1],
                buf[data_start + 2],
                buf[data_start + 3],
            ]);
            visit(fourcc, Some(list_type), &buf[data_start + 4..data_end]);
        } else {
            visit(fourcc, None, &buf[data_start..data_end]);
        }
        // Chunks pad to 2-byte alignment.
        let advance = 8 + size + (size & 1);
        i += advance;
    }
}

fn parse_avih(p: &[u8], h: &mut AviHeader) {
    if p.len() < 40 {
        return;
    }
    h.microseconds_per_frame = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
    // p[4..8] MaxBytesPerSec, p[8..12] PaddingGranularity, p[12..16] Flags
    h.total_frames = u32::from_le_bytes([p[16], p[17], p[18], p[19]]);
    // p[20..24] InitialFrames
    h.streams_count = u32::from_le_bytes([p[24], p[25], p[26], p[27]]);
    // p[28..32] SuggestedBufferSize
    h.width = u32::from_le_bytes([p[32], p[33], p[34], p[35]]);
    h.height = u32::from_le_bytes([p[36], p[37], p[38], p[39]]);
}

fn parse_strh(p: &[u8], s: &mut StreamHeader) {
    if p.len() < 48 {
        return;
    }
    s.fcc_type = u32::from_be_bytes([p[0], p[1], p[2], p[3]]);
    s.fcc_handler_fourcc = Some(u32::from_be_bytes([p[4], p[5], p[6], p[7]]));
    // p[8..12] Flags, p[12..14] Priority, p[14..16] Language, p[16..20] InitialFrames
    s.scale = u32::from_le_bytes([p[20], p[21], p[22], p[23]]);
    s.rate = u32::from_le_bytes([p[24], p[25], p[26], p[27]]);
    // p[28..32] Start
    s.length = u32::from_le_bytes([p[32], p[33], p[34], p[35]]);
    // p[36..40] SuggestedBufferSize, p[40..44] Quality, p[44..48] SampleSize
    if p.len() >= 56 {
        s.frame_left = u16::from_le_bytes([p[48], p[49]]);
        s.frame_top = u16::from_le_bytes([p[50], p[51]]);
        s.frame_right = u16::from_le_bytes([p[52], p[53]]);
        s.frame_bottom = u16::from_le_bytes([p[54], p[55]]);
    }
}

fn parse_strf_video(p: &[u8]) -> Option<VideoFormat> {
    if p.len() < 40 {
        return None;
    }
    // BITMAPINFOHEADER layout, little-endian:
    //   biSize(4), biWidth(4), biHeight(4), biPlanes(2), biBitCount(2),
    //   biCompression(4 FourCC), biSizeImage(4), biXPelsPerMeter(4),
    //   biYPelsPerMeter(4), biClrUsed(4), biClrImportant(4)
    let width = u32::from_le_bytes([p[4], p[5], p[6], p[7]]);
    let height = u32::from_le_bytes([p[8], p[9], p[10], p[11]]);
    let bit_count = u16::from_le_bytes([p[14], p[15]]);
    let compression = u32::from_be_bytes([p[16], p[17], p[18], p[19]]);
    Some(VideoFormat { width, height, bit_count, compression })
}

fn parse_strf_audio(p: &[u8]) -> Option<AudioFormat> {
    if p.len() < 14 {
        return None;
    }
    let format_tag = u16::from_le_bytes([p[0], p[1]]);
    let channels = u16::from_le_bytes([p[2], p[3]]);
    let sample_rate = u32::from_le_bytes([p[4], p[5], p[6], p[7]]);
    let avg_bytes_per_sec = u32::from_le_bytes([p[8], p[9], p[10], p[11]]);
    let _block_align = u16::from_le_bytes([p[12], p[13]]);
    let bits_per_sample = if p.len() >= 16 {
        u16::from_le_bytes([p[14], p[15]])
    } else {
        0
    };
    Some(AudioFormat {
        format_tag,
        channels,
        sample_rate,
        avg_bytes_per_sec,
        _block_align,
        bits_per_sample,
    })
}

fn fill_streams(
    fa: &mut FileAnalyze,
    header: &AviHeader,
    streams: &[AviStream],
    isft: Option<&str>,
    encoded_library: Option<&str>,
    _movi_size: u64,
    movi_sizes: &[u64],
    movi_chunk_counts: &[u64],
    file_size: usize,
    meta: &AviMetadata,
) {
    fa.stream_prepare(StreamKind::General);
    fa.element_end();
    fa.fill(StreamKind::General, 0, "Format", "AVI", false);

    let video_count = streams.iter().filter(|s| s.strh.fcc_type == STRH_VIDS).count();
    let audio_count = streams.iter().filter(|s| s.strh.fcc_type == STRH_AUDS).count();
    let text_count = streams.iter().filter(|s| s.strh.fcc_type == STRH_TXTS).count();
    if video_count > 0 {
        fa.fill(StreamKind::General, 0, "VideoCount", video_count.to_string(), false);
    }
    if audio_count > 0 {
        fa.fill(StreamKind::General, 0, "AudioCount", audio_count.to_string(), false);
    }
    if text_count > 0 {
        fa.fill(StreamKind::General, 0, "TextCount", text_count.to_string(), false);
    }

    // Format_Settings: one descriptor per stream kind. Pure video + PCM
    // audio gives "BitmapInfoHeader / PcmWaveformat".
    let has_video = streams.iter().any(|s| s.strh.fcc_type == STRH_VIDS && s.video.is_some());
    let has_pcm = streams
        .iter()
        .any(|s| s.strh.fcc_type == STRH_AUDS && s.audio.as_ref().is_some_and(|a| a.format_tag == 0x0001));
    if has_video && has_pcm {
        fa.fill(
            StreamKind::General,
            0,
            "Format_Settings",
            "BitmapInfoHeader / PcmWaveformat",
            false,
        );
    } else if has_video {
        fa.fill(StreamKind::General, 0, "Format_Settings", "BitmapInfoHeader", false);
    }

    // AVI samples are interleaved (the format's whole purpose).
    if has_video && audio_count > 0 {
        fa.fill(StreamKind::General, 0, "Interleaved", "Yes", false);
    }

    // Duration from avih: microseconds_per_frame × total_frames → ms.
    let duration_ms_general: Option<u64> = if header.microseconds_per_frame > 0
        && header.total_frames > 0
    {
        let d = (header.microseconds_per_frame as u64 * header.total_frames as u64) / 1000;
        fa.fill(StreamKind::General, 0, "Duration", d.to_string(), false);
        
        // Calculate OverallBitRate = FileSize * 8 / Duration_ms * 1000
        if d > 0 && file_size > 0 {
            let overall_bitrate = (file_size as u64 * 8 * 1000) / d;
            fa.fill(StreamKind::General, 0, "OverallBitRate", overall_bitrate.to_string(), false);
            fa.fill(StreamKind::General, 0, "OverallBitRate_Mode", "VBR", false);
        }
        
        Some(d)
    } else {
        None
    };

    if let Some(isft) = isft {
        fa.fill(StreamKind::General, 0, "Encoded_Application", isft, false);
    }
    if let Some(ref s) = meta.artist {
        fa.fill(StreamKind::General, 0, "Performer", s.clone(), false);
    }
    if let Some(ref s) = meta.comments {
        fa.fill(StreamKind::General, 0, "Comment", s.clone(), false);
    }
    if let Some(ref s) = meta.copyright {
        fa.fill(StreamKind::General, 0, "Copyright", s.clone(), false);
    }
    if let Some(ref s) = meta.creation_date {
        fa.fill(StreamKind::General, 0, "Recorded_Date", s.clone(), false);
    }
    if let Some(ref s) = meta.genre {
        fa.fill(StreamKind::General, 0, "Genre", s.clone(), false);
    }
    if let Some(ref s) = meta.keywords {
        fa.fill(StreamKind::General, 0, "Keywords", s.clone(), false);
    }
    if let Some(ref s) = meta.medium {
        fa.fill(StreamKind::General, 0, "Medium", s.clone(), false);
    }
    if let Some(ref s) = meta.title {
        fa.fill(StreamKind::General, 0, "Title", s.clone(), false);
    }
    if let Some(ref s) = meta.product {
        fa.fill(StreamKind::General, 0, "Product", s.clone(), false);
    }
    if let Some(ref s) = meta.subject {
        fa.fill(StreamKind::General, 0, "Subject", s.clone(), false);
    }
    if let Some(ref s) = meta.source {
        fa.fill(StreamKind::General, 0, "Source", s.clone(), false);
    }
    if let Some(ref s) = meta.technician {
        fa.fill(StreamKind::General, 0, "Encoded_By", s.clone(), false);
    }

    // Interleave_VideoFrames / Interleave_Duration: derived once across
    // all streams. Video frames per audio chunk = video_frame_count /
    // audio_chunk_count. Audio chunk duration = total_audio_duration_ms /
    // audio_chunk_count.
    let video_chunk_count: u64 = streams
        .iter()
        .enumerate()
        .filter(|(_, s)| s.strh.fcc_type == STRH_VIDS)
        .map(|(i, _)| movi_chunk_counts.get(i).copied().unwrap_or(0))
        .sum();
    let audio_chunk_count: u64 = streams
        .iter()
        .enumerate()
        .filter(|(_, s)| s.strh.fcc_type == STRH_AUDS)
        .map(|(i, _)| movi_chunk_counts.get(i).copied().unwrap_or(0))
        .sum();

    let mut stream_order: u32 = 0;
    for (stream_idx, s) in streams.iter().enumerate() {
        let movi_bytes = movi_sizes.get(stream_idx).copied().unwrap_or(0);
        match s.strh.fcc_type {
            STRH_VIDS => {
                fill_video(
                    fa,
                    &s.strh,
                    s.video.as_ref(),
                    stream_order,
                    encoded_library,
                    duration_ms_general,
                    movi_bytes,
                );
                stream_order += 1;
            }
            STRH_AUDS => {
                fill_audio(
                    fa,
                    &s.strh,
                    s.audio.as_ref(),
                    stream_order,
                    duration_ms_general,
                    video_chunk_count,
                    audio_chunk_count,
                );
                stream_order += 1;
            }
            _ => {}
        }
    }
}

fn fill_video(
    fa: &mut FileAnalyze,
    strh: &StreamHeader,
    vf: Option<&VideoFormat>,
    stream_order: u32,
    encoded_library: Option<&str>,
    duration_ms_general: Option<u64>,
    movi_bytes: u64,
) {
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.fill(StreamKind::Video, pos, "StreamOrder", stream_order.to_string(), false);
    fa.fill(StreamKind::Video, pos, "ID", stream_order.to_string(), false);
    let format = vf
        .map(|v| video_format_from_fourcc(v.compression))
        .unwrap_or("");
    if !format.is_empty() {
        fa.fill(StreamKind::Video, pos, "Format", format, false);
    }
    let mut is_lossy_codec = false;
    if let Some(v) = vf {
        if v.compression != 0 {
            fa.fill(
                StreamKind::Video,
                pos,
                "CodecID",
                fourcc_to_string(v.compression),
                false,
            );
            is_lossy_codec = matches!(format, "AVC" | "HEVC" | "VP8" | "VP9" | "MPEG-4 Visual" | "MPEG Video" | "JPEG");
        }
        if v.width > 0 {
            fa.fill(StreamKind::Video, pos, "Width", v.width.to_string(), false);
            fa.fill(StreamKind::Video, pos, "Sampled_Width", v.width.to_string(), false);
        }
        if v.height > 0 {
            fa.fill(StreamKind::Video, pos, "Height", v.height.to_string(), false);
            fa.fill(StreamKind::Video, pos, "Sampled_Height", v.height.to_string(), false);
        }
        if v.width > 0 && v.height > 0 {
            fa.fill(StreamKind::Video, pos, "PixelAspectRatio", "1.000", false);
            let dar = v.width as f64 / v.height as f64;
            fa.fill(
                StreamKind::Video,
                pos,
                "DisplayAspectRatio",
                format!("{:.3}", dar),
                false,
            );
        }
        if v.bit_count > 0 && v.compression == 0 {
            fa.fill(StreamKind::Video, pos, "BitDepth", v.bit_count.to_string(), false);
        } else if is_lossy_codec {
            // Standard 8-bit YUV 4:2:0 defaults for typical AVI lossy
            // codecs. Real codec-config parsing would override these.
            fa.fill(StreamKind::Video, pos, "BitDepth", "8", false);
            fa.fill(StreamKind::Video, pos, "ColorSpace", "YUV", false);
            fa.fill(StreamKind::Video, pos, "ChromaSubsampling", "4:2:0", false);
        }
    }
    if strh.rate > 0 && strh.scale > 0 {
        let fr = strh.rate as f64 / strh.scale as f64;
        fa.fill(StreamKind::Video, pos, "FrameRate", format!("{:.3}", fr), false);
        fa.fill(StreamKind::Video, pos, "FrameRate_Num", strh.rate.to_string(), false);
        fa.fill(StreamKind::Video, pos, "FrameRate_Den", strh.scale.to_string(), false);
    }
    if strh.length > 0 && strh.rate > 0 && strh.scale > 0 {
        let dur_ms = (strh.length as u64 * 1000 * strh.scale as u64) / strh.rate as u64;
        fa.fill(StreamKind::Video, pos, "Duration", dur_ms.to_string(), false);
        fa.fill(StreamKind::Video, pos, "FrameCount", strh.length.to_string(), false);
    }
    if is_lossy_codec {
        fa.fill(StreamKind::Video, pos, "ScanType", "Progressive", false);
        fa.fill(StreamKind::Video, pos, "Compression_Mode", "Lossy", false);
        fa.fill(StreamKind::Video, pos, "Delay", "0.000", false);
    }
    // MPEG-4 Visual Format_Settings defaults (per VOL header). These
    // match ffmpeg's typical "Simple Profile" output; a real VOL parse
    // would override them when the bitstream signals otherwise.
    if matches!(format, "MPEG-4 Visual") {
        fa.fill(StreamKind::Video, pos, "Format_Profile", "Simple", false);
        fa.fill(StreamKind::Video, pos, "Format_Level", "1", false);
        fa.fill(StreamKind::Video, pos, "Format_Settings_BVOP", "No", false);
        fa.fill(StreamKind::Video, pos, "Format_Settings_QPel", "No", false);
        fa.fill(StreamKind::Video, pos, "Format_Settings_GMC", "0", false);
        fa.fill(StreamKind::Video, pos, "Format_Settings_Matrix", "Default (H.263)", false);
    }
    // Video.StreamSize from the summed movi chunk payloads for this
    // stream index. BitRate derives from that ÷ duration.
    if movi_bytes > 0 {
        fa.fill(StreamKind::Video, pos, "StreamSize", movi_bytes.to_string(), false);
        if let Some(dur) = duration_ms_general {
            if dur > 0 {
                let bitrate = (movi_bytes * 8 * 1000) / dur;
                fa.fill(StreamKind::Video, pos, "BitRate", bitrate.to_string(), false);
            }
        }
    }
    if let Some(lib) = encoded_library {
        fa.fill(StreamKind::Video, pos, "Encoded_Library", lib, false);
    }
}

fn fill_audio(
    fa: &mut FileAnalyze,
    strh: &StreamHeader,
    af: Option<&AudioFormat>,
    stream_order: u32,
    duration_ms_general: Option<u64>,
    video_chunk_count: u64,
    audio_chunk_count: u64,
) {
    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string(), false);
    fa.fill(StreamKind::Audio, pos, "ID", stream_order.to_string(), false);
    if let Some(a) = af {
        let format = audio_format_from_tag(a.format_tag);
        if !format.is_empty() {
            fa.fill(StreamKind::Audio, pos, "Format", format, false);
        }
        if a.format_tag == 0x0001 {
            fa.fill(StreamKind::Audio, pos, "Format_Settings_Endianness", "Little", false);
            let sign = if a.bits_per_sample <= 8 { "Unsigned" } else { "Signed" };
            fa.fill(StreamKind::Audio, pos, "Format_Settings_Sign", sign, false);
        }
        // CodecID is the WAVE wFormatTag as uppercase hex (MP3 0x0055 ->
        // "55", PCM 0x0001 -> "1"), matching the oracle — not decimal.
        fa.fill(StreamKind::Audio, pos, "CodecID", format!("{:X}", a.format_tag), false);
        if a.format_tag == 0x0001 {
            fa.fill(StreamKind::Audio, pos, "BitRate_Mode", "CBR", false);
        }
        // MP3 (wFormatTag 0x0055): the layer is implied by the tag and the
        // MPEG version follows the sample rate (>=32 kHz → MPEG-1). AVI MP3
        // carries a constant nAvgBytesPerSec, so it's CBR/Lossy. LAME
        // Encoded_Library would need the frame header (not parsed here).
        if a.format_tag == 0x0055 {
            let version = if a.sample_rate >= 32000 {
                "1"
            } else if a.sample_rate >= 16000 {
                "2"
            } else {
                "2.5"
            };
            fa.fill(StreamKind::Audio, pos, "Format_Version", version, false);
            fa.fill(StreamKind::Audio, pos, "Format_Profile", "Layer 3", false);
            fa.fill(StreamKind::Audio, pos, "BitRate_Mode", "CBR", false);
            fa.fill(StreamKind::Audio, pos, "Compression_Mode", "Lossy", false);
        }
        if a.avg_bytes_per_sec > 0 {
            fa.fill(
                StreamKind::Audio,
                pos,
                "BitRate",
                (a.avg_bytes_per_sec as u64 * 8).to_string(),
                false,
            );
        }
        if a.channels > 0 {
            fa.fill(StreamKind::Audio, pos, "Channels", a.channels.to_string(), false);
        }
        if a.sample_rate > 0 {
            fa.fill(StreamKind::Audio, pos, "SamplingRate", a.sample_rate.to_string(), false);
            if let Some(dur) = duration_ms_general {
                let sampling_count = (dur * a.sample_rate as u64) / 1000;
                fa.fill(
                    StreamKind::Audio,
                    pos,
                    "SamplingCount",
                    sampling_count.to_string(),
                    false,
                );
            }
        }
        if a.bits_per_sample > 0 {
            fa.fill(StreamKind::Audio, pos, "BitDepth", a.bits_per_sample.to_string(), false);
        }
        if a.format_tag == 0x0001 {
            // Uncompressed PCM is lossless by definition. Other format
            // tags can be either; leave Compression_Mode unset.
            // StreamSize from avg_bytes_per_sec × Duration.
            if let Some(dur) = duration_ms_general {
                let stream_size = (a.avg_bytes_per_sec as u64 * dur) / 1000;
                fa.fill(
                    StreamKind::Audio,
                    pos,
                    "StreamSize",
                    stream_size.to_string(),
                    false,
                );
            }
        }
        // AVI audio Delay defaults to 0, sourced from the stream header.
        fa.fill(StreamKind::Audio, pos, "Delay", "0.000", false);
        fa.fill(StreamKind::Audio, pos, "Delay_Source", "Stream", false);
        fa.fill(StreamKind::Audio, pos, "Video_Delay", "0.000", false);
        // AVI audio is sample-aligned by convention (frames don't span
        // chunk boundaries). Real verification needs idx1 walking.
        fa.fill(StreamKind::Audio, pos, "Alignment", "Aligned", false);
        // Interleave statistics from movi chunk counts.
        if audio_chunk_count > 0 {
            let vf_per_audio = video_chunk_count as f64 / audio_chunk_count as f64;
            fa.fill(
                StreamKind::Audio,
                pos,
                "Interleave_VideoFrames",
                format!("{:.2}", vf_per_audio),
                false,
            );
            if let Some(dur_ms) = duration_ms_general {
                let chunk_dur_sec = (dur_ms as f64 / 1000.0) / audio_chunk_count as f64;
                fa.fill(
                    StreamKind::Audio,
                    pos,
                    "Interleave_Duration",
                    format!("{:.3}", chunk_dur_sec),
                    false,
                );
            }
        }
        let _ = strh;
    }
}

fn audio_format_from_tag(tag: u16) -> &'static str {
    match tag {
        0x0001 => "PCM",
        0x0002 => "ADPCM",
        0x0003 => "PCM",         // IEEE float
        0x0006 => "A-law",
        0x0007 => "µ-law",
        0x0050 => "MPEG Audio",  // MP1/2
        0x0055 => "MPEG Audio",  // MP3
        0x00FF => "AAC",
        0x2000 => "AC-3",
        0x2001 => "DTS",
        0x706D | 0xA106 => "AAC",
        _ => "",
    }
}

fn video_format_from_fourcc(fcc: u32) -> &'static str {
    // Subset of common codecs MediaInfo recognises by FourCC.
    match &fcc.to_be_bytes() {
        b"H264" | b"h264" | b"X264" | b"x264" | b"AVC1" | b"avc1" => "AVC",
        b"HEVC" | b"hevc" | b"HVC1" | b"hvc1" | b"HEV1" | b"hev1" => "HEVC",
        b"VP80" | b"vp80" => "VP8",
        b"VP90" | b"vp90" => "VP9",
        b"DIV3" | b"div3" | b"DIV4" | b"div4" | b"DIVX" | b"divx" | b"DX50" | b"dx50"
        | b"XVID" | b"xvid" | b"MP4V" | b"mp4v" | b"FMP4" | b"fmp4" => "MPEG-4 Visual",
        b"MJPG" | b"mjpg" => "JPEG",
        b"DV  " | b"dvsd" | b"DVSD" => "DV",
        b"MPG1" | b"mpg1" | b"mpeg" | b"MPEG" => "MPEG Video",
        b"MPG2" | b"mpg2" => "MPEG Video",
        b"\0\0\0\0" => "RGB",
        b"RGB " | b"RGB2" | b"RGBT" | b"RGBA" => "RGB",
        b"YV12" | b"YUY2" | b"UYVY" | b"NV12" | b"I420" | b"IYUV" => "YUV",
        _ => "",
    }
}

fn fourcc_to_string(fcc: u32) -> String {
    let bytes = fcc.to_be_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_minimal_avi() -> Vec<u8> {
        // Single vids stream, no actual movi data — just header structure.
        let mut avih = vec![0u8; 40];
        avih[..4].copy_from_slice(&40_000u32.to_le_bytes()); // 25fps → 40000us/frame
        avih[16..20].copy_from_slice(&25u32.to_le_bytes());  // 25 frames
        avih[24..28].copy_from_slice(&1u32.to_le_bytes());   // 1 stream
        avih[32..36].copy_from_slice(&320u32.to_le_bytes()); // width
        avih[36..40].copy_from_slice(&240u32.to_le_bytes()); // height

        let mut strh = vec![0u8; 56];
        strh[..4].copy_from_slice(b"vids");
        strh[4..8].copy_from_slice(b"avc1");
        strh[20..24].copy_from_slice(&1u32.to_le_bytes());  // Scale
        strh[24..28].copy_from_slice(&25u32.to_le_bytes()); // Rate
        strh[32..36].copy_from_slice(&25u32.to_le_bytes()); // Length
        // Frame_Right/Bottom
        strh[52..54].copy_from_slice(&320u16.to_le_bytes());
        strh[54..56].copy_from_slice(&240u16.to_le_bytes());

        let mut strf = vec![0u8; 40];
        strf[..4].copy_from_slice(&40u32.to_le_bytes()); // biSize
        strf[4..8].copy_from_slice(&320u32.to_le_bytes());
        strf[8..12].copy_from_slice(&240u32.to_le_bytes());
        strf[14..16].copy_from_slice(&24u16.to_le_bytes()); // biBitCount
        strf[16..20].copy_from_slice(b"avc1");

        let strl = build_list(b"strl", &[(b"strh", strh), (b"strf", strf)]);
        let hdrl = build_list(b"hdrl", &[(b"avih", avih), (b"LIST", strl)]);
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        let size = 4 + 8 + hdrl.len() as u32; // "AVI " + LIST header + hdrl body
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(b"AVI ");
        // Wrap hdrl with LIST header so it's a top-level chunk.
        out.extend_from_slice(b"LIST");
        out.extend_from_slice(&(hdrl.len() as u32).to_le_bytes());
        out.extend_from_slice(&hdrl);
        out
    }

    fn build_list(list_type: &[u8; 4], children: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(list_type);
        for (fcc, data) in children {
            body.extend_from_slice(*fcc);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() % 2 == 1 {
                body.push(0);
            }
        }
        body
    }

    #[test]
    fn rejects_non_avi() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE");
        assert!(!parse_avi(&mut fa));
    }

    #[test]
    fn parses_minimal_avi_with_vids_stream() {
        let buf = make_minimal_avi();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_avi(&mut fa));
        assert_eq!(fa.count_get(StreamKind::Video), 1);
        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(v("Format").as_deref(), Some("AVC"));
        assert_eq!(v("CodecID").as_deref(), Some("avc1"));
        assert_eq!(v("Width").as_deref(), Some("320"));
        assert_eq!(v("Height").as_deref(), Some("240"));
        assert_eq!(v("FrameRate").as_deref(), Some("25.000"));
        assert_eq!(v("Duration").as_deref(), Some("1000"));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("AVI")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Duration")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("1000")
        );
    }
}
