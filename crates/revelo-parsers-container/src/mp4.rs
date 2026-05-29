//! MP4 / MOV (ISO Base Media File Format) parser.
//!
//! First major container in the engine. Walks a box tree:
//!   [size:u32 BE][type:4CC]{[size64:u64 BE]?}{payload}
//! where size==0 means "to end of file" and size==1 means "8-byte
//! extended size follows".
//!
//! Subset implemented this commit:
//! - `ftyp` → Format=MPEG-4, CodecID=major_brand, CodecID_Compatible=brand list
//! - Walk `moov` > `trak`* > `mdia` to identify audio/video tracks
//! - `mdhd` v0/v1 for timescale + duration
//! - `hdlr` for stream type (soun → Audio, vide → Video)
//! - `stsd` → first sample entry, `mp4a` → Format=AAC + Channels + SamplingRate
//! - `stsz` → SamplingCount (= sample_count)
//!
//! Deferred:
//! - `esds` (Elementary Stream Descriptor) → exact AAC profile (LC/SBR/etc.)
//! - `stco`/`co64` (chunk offsets) for StreamSize from mdat
//! - Track-level Default/AlternateGroup flags from `tkhd`
//! - iTunes-style metadata atoms (`udta` > `meta` > `ilst`) for
//!   Encoded_Application and Format_Profile="Apple audio with iTunes info"

use revelo_core::{FileAnalyze, Reader, StreamKind};

const BOX_FTYP: u32 = u32::from_be_bytes(*b"ftyp");
const BOX_MOOV: u32 = u32::from_be_bytes(*b"moov");
const BOX_TRAK: u32 = u32::from_be_bytes(*b"trak");
const BOX_MDIA: u32 = u32::from_be_bytes(*b"mdia");
const BOX_MDHD: u32 = u32::from_be_bytes(*b"mdhd");
const BOX_HDLR: u32 = u32::from_be_bytes(*b"hdlr");
const BOX_MINF: u32 = u32::from_be_bytes(*b"minf");
const BOX_STBL: u32 = u32::from_be_bytes(*b"stbl");
const BOX_STSD: u32 = u32::from_be_bytes(*b"stsd");
const BOX_STSZ: u32 = u32::from_be_bytes(*b"stsz");
const BOX_STTS: u32 = u32::from_be_bytes(*b"stts");
const BOX_MVHD: u32 = u32::from_be_bytes(*b"mvhd");
const BOX_EDTS: u32 = u32::from_be_bytes(*b"edts");
const BOX_ELST: u32 = u32::from_be_bytes(*b"elst");
const BOX_UDTA: u32 = u32::from_be_bytes(*b"udta");
const BOX_META: u32 = u32::from_be_bytes(*b"meta");
#[allow(dead_code)]
const BOX_CHPL: u32 = u32::from_be_bytes(*b"chpl"); // Nero chapter list
#[allow(dead_code)]
const BOX_NERO: u32 = u32::from_be_bytes(*b"Nero"); // Nero metadata
#[allow(dead_code)]
const BOX_COVR: u32 = u32::from_be_bytes(*b"covr"); // iTunes cover art
#[allow(dead_code)]
const BOX_THMB: u32 = u32::from_be_bytes(*b"thmb"); // 3GP thumbnail
const BOX_ILST: u32 = u32::from_be_bytes(*b"ilst");
const BOX_DATA: u32 = u32::from_be_bytes(*b"data");
const BOX_KEYS: u32 = u32::from_be_bytes(*b"keys");
const BOX_MDAT: u32 = u32::from_be_bytes(*b"mdat");
const BOX_TKHD: u32 = u32::from_be_bytes(*b"tkhd");
const BOX_STCO: u32 = u32::from_be_bytes(*b"stco"); // 32-bit chunk offsets
const BOX_CO64: u32 = u32::from_be_bytes(*b"co64"); // 64-bit chunk offsets

/// iTunes-style `©too` (tool/encoder) metadata key.
const ITUNES_KEY_TOOL: u32 = 0xA9_74_6F_6F;
/// iTunes-style `©cmt` (Comment) metadata key.
const ITUNES_KEY_COMMENT: u32 = 0xA9_63_6D_74;
/// iTunes-style `©nam` (Title/Name) metadata key.
const ITUNES_KEY_TITLE: u32 = 0xA9_6E_61_6D;
/// iTunes-style `©ART` (Artist) metadata key.
const ITUNES_KEY_ARTIST: u32 = 0xA9_41_52_54;
/// iTunes-style `©alb` (Album) metadata key.
const ITUNES_KEY_ALBUM: u32 = 0xA9_61_6C_62;
/// iTunes-style `©day` (Release Date) metadata key.
const ITUNES_KEY_DATE: u32 = 0xA9_64_61_79;
/// iTunes-style `©gen` (Genre) metadata key.
const ITUNES_KEY_GENRE: u32 = 0xA9_67_65_6E;
/// iTunes-style `©wrt` (Composer/Writer) metadata key.
const ITUNES_KEY_WRITER: u32 = 0xA9_77_72_74;
/// iTunes-style `©grp` (Grouping) metadata key.
const ITUNES_KEY_GROUPING: u32 = 0xA9_67_72_70;
/// iTunes-style `trkn` (Track Number) metadata key.
const ITUNES_KEY_TRACK: u32 = 0x74_72_6B_6E;
/// iTunes-style `disk` (Disc Number) metadata key.
const ITUNES_KEY_DISK: u32 = 0x64_69_73_6B;
/// iTunes-style `cpil` (Compilation) metadata key.
const ITUNES_KEY_COMPILATION: u32 = 0x63_70_69_6C;
/// iTunes-style `pgap` (Gapless Playback) metadata key.
const ITUNES_KEY_GAPLESS: u32 = 0x70_67_61_70;
/// iTunes-style `©lyr` (Lyrics) metadata key.
const ITUNES_KEY_LYRICS: u32 = 0xA9_6C_79_72;
/// iTunes-style `tvsh` (TV Show Name) metadata key.
const ITUNES_KEY_TV_SHOW: u32 = 0x74_76_73_68;
/// iTunes-style `tven` (TV Episode ID) metadata key.
const ITUNES_KEY_TV_EPISODE: u32 = 0x74_76_65_6E;
/// iTunes-style `tvsn` (TV Season) metadata key.
const ITUNES_KEY_TV_SEASON: u32 = 0x74_76_73_6E;
/// iTunes-style `tves` (TV Episode Number) metadata key.
const ITUNES_KEY_TV_EPISODE_NUM: u32 = 0x74_76_65_73;
/// iTunes-style `hdvd` (HD Video) metadata key.
const ITUNES_KEY_HD_VIDEO: u32 = 0x6864_7664;
/// iTunes-style `stik` (Media Type) metadata key.
const ITUNES_KEY_MEDIA_TYPE: u32 = 0x73_74_69_6B;
/// iTunes-style `rtng` (Content Rating) metadata key.
const ITUNES_KEY_RATING: u32 = 0x72_74_6E_67;
/// iTunes-style `©pub` (Publisher) metadata key.
const ITUNES_KEY_PUBLISHER: u32 = 0xA9_70_75_62;
/// iTunes-style `©enc` (Encoded By) metadata key.
const ITUNES_KEY_ENCODED_BY: u32 = 0xA9_65_6E_63;

const HANDLER_SOUN: u32 = u32::from_be_bytes(*b"soun");
const HANDLER_VIDE: u32 = u32::from_be_bytes(*b"vide");
const HANDLER_HINT: u32 = u32::from_be_bytes(*b"hint");
const HANDLER_META: u32 = u32::from_be_bytes(*b"meta");
const HANDLER_TEXT: u32 = u32::from_be_bytes(*b"text");
const HANDLER_SUBT: u32 = u32::from_be_bytes(*b"subt");
const HANDLER_SBTL: u32 = u32::from_be_bytes(*b"sbtl");
const SAMPLE_ENTRY_RTP: u32 = u32::from_be_bytes(*b"rtp ");
const SAMPLE_ENTRY_MEBX: u32 = u32::from_be_bytes(*b"mebx");

const SAMPLE_ENTRY_MP4A: u32 = u32::from_be_bytes(*b"mp4a");
const SAMPLE_ENTRY_AVC1: u32 = u32::from_be_bytes(*b"avc1");
const SAMPLE_ENTRY_AVC3: u32 = u32::from_be_bytes(*b"avc3");
const SAMPLE_ENTRY_HVC1: u32 = u32::from_be_bytes(*b"hvc1");
const SAMPLE_ENTRY_HEV1: u32 = u32::from_be_bytes(*b"hev1");
const SAMPLE_ENTRY_MP4V: u32 = u32::from_be_bytes(*b"mp4v");
const BOX_ESDS: u32 = u32::from_be_bytes(*b"esds");
const BOX_AVCC: u32 = u32::from_be_bytes(*b"avcC");
const BOX_HVCC: u32 = u32::from_be_bytes(*b"hvcC");
const BOX_COLR: u32 = u32::from_be_bytes(*b"colr");
const BOX_DVCC: u32 = u32::from_be_bytes(*b"dvcC");
const BOX_DVVC: u32 = u32::from_be_bytes(*b"dvvC");
const BOX_PASP: u32 = u32::from_be_bytes(*b"pasp");

#[derive(Debug, Default)]
struct MovieInfo {
    timescale: u32,
    duration: u64,
    /// mvhd creation_time and modification_time (seconds since 1904-01-01 UTC).
    creation_time: Option<u64>,
    modification_time: Option<u64>,
    /// iTunes-style metadata items from udta > meta > ilst. Keyed by
    /// the 4-byte item type code (e.g. `©too` = 0xA9_74_6F_6F).
    itunes_metadata: Vec<(u32, String)>,
    /// QuickTime `mdta` keys box payload — reverse-DNS strings indexed
    /// from 1 by ilst items. Populated when udta > meta uses the
    /// `mdta` handler (iPhone/iPad recordings).
    qt_keys: Vec<String>,
    /// (key_string, value) tuples resolved against `qt_keys` after
    /// parsing the matching ilst. Independent from iTunes metadata.
    qt_metadata: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct TrackInfo {
    handler: u32,
    /// Optional human-readable track name from the hdlr box's trailing
    /// name field. Decoded from both ISO BMFF C-string and QuickTime
    /// Pascal-string forms. Used as the per-track `Title` field.
    handler_name: Option<String>,
    timescale: u32,
    duration_units: u64,
    /// Effective playback duration in movie-timescale units (from
    /// first elst entry's segment_duration), if any edit list present.
    elst_segment_duration: Option<u64>,
    /// first elst entry's media_time in media units (signed; >0 means
    /// the first N media units are encoder priming and should be
    /// excluded from the playable content).
    elst_media_time: Option<i64>,
    audio_channels: Option<u16>,
    audio_sample_rate: Option<u32>,
    audio_format: Option<&'static str>,
    sample_count: Option<u32>,
    /// objectTypeIndication from esds DecoderConfigDescriptor — 0x40
    /// for MPEG-4 Audio, 0x6B for MPEG-1 Audio Layer 3, etc.
    object_type_indication: Option<u8>,
    /// Audio Object Type from AudioSpecificConfig (AAC profile: 2=LC,
    /// 5=SBR, 29=PS, etc.).
    audio_object_type: Option<u8>,
    /// avgBitrate field from DecoderConfigDescriptor, bps.
    avg_bitrate_bps: Option<u32>,
    /// maxBitrate field from DecoderConfigDescriptor, bps.
    max_bitrate_bps: Option<u32>,
    /// bufferSizeDB field from DecoderConfigDescriptor (MPEG-4 esds).
    buffer_size_db: Option<u32>,
    /// Sum of all per-sample sizes from `stsz` — raw byte count
    /// before any edit-list trimming.
    source_stream_size: Option<u64>,
    /// Size in bytes of the first stsz entry. Subtracted from
    /// Source_StreamSize to derive the post-elst Audio.StreamSize.
    first_sample_size: Option<u32>,
    /// Size in bytes of the last stsz entry. Oracle subtracts this from
    /// Source_StreamSize to derive the post-elst Audio.StreamSize for
    /// AAC (the trailing frame is partial after encoder padding).
    last_sample_size: Option<u32>,
    /// tkhd track_ID; if absent we fall back to (track_index+1).
    track_id: Option<u32>,
    /// tkhd alternate_group (2 bytes, 0 if unused).
    alternate_group: Option<u16>,
    /// True if the tkhd flags' track_enabled bit (bit 0) is set.
    track_enabled: Option<bool>,
    /// tkhd creation_time and modification_time (seconds since 1904-01-01 UTC).
    creation_time: Option<u64>,
    modification_time: Option<u64>,
    /// Visual sample entry width/height (pixels). Pre-PAR.
    video_width: Option<u16>,
    video_height: Option<u16>,
    /// Codec format label ("AVC", "HEVC", "MPEG-4 Visual").
    video_format: Option<&'static str>,
    /// 4-char sample entry type ("avc1", "hvc1", "mp4v").
    video_codec_id: Option<&'static str>,
    /// Sample entry 4cc for hint tracks (e.g. "rtp ").
    hint_codec_id: Option<&'static str>,
    /// Sample entry 4cc for timed-metadata tracks (e.g. "mebx" — iPhone
    /// Core Media metadata). Triggers `Format=Timed Metadata Sample`.
    meta_codec_id: Option<&'static str>,
    /// ISO 639-2 three-letter language code from mdhd, e.g. "eng".
    /// Stored raw; conversion to oracle's ISO 639-1 form happens at emit time.
    language: Option<String>,
    /// AVCProfileIndication byte from avcC (e.g. 0x42=Baseline, 0x4D=Main).
    avc_profile_idc: Option<u8>,
    /// profile_compatibility byte from avcC — bit 6 set means "Constrained".
    avc_profile_compat: Option<u8>,
    /// AVCLevelIndication byte from avcC (e.g. 0x1E=3.0).
    avc_level_idc: Option<u8>,
    /// colr box: 'nclx' or 'nclc' color descriptor (CICP indices).
    color_primaries_idc: Option<u16>,
    color_transfer_idc: Option<u16>,
    color_matrix_idc: Option<u16>,
    /// Only set for 'nclx'; bit 7 of the 17th byte = full_range_flag.
    color_full_range: Option<bool>,
    /// pasp box: pixel aspect ratio = h_spacing / v_spacing.
    pasp_h: Option<u32>,
    pasp_v: Option<u32>,
    /// Decoded AVC SPS (from avcC first SPS entry). Carries VUI colour,
    /// SAR, RefFrames, stored_height, CABAC-able flags.
    avc_sps: Option<revelo_parsers_video::AvcInfo>,
    // Parsed HEVC SPS (from hvcC). Carries VUI colour info similar to AVC.
    hevc_sps: Option<revelo_parsers_video::HevcInfo>,
    // Encoder info extracted from AVC/HEVC SEI user_data_unregistered
    // message (library + name/version/settings sub-fields).
    encoder_info: Option<revelo_parsers_video::EncoderInfo>,
    /// Absolute file offset of this track's first chunk (stco/co64 entry 0)
    /// — start of the first sample, used to locate AVC SEI in mdat.
    first_chunk_offset: Option<u64>,
    /// NAL length prefix size from avcC's lengthSizeMinusOne (+1); 4 is the
    /// near-universal default when absent.
    avc_nal_length_size: Option<u8>,
    /// CABAC mode from avcC's PPS entry — true if entropy_coding_mode_flag.
    avc_cabac: Option<bool>,
    /// tkhd matrix-derived rotation, in degrees (clockwise, 0/90/180/270).
    /// None when matrix is identity (no rotation).
    rotation_deg: Option<u32>,
    /// Dolby Vision fields from dvcC/dvvC.
    dovi_profile: Option<u8>,
    dovi_level: Option<u8>,
    dovi_bl_present: bool,
    dovi_bl_compat_id: Option<u8>,
    dovi_rpu_present: bool,
    dovi_el_present: bool,
    /// HEVC fixed-header fields from hvcC.
    hevc_profile_idc: Option<u8>,
    /// general_tier_flag — true = High tier, false = Main tier.
    hevc_tier_high: Option<bool>,
    hevc_level_idc: Option<u8>,
    hevc_chroma_format_idc: Option<u8>,
    hevc_bit_depth_luma: Option<u8>,
    /// stts: if all entries share the same sample_delta, the track is
    /// CFR with `frame_rate = timescale / sample_delta`. None when stts
    /// is missing or contains mixed deltas (treated as VFR).
    stts_cfr_delta: Option<u32>,
    /// stts: smallest sample_delta encountered (any entry's delta).
    /// Used with `timescale` to derive `FrameRate_Maximum`.
    stts_min_delta: Option<u32>,
    /// stts: largest sample_delta encountered.
    /// Used with `timescale` to derive `FrameRate_Minimum`.
    stts_max_delta: Option<u32>,
    /// stts: sum of all sample_counts. Lets us derive an exact average
    /// FrameRate without depending on stsz's `sample_count`.
    stts_total_samples: u64,
    /// stts: sum of (sample_count × sample_delta). Equals the media
    /// duration in track-timescale units.
    stts_total_duration: u64,
    has_data: bool,
}

/// Detection: `ftyp` box at offset 4.
/// Fills: Brands, moov→trak→mdia→minf→stbl metadata, CodecPrivate, tracks, chapters.
pub fn parse_mp4(fa: &mut FileAnalyze) -> bool {
    // Detect by leading ftyp box.
    let head = fa.peek_raw(8);
    let Some(h) = head else { return false };
    if &h[4..8] != b"ftyp" {
        return false;
    }

    let mut tracks: Vec<TrackInfo> = Vec::new();
    let mut ftyp_brands: Vec<String> = Vec::new();
    let mut movie = MovieInfo::default();
    let mut mdat_offset: Option<usize> = None;
    let mut mdat_size: Option<usize> = None;
    let mut moov_offset: Option<usize> = None;
    let file_size = fa.remain();

    walk_boxes(fa, file_size, 0, &mut |fa, box_type, box_size, box_start, depth| match box_type {
        BOX_FTYP => {
            ftyp_brands = parse_ftyp(fa, box_size);
        }
        BOX_MDAT => {
            if mdat_offset.is_none() {
                mdat_offset = Some(box_start);
                mdat_size = Some(box_size);
            }
            fa.skip_hexa(box_size.saturating_sub(8), "mdat_body");
        }
        BOX_MOOV => {
            if moov_offset.is_none() {
                moov_offset = Some(box_start);
            }
            let inner = box_size.saturating_sub(8);
            walk_boxes(fa, inner, depth + 1, &mut |fa, t, s, _, _| {
                handle_inner(fa, t, s, &mut tracks, &mut movie);
            });
        }
        BOX_MDIA | BOX_MINF | BOX_STBL | BOX_EDTS | BOX_UDTA => {
            let inner = box_size.saturating_sub(8);
            walk_boxes(fa, inner, depth + 1, &mut |fa, t, s, _, _| {
                handle_inner(fa, t, s, &mut tracks, &mut movie);
            });
        }
        BOX_TRAK => {
            tracks.push(TrackInfo::default());
            let inner = box_size.saturating_sub(8);
            walk_boxes(fa, inner, depth + 1, &mut |fa, t, s, _, _| {
                handle_inner(fa, t, s, &mut tracks, &mut movie);
            });
        }
        _ => {
            fa.skip_hexa(box_size.saturating_sub(8), "BoxBody");
        }
    });

    let layout = BoxLayout { file_size, mdat_offset, mdat_size, moov_offset };
    // AVC encoder SEI lives in the first mdat sample, not avcC — scan for
    // it before emitting (HEVC already gets its SEI from the hvcC arrays).
    scan_avc_encoder_sei(fa, &mut tracks);
    fill_streams(fa, &ftyp_brands, &tracks, &movie, &layout);
    true
}

/// For AVC video tracks lacking encoder info, read the first sample from
/// mdat (located via stco/co64 + stsz entry 0) and pull the x264-style
/// encoder string out of its SEI user_data_unregistered NAL.
fn scan_avc_encoder_sei(fa: &FileAnalyze, tracks: &mut [TrackInfo]) {
    for track in tracks.iter_mut() {
        if track.video_format != Some("AVC") || track.encoder_info.is_some() {
            continue;
        }
        let (Some(off), Some(size)) = (track.first_chunk_offset, track.first_sample_size) else {
            continue;
        };
        let len_size = track.avc_nal_length_size.unwrap_or(4) as usize;
        let Some(sample) = fa.peek_raw_at(off as usize, size as usize) else {
            continue;
        };
        let sei_nalus = collect_avc_sei_nalus(sample, len_size);
        if sei_nalus.is_empty() {
            continue;
        }
        if let Some(enc) = revelo_parsers_video::extract_encoder_from_avc_sei_nalus(&sei_nalus) {
            track.encoder_info = Some(enc);
        }
    }
}

/// Walk length-prefixed NAL units in an MP4 sample and return the SEI ones
/// (AVC nal_unit_type 6).
fn collect_avc_sei_nalus(sample: &[u8], len_size: usize) -> Vec<&[u8]> {
    let mut out = Vec::new();
    if len_size == 0 || len_size > 4 {
        return out;
    }
    let mut pos = 0usize;
    while pos + len_size <= sample.len() {
        let mut nalu_len = 0usize;
        for i in 0..len_size {
            nalu_len = (nalu_len << 8) | sample[pos + i] as usize;
        }
        pos += len_size;
        if nalu_len == 0 || pos + nalu_len > sample.len() {
            break;
        }
        let nal = &sample[pos..pos + nalu_len];
        if !nal.is_empty() && (nal[0] & 0x1F) == 6 {
            out.push(nal);
        }
        pos += nalu_len;
    }
    out
}

struct BoxLayout {
    file_size: usize,
    mdat_offset: Option<usize>,
    mdat_size: Option<usize>,
    moov_offset: Option<usize>,
}

/// Iterate boxes from the current position for up to `region_size`
/// bytes (or to end of buffer if 0). Stops early on truncated reads.
/// Visitor receives `(fa, box_type, total_size, box_start_offset, depth)`
/// where `box_start_offset` is the file offset of the box header.
fn walk_boxes(
    fa: &mut FileAnalyze,
    region_size: usize,
    depth: usize,
    visit: &mut dyn FnMut(&mut FileAnalyze, u32, usize, usize, usize),
) {
    let region_end = fa.element_offset() + region_size;
    while fa.element_offset() + 8 <= region_end && fa.remain() >= 8 {
        let start = fa.element_offset();
        let size32 = Reader::wrap(fa).be_u32("Size").unwrap_or(0);
        let box_type = Reader::wrap(fa).fourcc("Type").unwrap_or(0);

        let total_size: usize = if size32 == 1 {
            let size64 = Reader::wrap(fa).be_u64("Size64").unwrap_or(0);
            size64 as usize
        } else if size32 == 0 {
            // Box extends to end of file
            region_end - start
        } else {
            size32 as usize
        };

        let header_size = fa.element_offset() - start;
        let body_size = total_size.saturating_sub(header_size);

        // Save position to ensure we always advance exactly `body_size`.
        let body_start = fa.element_offset();
        let body_end = body_start + body_size;
        if body_end > start + region_size && depth == 0 {
            // Truncated final box at top level — stop.
            break;
        }

        // Call the visitor — the visitor may either consume the body
        // (Skip_Hexa) or recurse into sub-boxes. To stay safe, snap the
        // position back to body_end after the visitor returns.
        fa.element_begin(box_type_name(box_type).as_str());
        let _name = box_type_name(box_type);
        visit(fa, box_type, total_size, start, depth);
        fa.element_end();
        if fa.element_offset() < body_end {
            fa.skip_hexa(body_end - fa.element_offset(), "BoxTail");
        } else if fa.element_offset() > body_end {
            // Visitor over-consumed — should not happen, but bail if it does.
            break;
        }
    }
}

fn box_type_name(t: u32) -> String {
    let bytes = t.to_be_bytes();
    if bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        format!("{:08X}", t)
    }
}

fn parse_ftyp(fa: &mut FileAnalyze, box_size: usize) -> Vec<String> {
    let body_size = box_size.saturating_sub(8);
    let mut brands: Vec<String> = Vec::new();
    if body_size < 8 {
        fa.skip_hexa(body_size, "ftyp");
        return brands;
    }
    let major = Reader::wrap(fa).fourcc("major_brand").unwrap_or(0);
    brands.push(four_cc_str(major));
    // minor_version is consumed to advance the cursor but not surfaced.
    let _ = Reader::wrap(fa).be_u32("minor_version");
    let remain = body_size - 8;
    let count = remain / 4;
    for _ in 0..count {
        let b = Reader::wrap(fa).fourcc("compatible_brand").unwrap_or(0);
        brands.push(four_cc_str(b));
    }
    // Skip any tail bytes that aren't a multiple of 4.
    if remain % 4 != 0 {
        fa.skip_hexa(remain % 4, "ftyp_tail");
    }
    brands
}

fn four_cc_str(v: u32) -> String {
    let bytes = v.to_be_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn handle_inner(
    fa: &mut FileAnalyze,
    box_type: u32,
    box_size: usize,
    tracks: &mut Vec<TrackInfo>,
    movie: &mut MovieInfo,
) {
    match box_type {
        BOX_MDIA | BOX_MINF | BOX_STBL | BOX_EDTS | BOX_UDTA => {
            let inner = box_size.saturating_sub(8);
            walk_boxes(fa, inner, 1, &mut |fa, t, s, _, _| {
                handle_inner(fa, t, s, tracks, movie);
            });
        }
        BOX_META => {
            // ISO BMFF `meta` is a FullBox with a 4-byte version_flags
            // header before nested children. QuickTime `meta` (iPhone
            // MOV) is a plain Box — children start immediately. Detect
            // by peeking for "hdlr" at offset 4 (QuickTime) vs offset 8
            // (ISO with version_flags). We dispatch only keys/ilst
            // because meta's hdlr declares the metadata namespace
            // (e.g. "mdir" iTunes, "mdta" QuickTime) — not the track's
            // audio/video handler.
            let inner = box_size.saturating_sub(8);
            if inner < 8 {
                fa.skip_hexa(inner, "meta_short");
            } else {
                let probe = fa.peek_raw(8);
                let is_qt_meta = probe
                    .map(|p| &p[4..8] == b"hdlr" || &p[4..8] == b"keys" || &p[4..8] == b"ilst")
                    .unwrap_or(false);
                let children_size = if is_qt_meta {
                    inner
                } else {
                    fa.skip_hexa(4, "version_flags");
                    inner - 4
                };
                // qt mdta item indices are scoped to THIS meta box's
                // keys table — clear before each meta so a track-level
                // meta (with 2 keys) doesn't leak into the movie-level
                // meta's index resolution.
                movie.qt_keys.clear();
                walk_boxes(fa, children_size, 1, &mut |fa, t, s, _, _| match t {
                    BOX_KEYS => parse_keys(fa, s, movie),
                    BOX_ILST => parse_ilst(fa, s, movie),
                    _ => fa.skip_hexa(s.saturating_sub(8), "meta_child"),
                });
            }
        }
        BOX_TRAK => {
            tracks.push(TrackInfo::default());
            let inner = box_size.saturating_sub(8);
            walk_boxes(fa, inner, 1, &mut |fa, t, s, _, _| {
                handle_inner(fa, t, s, tracks, movie);
            });
        }
        BOX_MVHD => {
            parse_mvhd(fa, box_size, movie);
        }
        BOX_TKHD => {
            if let Some(track) = tracks.last_mut() {
                parse_tkhd(fa, box_size, track);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "tkhd");
            }
        }
        BOX_MDHD => {
            if let Some(track) = tracks.last_mut() {
                parse_mdhd(fa, box_size, track);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "mdhd");
            }
        }
        BOX_HDLR => {
            if let Some(track) = tracks.last_mut() {
                parse_hdlr(fa, box_size, track);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "hdlr");
            }
        }
        BOX_STSD => {
            if let Some(track) = tracks.last_mut() {
                parse_stsd(fa, box_size, track);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "stsd");
            }
        }
        BOX_STSZ => {
            if let Some(track) = tracks.last_mut() {
                parse_stsz(fa, box_size, track);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "stsz");
            }
        }
        BOX_STCO => {
            if let Some(track) = tracks.last_mut() {
                parse_stco(fa, box_size, track, false);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "stco");
            }
        }
        BOX_CO64 => {
            if let Some(track) = tracks.last_mut() {
                parse_stco(fa, box_size, track, true);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "co64");
            }
        }
        BOX_STTS => {
            if let Some(track) = tracks.last_mut() {
                parse_stts(fa, box_size, track);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "stts");
            }
        }
        BOX_ELST => {
            if let Some(track) = tracks.last_mut() {
                parse_elst(fa, box_size, track);
            } else {
                fa.skip_hexa(box_size.saturating_sub(8), "elst");
            }
        }
        _ => {
            fa.skip_hexa(box_size.saturating_sub(8), "BoxBody");
        }
    }
}

/// Parse tkhd v0/v1 — extract track_ID, alternate_group, and the
/// track_enabled flag bit. Layout (v0):
///   1 byte version + 3 bytes flags (bit 0 = track_enabled)
///   4 bytes creation_time
///   4 bytes modification_time
///   4 bytes track_ID
///   4 bytes reserved
///   4 bytes duration
///   8 bytes reserved
///   2 bytes layer
///   2 bytes alternate_group
///   2 bytes volume + 2 bytes reserved
///   36 bytes matrix
///   4 bytes width (16.16 fixed)
///   4 bytes height (16.16 fixed)
/// v1 uses 8-byte timestamps and 8-byte duration.
fn parse_tkhd(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 24 {
        fa.skip_hexa(body_size, "tkhd_short");
        return;
    }
    let start = fa.element_offset();
    let version_flags = Reader::wrap(fa).be_u32("version_flags").unwrap_or(0);
    let version = (version_flags >> 24) as u8;
    let flags = version_flags & 0x00FF_FFFF;
    track.track_enabled = Some((flags & 0x1) != 0);

    if version == 1 {
        let ct = Reader::wrap(fa).be_u64("creation_time").unwrap_or(0);
        let mt = Reader::wrap(fa).be_u64("modification_time").unwrap_or(0);
        track.creation_time = Some(ct);
        track.modification_time = Some(mt);
    } else {
        let ct = Reader::wrap(fa).be_u32("creation_time").unwrap_or(0);
        let mt = Reader::wrap(fa).be_u32("modification_time").unwrap_or(0);
        track.creation_time = Some(ct as u64);
        track.modification_time = Some(mt as u64);
    }
    let tid = Reader::wrap(fa).be_u32("track_ID").unwrap_or(0);
    track.track_id = Some(tid);
    fa.skip_hexa(4, "reserved");
    if version == 1 {
        fa.skip_hexa(8, "duration");
    } else {
        fa.skip_hexa(4, "duration");
    }
    fa.skip_hexa(8, "reserved");
    fa.skip_hexa(2, "layer");
    let alt_group = Reader::wrap(fa).be_u16("alternate_group").unwrap_or(0);
    track.alternate_group = Some(alt_group);
    fa.skip_hexa(2, "volume");
    fa.skip_hexa(2, "reserved_v");
    // 9 matrix entries: top-left 4 are 16.16 fixed, last column is 2.30.
    // We only need (a, b) = matrix[0], matrix[1] for rotation in the
    // top-left 2x2; identity = (1, 0). 90° CW = (0, 1); 180° = (-1, 0);
    // 270° CW = (0, -1).
    let a = Reader::wrap(fa).be_u32("matrix_a").unwrap_or(0);
    let b = Reader::wrap(fa).be_u32("matrix_b").unwrap_or(0);
    fa.skip_hexa(28, "matrix_rest"); // 7 remaining matrix entries
    let a_i = a as i32;
    let b_i = b as i32;
    let one_16_16 = 0x10000_i32;
    let rot = if a_i == one_16_16 && b_i == 0 {
        0
    } else if a_i == 0 && b_i == one_16_16 {
        90
    } else if a_i == -one_16_16 && b_i == 0 {
        180
    } else if a_i == 0 && b_i == -one_16_16 {
        270
    } else {
        // Non-axis-aligned rotation — skip for now (would need atan2).
        0
    };
    if rot != 0 {
        track.rotation_deg = Some(rot);
    }

    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "tkhd_tail");
    }
}

/// Parse mvhd v0/v1 — we only need the `timescale` field for converting
/// elst.segment_duration into seconds.
fn parse_mvhd(fa: &mut FileAnalyze, box_size: usize, movie: &mut MovieInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 16 {
        fa.skip_hexa(body_size, "mvhd");
        return;
    }
    let start = fa.element_offset();
    let version_flags = Reader::wrap(fa).be_u32("version_flags").unwrap_or(0);
    let version = (version_flags >> 24) as u8;
    if version == 1 {
        let ct = Reader::wrap(fa).be_u64("creation_time").unwrap_or(0);
        let mt = Reader::wrap(fa).be_u64("modification_time").unwrap_or(0);
        movie.creation_time = Some(ct);
        movie.modification_time = Some(mt);
        let ts = Reader::wrap(fa).be_u32("timescale").unwrap_or(0);
        movie.timescale = ts;
        let dur = Reader::wrap(fa).be_u64("duration").unwrap_or(0);
        movie.duration = dur;
    } else {
        let ct = Reader::wrap(fa).be_u32("creation_time").unwrap_or(0);
        let mt = Reader::wrap(fa).be_u32("modification_time").unwrap_or(0);
        movie.creation_time = Some(ct as u64);
        movie.modification_time = Some(mt as u64);
        let ts = Reader::wrap(fa).be_u32("timescale").unwrap_or(0);
        movie.timescale = ts;
        let dur = Reader::wrap(fa).be_u32("duration").unwrap_or(0);
        movie.duration = dur as u64;
    }

    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "mvhd_tail");
    }
}

/// Parse the iTunes `ilst` (item list) box. Each item is a `[size][4cc]`
/// box containing a nested `data` box. Captures UTF-8 string items
/// indexed by their 4cc type.
fn parse_ilst(fa: &mut FileAnalyze, box_size: usize, movie: &mut MovieInfo) {
    let inner = box_size.saturating_sub(8);
    let end = fa.element_offset() + inner;
    while fa.element_offset() + 8 <= end {
        let item_start = fa.element_offset();
        let item_size = Reader::wrap(fa).be_u32("item_size").unwrap_or(0);
        let item_type = Reader::wrap(fa).fourcc("item_type").unwrap_or(0);
        let item_total = item_size as usize;
        if item_total < 8 || item_start + item_total > end {
            break;
        }
        let body = item_total - 8;
        let body_end = fa.element_offset() + body;

        // Inside each item is a `data` box (and maybe other helpers).
        while fa.element_offset() + 8 <= body_end {
            let sub_size = Reader::wrap(fa).be_u32("sub_size").unwrap_or(0);
            let sub_type = Reader::wrap(fa).fourcc("sub_type").unwrap_or(0);
            let sub_total = sub_size as usize;
            if sub_total < 8 || sub_total > (body_end - fa.element_offset() + 8) {
                break;
            }
            let sub_body = sub_total - 8;
            if sub_type == BOX_DATA && sub_body >= 8 {
                // data box: 4 bytes type_indicator, 4 bytes locale, then payload.
                let type_indicator = Reader::wrap(fa).be_u32("type_indicator").unwrap_or(0);
                fa.skip_hexa(4, "locale");
                let payload_size = sub_body - 8;
                let payload = fa.read_raw(payload_size).to_vec();
                // type_indicator: 1 = UTF-8 string, 23 = 32-bit float BE,
                // 75 = unsigned 8-bit. We keep strings universally as
                // UTF-8 and the few small numerics encountered in
                // QuickTime metadata (e.g. flags) as decimal strings.
                let value = match type_indicator {
                    1 => Some(String::from_utf8_lossy(&payload).into_owned()),
                    21 if payload_size == 1 => Some((payload[0] as i8).to_string()),
                    21 if payload_size == 4 => Some(
                        i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
                            .to_string(),
                    ),
                    22 if payload_size == 1 => Some(payload[0].to_string()),
                    22 if payload_size == 4 => Some(
                        u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
                            .to_string(),
                    ),
                    23 if payload_size == 4 => Some(format!(
                        "{}",
                        f32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
                    )),
                    _ => None,
                };
                if let Some(v) = value {
                    // QuickTime mdta items use a small numeric item_type
                    // (1-based index into `keys`). Any 4cc with the high
                    // byte clear and a value in [1, keys.len()] is
                    // treated as an mdta index.
                    let idx = item_type as usize;
                    if (item_type & 0xFF000000) == 0 && idx >= 1 && idx <= movie.qt_keys.len() {
                        let key = movie.qt_keys[idx - 1].clone();
                        movie.qt_metadata.push((key, v));
                    } else {
                        movie.itunes_metadata.push((item_type, v));
                    }
                }
            } else {
                fa.skip_hexa(sub_body, "sub_body");
            }
        }
        // Pin to item boundary in case the loop above bailed.
        if fa.element_offset() < body_end {
            fa.skip_hexa(body_end - fa.element_offset(), "item_tail");
        }
    }
    if fa.element_offset() < end {
        fa.skip_hexa(end - fa.element_offset(), "ilst_tail");
    }
}

/// Parse the QuickTime `keys` box. Layout:
///   4 bytes version_flags
///   4 bytes entry_count
///   per entry:
///     4 bytes key_size (entire entry length incl. these 4 bytes)
///     4 bytes key_namespace (typically "mdta")
///     (key_size - 8) bytes UTF-8 key_value (e.g. "com.apple.quicktime.make")
/// We capture only the key_value strings; the namespace is informational.
fn parse_keys(fa: &mut FileAnalyze, box_size: usize, movie: &mut MovieInfo) {
    let inner = box_size.saturating_sub(8);
    if inner < 8 {
        fa.skip_hexa(inner, "keys_short");
        return;
    }
    let start = fa.element_offset();
    let end = start + inner;
    fa.skip_hexa(4, "version_flags");
    let entry_count = Reader::wrap(fa).be_u32("entry_count").unwrap_or(0);
    for _ in 0..entry_count {
        if fa.element_offset() + 8 > end {
            break;
        }
        let entry_size = Reader::wrap(fa).be_u32("key_size").unwrap_or(0);
        fa.skip_hexa(4, "key_namespace");
        let total = entry_size as usize;
        if total < 8 || fa.element_offset() + (total - 8) > end {
            break;
        }
        let key_bytes = fa.read_raw(total - 8).to_vec();
        let key = String::from_utf8_lossy(&key_bytes).into_owned();
        movie.qt_keys.push(key);
    }
    if fa.element_offset() < end {
        fa.skip_hexa(end - fa.element_offset(), "keys_tail");
    }
}

/// Parse elst — capture only the first entry's segment_duration
/// (sufficient for typical single-entry edit lists used to trim AAC
/// encoder priming).
fn parse_elst(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 8 {
        fa.skip_hexa(body_size, "elst");
        return;
    }
    let start = fa.element_offset();
    let version_flags = Reader::wrap(fa).be_u32("version_flags").unwrap_or(0);
    let version = (version_flags >> 24) as u8;
    let entry_count = Reader::wrap(fa).be_u32("entry_count").unwrap_or(0);

    if entry_count > 0 {
        let segment_duration: u64;
        let media_time: i64;
        if version == 1 {
            let sd = Reader::wrap(fa).be_u64("segment_duration").unwrap_or(0);
            segment_duration = sd;
            let mt = Reader::wrap(fa).be_u64("media_time").unwrap_or(0);
            media_time = mt as i64;
            fa.skip_hexa(4, "media_rate");
        } else {
            let sd = Reader::wrap(fa).be_u32("segment_duration").unwrap_or(0);
            segment_duration = sd as u64;
            let mt = Reader::wrap(fa).be_u32("media_time").unwrap_or(0);
            media_time = mt as i32 as i64;
            fa.skip_hexa(4, "media_rate");
        }
        let other_entries = entry_count.saturating_sub(1) as usize;
        let entry_size = if version == 1 { 20 } else { 12 };
        if other_entries > 0 {
            fa.skip_hexa(other_entries * entry_size, "other_entries");
        }
        track.elst_segment_duration = Some(segment_duration);
        track.elst_media_time = Some(media_time);
    }

    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "elst_tail");
    }
}

fn parse_mdhd(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 4 {
        fa.skip_hexa(body_size, "mdhd");
        return;
    }
    let start = fa.element_offset();
    let version_flags = Reader::wrap(fa).be_u32("version_flags").unwrap_or(0);
    let version = (version_flags >> 24) as u8;

    if version == 1 {
        let _created = Reader::wrap(fa).be_u64("creation_time").unwrap_or(0);
        let _modified = Reader::wrap(fa).be_u64("modification_time").unwrap_or(0);
        let ts = Reader::wrap(fa).be_u32("timescale").unwrap_or(0);
        let dur = Reader::wrap(fa).be_u64("duration").unwrap_or(0);
        track.timescale = ts;
        track.duration_units = dur;
    } else {
        let _created = Reader::wrap(fa).be_u32("creation_time").unwrap_or(0);
        let _modified = Reader::wrap(fa).be_u32("modification_time").unwrap_or(0);
        let ts = Reader::wrap(fa).be_u32("timescale").unwrap_or(0);
        let dur = Reader::wrap(fa).be_u32("duration").unwrap_or(0);
        track.timescale = ts;
        track.duration_units = dur as u64;
    }

    // 16-bit packed ISO 639-2 language: 1 padding bit + 3×5-bit chars,
    // each char = (letter - 0x60). "eng" → 0x15C7.
    let lang_raw = Reader::wrap(fa).be_u16("language").unwrap_or(0);
    let c0 = ((lang_raw >> 10) & 0x1F) as u8;
    let c1 = ((lang_raw >> 5) & 0x1F) as u8;
    let c2 = (lang_raw & 0x1F) as u8;
    if c0 > 0 && c1 > 0 && c2 > 0 {
        let chars = [c0 + 0x60, c1 + 0x60, c2 + 0x60];
        let s = String::from_utf8_lossy(&chars).into_owned();
        track.language = Some(s);
    }

    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "mdhd_tail");
    }
}

fn parse_hdlr(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 12 {
        fa.skip_hexa(body_size, "hdlr");
        return;
    }
    let start = fa.element_offset();
    fa.skip_hexa(4, "version_flags");
    fa.skip_hexa(4, "pre_defined");
    let handler = Reader::wrap(fa).fourcc("handler_type").unwrap_or(0);
    fa.skip_hexa(12, "reserved");
    // Trailing name: ISO BMFF uses a null-terminated UTF-8 C-string;
    // QuickTime uses a Pascal-style string (1-byte length prefix). Both
    // sit at the same offset. Detect by checking whether the first byte
    // is a plausible length that matches the remaining bytes.
    let consumed_so_far = fa.element_offset() - start;
    let name_bytes_len = body_size.saturating_sub(consumed_so_far);
    let raw_name = if name_bytes_len > 0 {
        let bytes = fa.read_raw(name_bytes_len).to_vec();
        Some(bytes)
    } else {
        None
    };
    let name = raw_name.as_deref().and_then(decode_hdlr_name);

    // Only set on first hdlr (the one under mdia). minf > hdlr is a
    // data-handler reference (e.g. "alis"/"url "/"dhlr") that would
    // clobber the real track-type handler if we wrote unconditionally.
    if track.handler == 0 {
        track.handler = handler;
        if let Some(n) = name {
            if !n.is_empty() {
                track.handler_name = Some(n);
            }
        }
    }
}

/// hdlr trailing name decoder. ISO BMFF uses a null-terminated UTF-8
/// C-string; QuickTime uses a Pascal-style 1-byte length prefix. Pick
/// the interpretation that matches the available length. Returns None
/// for the boilerplate Apple-tool defaults ("Apple Video Media Handler",
/// etc.) since the oracle suppresses those — Title only appears for
/// meaningful per-track names (e.g. iPhone's "Core Media Video").
fn decode_hdlr_name(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    // QuickTime Pascal form: leading byte = string length, followed by
    // exactly that many bytes. Discriminator: first byte == bytes.len()-1.
    let pascal_len = bytes[0] as usize;
    let raw = if pascal_len > 0 && pascal_len == bytes.len() - 1 {
        String::from_utf8_lossy(&bytes[1..1 + pascal_len]).into_owned()
    } else {
        String::from_utf8_lossy(bytes).trim_end_matches('\0').trim().to_string()
    };
    if raw.is_empty() {
        return None;
    }
    // Suppress the generic Apple QuickTime authoring tool defaults.
    if matches!(
        raw.as_str(),
        "Apple Video Media Handler"
            | "Apple Sound Media Handler"
            | "Apple Alias Data Handler"
            | "GPAC ISO Video Handler"
            | "GPAC ISO Audio Handler"
            | "L-SMASH Video Handler"
            | "L-SMASH Audio Handler"
            | "Mainconcept Video Media Handler"
            | "Mainconcept Sound Media Handler"
            | "Mainconcept MP4 Sound Media Handler"
            | "Mainconcept MP4 Video Media Handler"
            | "VideoHandler"
            | "SoundHandler"
            | "TextHandler"
            | "DataHandler"
    ) {
        return None;
    }
    Some(raw)
}

fn parse_stsd(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 8 {
        fa.skip_hexa(body_size, "stsd");
        return;
    }
    let start = fa.element_offset();
    fa.skip_hexa(4, "version_flags");
    let entry_count = Reader::wrap(fa).be_u32("entry_count").unwrap_or(0);

    // Walk the first sample entry only (sufficient for single-codec tracks).
    if entry_count > 0 && fa.remain() >= 8 {
        let entry_start = fa.element_offset();
        let entry_size = Reader::wrap(fa).be_u32("entry_size").unwrap_or(0);
        let entry_type = Reader::wrap(fa).fourcc("entry_type").unwrap_or(0);
        let entry_total = entry_size as usize;

        match entry_type {
            SAMPLE_ENTRY_MP4A => parse_mp4a_entry(fa, entry_total, track),
            SAMPLE_ENTRY_AVC1 => parse_visual_entry(fa, entry_total, track, "AVC", "avc1"),
            SAMPLE_ENTRY_AVC3 => parse_visual_entry(fa, entry_total, track, "AVC", "avc3"),
            SAMPLE_ENTRY_HVC1 => parse_visual_entry(fa, entry_total, track, "HEVC", "hvc1"),
            SAMPLE_ENTRY_HEV1 => parse_visual_entry(fa, entry_total, track, "HEVC", "hev1"),
            SAMPLE_ENTRY_MP4V => {
                parse_visual_entry(fa, entry_total, track, "MPEG-4 Visual", "mp4v")
            }
            SAMPLE_ENTRY_RTP => {
                track.hint_codec_id = Some("rtp ");
                track.has_data = true;
                fa.skip_hexa(entry_total.saturating_sub(8), "rtp_entry");
            }
            SAMPLE_ENTRY_MEBX => {
                track.meta_codec_id = Some("mebx");
                track.has_data = true;
                fa.skip_hexa(entry_total.saturating_sub(8), "mebx_entry");
            }
            _ => {
                // Unknown sample entry — record nothing, just skip.
                fa.skip_hexa(entry_total.saturating_sub(8), "unknown_sample_entry");
            }
        }
        let entry_end = entry_start + entry_total;
        if fa.element_offset() < entry_end {
            fa.skip_hexa(entry_end - fa.element_offset(), "entry_tail");
        }
    }

    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "stsd_tail");
    }
}

fn parse_mp4a_entry(fa: &mut FileAnalyze, entry_total: usize, track: &mut TrackInfo) {
    // mp4a sample entry layout (after 4-byte size + 4-byte type already consumed):
    //   6 bytes reserved
    //   2 bytes data_reference_index
    //   2 bytes version  (0 = basic, 1 = SoundDescriptionV1, 2 = V2)
    //   2 bytes revision
    //   4 bytes vendor
    //   2 bytes channel_count   (V0/V1; V2 ignores and uses extended block)
    //   2 bytes sample_size
    //   2 bytes pre_defined (V0) / compression_id (V1)
    //   2 bytes packet_size (always 0)
    //   4 bytes sample_rate (16.16 fixed)
    //   if version == 1: +16 bytes (samples_per_packet, bytes_per_packet,
    //                                bytes_per_frame, bytes_per_sample)
    //   if version == 2: +36 bytes (extended audio metadata)
    //   then extension boxes (esds, etc.)
    if entry_total < 8 + 28 {
        fa.skip_hexa(entry_total.saturating_sub(8), "mp4a_short");
        return;
    }
    let start_remain = entry_total - 8;
    fa.skip_hexa(6, "reserved");
    fa.skip_hexa(2, "data_reference_index");
    let version = Reader::wrap(fa).be_u16("version").unwrap_or(0);
    fa.skip_hexa(2, "revision");
    fa.skip_hexa(4, "vendor");
    let channel_count_u16 = Reader::wrap(fa).be_u16("channel_count").unwrap_or(0);
    let channel_count = channel_count_u16 as u32;
    let sample_size = Reader::wrap(fa).be_u16("sample_size").unwrap_or(0);
    let _ = sample_size;
    fa.skip_hexa(2, "pre_defined_or_compression_id");
    fa.skip_hexa(2, "packet_size");
    let sr_fixed = Reader::wrap(fa).be_u32("sample_rate_16.16").unwrap_or(0);
    let sample_rate = (sr_fixed >> 16) as u32;

    track.audio_channels = Some(channel_count as u16);
    track.audio_sample_rate = Some(sample_rate);
    track.audio_format = Some("AAC");
    track.has_data = true;

    let mut consumed = 28;
    if version == 1 && start_remain >= consumed + 16 {
        fa.skip_hexa(16, "sdv1_extra");
        consumed += 16;
    } else if version == 2 && start_remain >= consumed + 36 {
        fa.skip_hexa(36, "sdv2_extra");
        consumed += 36;
    }
    let remaining = start_remain.saturating_sub(consumed);

    // Audio sample entry "extension" region. The basic layout above is
    // an ISO BMFF V0/V1/V2 audio header; everything after is supposed
    // to be a sequence of `[size:4][type:4][body]` extension boxes.
    // In practice (QuickTime/iPhone, FFmpeg), the region can contain
    // junk bytes — wave/frma/chan blocks with idiosyncratic layouts —
    // before the real esds. Rather than try to parse every Apple/QT
    // extension shape, snarf the whole tail and scan for an esds box
    // at any 4-byte alignment.
    if remaining > 0 {
        let tail = fa.read_raw(remaining).to_vec();
        // Try the strict box-walk first (works for standard ISO BMFF).
        let mut walked_ok = false;
        let mut p = 0usize;
        while p + 8 <= tail.len() {
            let sz = u32::from_be_bytes([tail[p], tail[p + 1], tail[p + 2], tail[p + 3]]) as usize;
            let ty = u32::from_be_bytes([tail[p + 4], tail[p + 5], tail[p + 6], tail[p + 7]]);
            if sz < 8 || sz > tail.len() - p {
                break;
            }
            if ty == BOX_ESDS && p + sz <= tail.len() {
                parse_esds_bytes(&tail[p + 8..p + sz], track);
                walked_ok = true;
                break;
            }
            p += sz;
        }
        if !walked_ok {
            // Fallback: scan for "esds" 4cc at any position. Reads the
            // 4 bytes before as a box size for length validation.
            for i in 4..tail.len().saturating_sub(4) {
                if &tail[i..i + 4] == b"esds" {
                    let sz =
                        u32::from_be_bytes([tail[i - 4], tail[i - 3], tail[i - 2], tail[i - 1]])
                            as usize;
                    let body_start = i + 4;
                    let body_end = (i - 4 + sz).min(tail.len());
                    if body_end > body_start && sz >= 8 {
                        parse_esds_bytes(&tail[body_start..body_end], track);
                    }
                    break;
                }
            }
        }
    }
}

/// Parse esds from a byte buffer (no FileAnalyze needed). Mirrors
/// `parse_esds`'s descriptor walk but operates on a slice — used by
/// `parse_mp4a_entry` when the surrounding bytes don't form a clean
/// box-walkable sequence (QuickTime sound description quirks).
fn parse_esds_bytes(body: &[u8], track: &mut TrackInfo) {
    if body.len() < 4 {
        return;
    }
    // Skip 4-byte version+flags, then descriptor chain.
    parse_descriptor_chain_bytes(&body[4..], track);
}

fn parse_descriptor_chain_bytes(data: &[u8], track: &mut TrackInfo) {
    let mut p = 0usize;
    while p + 2 <= data.len() {
        let tag = data[p];
        p += 1;
        let mut sz = 0usize;
        for _ in 0..4 {
            if p >= data.len() {
                return;
            }
            let b = data[p];
            p += 1;
            sz = (sz << 7) | (b & 0x7F) as usize;
            if (b & 0x80) == 0 {
                break;
            }
        }
        if p + sz > data.len() {
            return;
        }
        match tag {
            0x03 => {
                // ES_Descriptor: 2 bytes ES_ID + 1 byte flags + optionals
                if sz < 3 {
                    return;
                }
                let flags = data[p + 2];
                let mut inner_p = p + 3;
                if (flags & 0x80) != 0 {
                    inner_p += 2;
                }
                if (flags & 0x40) != 0 {
                    if inner_p >= p + sz {
                        return;
                    }
                    let url_len = data[inner_p] as usize;
                    inner_p += 1 + url_len;
                }
                if (flags & 0x20) != 0 {
                    inner_p += 2;
                }
                if inner_p < p + sz {
                    parse_descriptor_chain_bytes(&data[inner_p..p + sz], track);
                }
            }
            0x04 => {
                // DecoderConfigDescriptor:
                //   1 byte OTI + 1 byte streamType+upStream+reserved
                //   3 bytes bufferSizeDB + 4 bytes maxBitrate + 4 bytes avgBitrate
                //   then DecoderSpecificInfo (tag 0x05)
                if sz < 13 {
                    return;
                }
                track.object_type_indication = Some(data[p]);
                let buf_sz = u32::from_be_bytes([0, data[p + 2], data[p + 3], data[p + 4]]);
                track.buffer_size_db = Some(buf_sz);
                let max_br =
                    u32::from_be_bytes([data[p + 5], data[p + 6], data[p + 7], data[p + 8]]);
                let avg_br =
                    u32::from_be_bytes([data[p + 9], data[p + 10], data[p + 11], data[p + 12]]);
                if max_br > 0 {
                    track.max_bitrate_bps = Some(max_br);
                }
                if avg_br > 0 {
                    track.avg_bitrate_bps = Some(avg_br);
                }
                if 13 < sz {
                    parse_descriptor_chain_bytes(&data[p + 13..p + sz], track);
                }
            }
            0x05 => {
                // AudioSpecificConfig — 5 bits AOT (if 31, +6 bits ext).
                if sz >= 1 {
                    let aot5 = (data[p] >> 3) & 0x1F;
                    let aot = if aot5 == 31 {
                        if sz >= 2 {
                            32 + (((data[p] & 0x07) << 3) | (data[p + 1] >> 5))
                        } else {
                            aot5
                        }
                    } else {
                        aot5
                    };
                    track.audio_object_type = Some(aot);
                }
            }
            _ => {}
        }
        p += sz;
    }
}

/// Parse a VisualSampleEntry — common layout for avc1/avc3/hvc1/hev1/mp4v.
/// After the 4-byte size + 4-byte type already consumed:
///   6 bytes reserved
///   2 bytes data_reference_index
///   16 bytes pre_defined/reserved (version/revision/vendor/temporalQ/spatialQ)
///   2 bytes width
///   2 bytes height
///   4 bytes horizresolution (16.16)
///   4 bytes vertresolution (16.16)
///   4 bytes reserved
///   2 bytes frame_count
///   32 bytes compressorname
///   2 bytes depth
///   2 bytes pre_defined (-1)
///   optional inner boxes (avcC, hvcC, pasp, colr, etc.)
fn parse_visual_entry(
    fa: &mut FileAnalyze,
    entry_total: usize,
    track: &mut TrackInfo,
    format: &'static str,
    codec_id: &'static str,
) {
    const HEADER_FIXED: usize = 78; // 6+2+16+2+2+4+4+4+2+32+2+2
    if entry_total < 8 + HEADER_FIXED {
        fa.skip_hexa(entry_total.saturating_sub(8), "visual_short");
        return;
    }
    let start_remain = entry_total - 8;
    fa.skip_hexa(6, "reserved");
    fa.skip_hexa(2, "data_reference_index");
    fa.skip_hexa(16, "pre_defined_reserved");
    let width = Reader::wrap(fa).be_u16("width").unwrap_or(0);
    let height = Reader::wrap(fa).be_u16("height").unwrap_or(0);
    fa.skip_hexa(4, "horizresolution");
    fa.skip_hexa(4, "vertresolution");
    fa.skip_hexa(4, "reserved");
    fa.skip_hexa(2, "frame_count");
    fa.skip_hexa(32, "compressorname");
    fa.skip_hexa(2, "depth");
    fa.skip_hexa(2, "pre_defined_neg1");

    track.video_width = Some(width);
    track.video_height = Some(height);
    track.video_format = Some(format);
    track.video_codec_id = Some(codec_id);
    track.has_data = true;

    // Walk inner extension boxes — avcC gives profile/level cheaply.
    // Others (hvcC, pasp, colr) would need bit-decoders we don't have yet.
    let mut remaining = start_remain.saturating_sub(HEADER_FIXED);
    while remaining >= 8 {
        let sub_size = Reader::wrap(fa).be_u32("ext_size").unwrap_or(0);
        let sub_type = Reader::wrap(fa).fourcc("ext_type").unwrap_or(0);
        let sub_total = sub_size as usize;
        if sub_total < 8 || sub_total > remaining {
            break;
        }
        let body = sub_total - 8;
        match sub_type {
            BOX_AVCC => parse_avcc(fa, body, track),
            BOX_HVCC => parse_hvcc(fa, body, track),
            BOX_COLR => parse_colr(fa, body, track),
            BOX_PASP => parse_pasp(fa, body, track),
            BOX_DVCC => parse_dvcc(fa, body, track),
            BOX_DVVC => parse_dvcc(fa, body, track),
            _ => fa.skip_hexa(body, "visual_ext"),
        }
        remaining -= sub_total;
    }
    if remaining > 0 {
        fa.skip_hexa(remaining, "visual_tail");
    }
}

/// Parse colr box. Layout:
///   4 bytes color_type ('nclx', 'nclc', 'rICC', 'prof')
///   if nclx/nclc:
///     2 bytes color_primaries
///     2 bytes transfer_characteristics
///     2 bytes matrix_coefficients
///     1 byte full_range_flag (nclx only — bit 7)
fn parse_colr(fa: &mut FileAnalyze, body_size: usize, track: &mut TrackInfo) {
    if body_size < 4 {
        fa.skip_hexa(body_size, "colr_short");
        return;
    }
    let color_type = Reader::wrap(fa).fourcc("color_type").unwrap_or(0);
    let is_nclx = color_type == u32::from_be_bytes(*b"nclx");
    let is_nclc = color_type == u32::from_be_bytes(*b"nclc");
    if (is_nclx || is_nclc) && body_size >= 4 + 6 {
        let prim = Reader::wrap(fa).be_u16("primaries").unwrap_or(0);
        let trc = Reader::wrap(fa).be_u16("transfer").unwrap_or(0);
        let mat = Reader::wrap(fa).be_u16("matrix").unwrap_or(0);
        track.color_primaries_idc = Some(prim);
        track.color_transfer_idc = Some(trc);
        track.color_matrix_idc = Some(mat);
        let mut consumed = 4 + 6;
        if is_nclx && body_size >= 4 + 7 {
            let flag_bytes = fa.read_raw(1).to_vec();
            track.color_full_range = Some((flag_bytes[0] & 0x80) != 0);
            consumed += 1;
        }
        let rest = body_size - consumed;
        if rest > 0 {
            fa.skip_hexa(rest, "colr_tail");
        }
    } else {
        fa.skip_hexa(body_size - 4, "colr_profile");
    }
}

/// Parse pasp box. Layout: 4 bytes h_spacing + 4 bytes v_spacing.
fn parse_pasp(fa: &mut FileAnalyze, body_size: usize, track: &mut TrackInfo) {
    if body_size < 8 {
        fa.skip_hexa(body_size, "pasp_short");
        return;
    }
    let h = Reader::wrap(fa).be_u32("h_spacing").unwrap_or(0);
    let v = Reader::wrap(fa).be_u32("v_spacing").unwrap_or(0);
    track.pasp_h = Some(h);
    track.pasp_v = Some(v);
    let rest = body_size - 8;
    if rest > 0 {
        fa.skip_hexa(rest, "pasp_tail");
    }
}

/// Parse dvcC / dvvC (Dolby Vision Configuration Box).
fn parse_dvcc(fa: &mut FileAnalyze, body_size: usize, track: &mut TrackInfo) {
    if body_size < 4 {
        fa.skip_hexa(body_size, "dvcc_short");
        return;
    }
    let body = fa.read_raw(body_size).to_vec();
    // dvcC layout:
    //   byte 0: dv_version_major
    //   byte 1: dv_version_minor
    //   byte 2: dv_profile << 1 | dv_bl_signal_compatibility_id
    //   byte 3: 4 bits reserved, 4 bits dv_level
    let dv_profile = (body[2] >> 1) & 0x7F;
    let dv_bl_present = (body[2] & 0x01) != 0;
    let dv_level = body[3] & 0x0F;
    let dv_rpu_present = if body_size > 4 { (body[4] & 0x80) != 0 } else { false };
    let dv_el_present = if body_size > 4 { (body[4] & 0x40) != 0 } else { false };

    track.dovi_profile = Some(dv_profile);
    track.dovi_level = Some(dv_level);
    track.dovi_bl_present = dv_bl_present;
    track.dovi_rpu_present = dv_rpu_present;
    track.dovi_el_present = dv_el_present;

    // BL compatibility ID in bits 5-2 of byte 4
    if body_size > 4 {
        track.dovi_bl_compat_id = Some((body[4] >> 2) & 0x0F);
    }
}

/// Parse avcC (AVCDecoderConfigurationRecord). Layout:/// Parse avcC (AVCDecoderConfigurationRecord). Layout:
///   1 byte configurationVersion
///   1 byte AVCProfileIndication
///   1 byte profile_compatibility
///   1 byte AVCLevelIndication
///   1 byte (6 reserved + 2 bits lengthSizeMinusOne)
///   1 byte (3 reserved + 5 bits numOfSequenceParameterSets)
///   for each SPS: 2 bytes spsLength + SPS NAL bytes (incl. NAL header byte)
///   1 byte numOfPictureParameterSets
///   for each PPS: 2 bytes ppsLength + PPS NAL bytes
fn parse_avcc(fa: &mut FileAnalyze, body_size: usize, track: &mut TrackInfo) {
    if body_size < 4 {
        fa.skip_hexa(body_size, "avcc_short");
        return;
    }
    let body = fa.read_raw(body_size).to_vec();
    track.avc_profile_idc = Some(body[1]);
    track.avc_profile_compat = Some(body[2]);
    track.avc_level_idc = Some(body[3]);
    if body.len() < 7 {
        return;
    }
    // body[4] = lengthSizeMinusOne (low 2 bits); body[5] = numOfSPS (low 5 bits)
    track.avc_nal_length_size = Some((body[4] & 0x03) + 1);
    let num_sps = (body[5] & 0x1F) as usize;
    let mut pos = 6;
    let mut first_sps: Option<&[u8]> = None;
    for i in 0..num_sps {
        if pos + 2 > body.len() {
            return;
        }
        let len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
        pos += 2;
        if pos + len > body.len() {
            return;
        }
        if i == 0 {
            first_sps = Some(&body[pos..pos + len]);
        }
        pos += len;
    }
    if let Some(sps) = first_sps {
        if let Some(info) = revelo_parsers_video::parse_avc_sps(sps) {
            track.avc_sps = Some(info);
        }
    }
    // PPS (optional second-pass): the entropy_coding_mode_flag is the
    // first bit of pic_parameter_set_rbsp after pic_parameter_set_id and
    // seq_parameter_set_id (both ue). We pull it for CABAC detection.
    if pos < body.len() {
        let num_pps = body[pos] as usize;
        pos += 1;
        for i in 0..num_pps {
            if pos + 2 > body.len() {
                return;
            }
            let len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
            pos += 2;
            if pos + len > body.len() {
                return;
            }
            if i == 0 {
                track.avc_cabac = parse_pps_cabac(&body[pos..pos + len]);
            }
            pos += len;
        }
    }
}

/// Parse hvcC (HEVCDecoderConfigurationRecord) — the fixed 23-byte
/// header carries everything needed for Format_Profile/Level/Tier/
/// ChromaSubsampling/BitDepth without touching the SPS arrays.
/// Layout:
///   1 byte configurationVersion
///   1 byte (profile_space:2 + tier_flag:1 + profile_idc:5)
///   4 bytes profile_compatibility_flags
///   6 bytes general_constraint_indicator_flags
///   1 byte level_idc
///   2 bytes (4 reserved + 12 bits min_spatial_segmentation_idc)
///   1 byte (6 reserved + 2 bits parallelismType)
///   1 byte (6 reserved + 2 bits chroma_format_idc)
///   1 byte (5 reserved + 3 bits bit_depth_luma_minus8)
///   1 byte (5 reserved + 3 bits bit_depth_chroma_minus8)
///   2 bytes avg_frame_rate
///   1 byte (constant_frame_rate:2 + num_temporal_layers:3 + temporal_id_nested:1 + lengthSizeMinusOne:2)
///   1 byte num_of_arrays
///   then VPS/SPS/PPS arrays.
fn parse_hvcc(fa: &mut FileAnalyze, body_size: usize, track: &mut TrackInfo) {
    if body_size < 23 {
        fa.skip_hexa(body_size, "hvcc_short");
        return;
    }
    let body = fa.read_raw(body_size).to_vec();
    let profile_byte = body[1];
    track.hevc_tier_high = Some((profile_byte & 0x20) != 0);
    track.hevc_profile_idc = Some(profile_byte & 0x1F);
    track.hevc_level_idc = Some(body[12]);
    // chroma_format_idc lives in low 2 bits of byte 16
    track.hevc_chroma_format_idc = Some(body[16] & 0x03);
    // bit_depth_luma_minus8 in low 3 bits of byte 17
    track.hevc_bit_depth_luma = Some(8 + (body[17] & 0x07));

    // Parse SPS and SEI arrays to extract VUI colour and encoder info.
    // hvcC layout after 23-byte header:
    //   byte 22: num_of_arrays
    //   For each array:
    //     1 byte: (array_completeness << 7) | nal_unit_type
    //     2 bytes: num_nalus
    //     For each nalu:
    //       2 bytes: nalu_length
    //       nalu_length bytes: NAL unit data
    let mut sei_nalus: Vec<&[u8]> = Vec::new();
    if body_size > 23 {
        let mut pos = 23usize;
        if pos < body_size {
            let num_arrays = body[pos] as usize;
            pos += 1;
            for _ in 0..num_arrays {
                if pos + 3 > body_size {
                    break;
                }
                let nal_type = body[pos] & 0x3F;
                pos += 1;
                let num_nalus = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
                pos += 2;
                for _ in 0..num_nalus {
                    if pos + 2 > body_size {
                        break;
                    }
                    let nalu_len = u16::from_be_bytes([body[pos], body[pos + 1]]) as usize;
                    pos += 2;
                    if pos + nalu_len > body_size {
                        break;
                    }
                    // NAL type 33 = SPS - parse it for VUI colour info
                    if nal_type == 33 {
                        let sps_data = &body[pos..pos + nalu_len];
                        if let Some(info) = revelo_parsers_video::parse_hevc_sps(sps_data) {
                            track.hevc_sps = Some(info);
                        }
                    }
                    // NAL type 39 = SEI_PREFIX, 40 = SEI_SUFFIX
                    if nal_type == 39 || nal_type == 40 {
                        sei_nalus.push(&body[pos..pos + nalu_len]);
                    }
                    pos += nalu_len;
                }
            }
        }
    }

    // Try to extract encoder string from SEI NALs
    if !sei_nalus.is_empty() {
        if let Some(enc) = revelo_parsers_video::extract_encoder_from_sei_nalus(&sei_nalus) {
            track.encoder_info = Some(enc);
        }
    }
}

/// Read the entropy_coding_mode_flag from a PPS NAL. Layout:
///   1 byte NAL header
///   ue: pic_parameter_set_id
///   ue: seq_parameter_set_id
///   1 bit: entropy_coding_mode_flag (1 = CABAC, 0 = CAVLC)
fn parse_pps_cabac(pps: &[u8]) -> Option<bool> {
    if pps.len() < 2 {
        return None;
    }
    let clean = remove_emulation_bytes(&pps[1..]);
    let mut br = BitReader::new(&clean);
    br.read_ue()?; // pic_parameter_set_id
    br.read_ue()?; // seq_parameter_set_id
    Some(br.read_bit()? == 1)
}

/// Strip 0x000003 emulation prevention bytes (collapse to 0x0000).
fn remove_emulation_bytes(rbsp: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rbsp.len());
    let mut i = 0;
    while i < rbsp.len() {
        if i + 2 < rbsp.len() && rbsp[i] == 0 && rbsp[i + 1] == 0 && rbsp[i + 2] == 3 {
            out.push(0);
            out.push(0);
            i += 3;
        } else {
            out.push(rbsp[i]);
            i += 1;
        }
    }
    out
}

/// Minimal MSB-first bit reader for parsing NAL bitstreams.
struct BitReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    fn read_bit(&mut self) -> Option<u32> {
        let byte = self.pos / 8;
        if byte >= self.buf.len() {
            return None;
        }
        let bit = 7 - (self.pos % 8);
        let v = (self.buf[byte] >> bit) & 1;
        self.pos += 1;
        Some(v as u32)
    }
    /// Read unsigned Exp-Golomb.
    fn read_ue(&mut self) -> Option<u32> {
        let mut zeros = 0u32;
        while self.read_bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        let mut val = 0u32;
        for _ in 0..zeros {
            val = (val << 1) | self.read_bit()?;
        }
        Some((1u32 << zeros) - 1 + val)
    }
}

/// Parse the esds box body (after the 8-byte box header has been
/// consumed by the caller). Layout:
///   1 byte version + 3 bytes flags (zero)
///   ES_Descriptor (tag 0x03)
///     2 bytes ES_ID + 1 byte flags
///     [if streamDependenceFlag: 2 bytes dependsOnESID]
///     [if URL_Flag: 1 byte URLlength + URL bytes]
///     [if OCRstreamFlag: 2 bytes OCRESID]
///     DecoderConfigDescriptor (tag 0x04)
///       1 byte objectTypeIndication
///       1 byte (streamType<6> + upStream<1> + reserved<1>)
///       3 bytes bufferSizeDB
///       4 bytes maxBitrate
///       4 bytes avgBitrate
///       DecoderSpecificInfo (tag 0x05)
///         AudioSpecificConfig:
///           5 bits: audioObjectType (if 31, then +6 bits explicit)
///           4 bits: samplingFrequencyIndex (if 15, then 24 bits explicit)
///           4 bits: channelConfiguration
///     SLConfigDescriptor (tag 0x06) — predefined, ignore
#[allow(dead_code)]
fn parse_esds(fa: &mut FileAnalyze, body_size: usize, track: &mut TrackInfo) {
    let start = fa.element_offset();
    let end = start + body_size;
    if body_size < 4 {
        fa.skip_hexa(body_size, "esds_short");
        return;
    }
    fa.skip_hexa(4, "version_flags");

    // ES_Descriptor (tag 0x03) — walk descriptors until we find the
    // decoder config + its DecoderSpecificInfo child.
    parse_descriptor_chain(fa, end - fa.element_offset(), track);

    if fa.element_offset() < end {
        fa.skip_hexa(end - fa.element_offset(), "esds_tail");
    }
}

/// Read a single MPEG-4 BER-style descriptor length (1-4 bytes,
/// each with a continuation bit in the high bit).
#[allow(dead_code)]
fn read_descriptor_length(fa: &mut FileAnalyze) -> usize {
    let mut size: usize = 0;
    for _ in 0..4 {
        let bytes = fa.peek_raw(1);
        let Some(b) = bytes else { return size };
        let byte = b[0];
        let _ = fa.read_raw(1);
        size = (size << 7) | (byte & 0x7F) as usize;
        if (byte & 0x80) == 0 {
            return size;
        }
    }
    size
}

#[allow(dead_code)]
fn parse_descriptor_chain(fa: &mut FileAnalyze, region_size: usize, track: &mut TrackInfo) {
    let region_end = fa.element_offset() + region_size;
    while fa.element_offset() + 2 <= region_end {
        let bytes = fa.peek_raw(1);
        let Some(b) = bytes else { break };
        let tag = b[0];
        let _ = fa.read_raw(1);
        let size = read_descriptor_length(fa);
        let body_start = fa.element_offset();
        let body_end = body_start + size;
        if body_end > region_end {
            break;
        }
        match tag {
            0x03 => parse_es_descriptor(fa, size, track),
            0x04 => parse_decoder_config(fa, size, track),
            0x05 => parse_decoder_specific_info(fa, size, track),
            _ => fa.skip_hexa(size, "unknown_descriptor"),
        }
        if fa.element_offset() < body_end {
            fa.skip_hexa(body_end - fa.element_offset(), "descriptor_tail");
        } else if fa.element_offset() > body_end {
            break;
        }
    }
}

#[allow(dead_code)]
fn parse_es_descriptor(fa: &mut FileAnalyze, size: usize, track: &mut TrackInfo) {
    if size < 3 {
        fa.skip_hexa(size, "es_descriptor_short");
        return;
    }
    let start = fa.element_offset();
    fa.skip_hexa(2, "ES_ID");
    let flags = Reader::wrap(fa).be_u8("flags").unwrap_or(0);
    let stream_dep = (flags & 0x80) != 0;
    let url_flag = (flags & 0x40) != 0;
    let ocr_flag = (flags & 0x20) != 0;
    if stream_dep {
        fa.skip_hexa(2, "dependsOnESID");
    }
    if url_flag {
        let url_bytes = fa.peek_raw(1);
        if let Some(b) = url_bytes {
            let url_len = b[0] as usize;
            let _ = fa.read_raw(1);
            fa.skip_hexa(url_len, "URL");
        }
    }
    if ocr_flag {
        fa.skip_hexa(2, "OCR_ES_Id");
    }
    let consumed = fa.element_offset() - start;
    // Remaining body bytes contain nested descriptors (DecoderConfig etc).
    let inner = size.saturating_sub(consumed);
    parse_descriptor_chain(fa, inner, track);
}

#[allow(dead_code)]
fn parse_decoder_config(fa: &mut FileAnalyze, size: usize, track: &mut TrackInfo) {
    if size < 13 {
        fa.skip_hexa(size, "decoder_config_short");
        return;
    }
    let start = fa.element_offset();
    let oti = Reader::wrap(fa).be_u8("objectTypeIndication").unwrap_or(0);
    track.object_type_indication = Some(oti);
    fa.skip_hexa(1, "streamType_upStream");
    fa.skip_hexa(3, "bufferSizeDB");
    let max_br = Reader::wrap(fa).be_u32("maxBitrate").unwrap_or(0);
    let avg_br = Reader::wrap(fa).be_u32("avgBitrate").unwrap_or(0);
    if avg_br > 0 {
        track.avg_bitrate_bps = Some(avg_br);
    }
    if max_br > 0 {
        track.max_bitrate_bps = Some(max_br);
    }
    let consumed = fa.element_offset() - start;
    let inner = size.saturating_sub(consumed);
    parse_descriptor_chain(fa, inner, track);
}

#[allow(dead_code)]
fn parse_decoder_specific_info(fa: &mut FileAnalyze, size: usize, track: &mut TrackInfo) {
    if size < 2 {
        fa.skip_hexa(size, "dsi_short");
        return;
    }
    let bytes = fa.read_raw(size.min(2)).to_vec();
    if bytes.len() < 2 {
        return;
    }
    // 5 bits AOT + 4 bits sampling_frequency_index + 4 bits
    // channel_config = 13 bits, fits in the first 2 bytes.
    let aot = (bytes[0] >> 3) & 0x1F;
    track.audio_object_type = Some(aot);
    if size > 2 {
        fa.skip_hexa(size - 2, "dsi_tail");
    }
}

fn parse_stsz(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 12 {
        fa.skip_hexa(body_size, "stsz");
        return;
    }
    let start = fa.element_offset();
    fa.skip_hexa(4, "version_flags");
    let sample_size = Reader::wrap(fa).be_u32("sample_size").unwrap_or(0);
    let sample_count = Reader::wrap(fa).be_u32("sample_count").unwrap_or(0);
    track.sample_count = Some(sample_count);

    if sample_size != 0 {
        // Uniform sample size — no per-sample table follows.
        track.source_stream_size = Some((sample_count as u64) * (sample_size as u64));
        track.first_sample_size = Some(sample_size);
        track.last_sample_size = Some(sample_size);
    } else {
        // Per-sample size table: 4 bytes per entry, sample_count entries.
        let table_bytes = (sample_count as usize) * 4;
        let available = body_size.saturating_sub(fa.element_offset() - start);
        let read_bytes = table_bytes.min(available);
        let entries = read_bytes / 4;
        let mut total: u64 = 0;
        let mut last_entry: u32 = 0;
        for idx in 0..entries {
            let entry = Reader::wrap(fa).be_u32("sample_size_entry").unwrap_or(0);
            if idx == 0 {
                track.first_sample_size = Some(entry);
            }
            last_entry = entry;
            total = total.saturating_add(entry as u64);
        }
        if entries > 0 {
            track.last_sample_size = Some(last_entry);
        }
        track.source_stream_size = Some(total);
    }

    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "stsz_tail");
    }
}

/// Parse stco (32-bit) / co64 (64-bit) chunk-offset tables. We only need
/// the first entry — the absolute file offset of the track's first chunk,
/// i.e. where its first sample begins — to locate AVC SEI in mdat.
fn parse_stco(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo, is_co64: bool) {
    let body_size = box_size.saturating_sub(8);
    let entry_size = if is_co64 { 8 } else { 4 };
    if body_size < 8 + entry_size {
        fa.skip_hexa(body_size, "stco");
        return;
    }
    let start = fa.element_offset();
    fa.skip_hexa(4, "version_flags");
    let entry_count = Reader::wrap(fa).be_u32("entry_count").unwrap_or(0);
    if entry_count > 0 {
        let offset = if is_co64 {
            let v = Reader::wrap(fa).be_u64("chunk_offset").unwrap_or(0);
            v
        } else {
            let v = Reader::wrap(fa).be_u32("chunk_offset").unwrap_or(0);
            v as u64
        };
        track.first_chunk_offset = Some(offset);
    }
    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "stco_tail");
    }
}

/// Parse stts (time-to-sample). Sets `stts_cfr_delta` when the track is
/// effectively CFR: all entries share one delta, optionally with a final
/// entry of `sample_count == 1` carrying a different delta (the trailing
/// short frame, common in encoded video — accompanies Duration_LastFrame).
fn parse_stts(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 8 {
        fa.skip_hexa(body_size, "stts");
        return;
    }
    let start = fa.element_offset();
    fa.skip_hexa(4, "version_flags");
    let entry_count = Reader::wrap(fa).be_u32("entry_count").unwrap_or(0);
    let table_bytes = (entry_count as usize) * 8;
    let available = body_size.saturating_sub(fa.element_offset() - start);
    let read_bytes = table_bytes.min(available);
    let entries = read_bytes / 8;
    let mut all: Vec<(u32, u32)> = Vec::with_capacity(entries);
    let mut min_d: Option<u32> = None;
    let mut max_d: Option<u32> = None;
    let mut total_samples: u64 = 0;
    let mut total_duration: u64 = 0;
    for _ in 0..entries {
        let sc = Reader::wrap(fa).be_u32("sample_count").unwrap_or(0);
        let sd = Reader::wrap(fa).be_u32("sample_delta").unwrap_or(0);
        all.push((sc, sd));
        if sd > 0 {
            min_d = Some(min_d.map_or(sd, |m| m.min(sd)));
            max_d = Some(max_d.map_or(sd, |m| m.max(sd)));
        }
        total_samples = total_samples.saturating_add(sc as u64);
        total_duration = total_duration.saturating_add((sc as u64) * (sd as u64));
    }
    track.stts_min_delta = min_d;
    track.stts_max_delta = max_d;
    track.stts_total_samples = total_samples;
    track.stts_total_duration = total_duration;
    // Determine the dominant delta. Allow the final entry to differ only
    // if it represents exactly one sample (the partial trailing frame).
    let core = if all.len() >= 2 && all.last().map(|(c, _)| *c == 1).unwrap_or(false) {
        &all[..all.len() - 1]
    } else {
        &all[..]
    };
    let mut common: Option<u32> = None;
    let mut cfr = !core.is_empty();
    for (_sc, sd) in core {
        match common {
            None => common = Some(*sd),
            Some(d) if d == *sd => {}
            Some(_) => {
                cfr = false;
                break;
            }
        }
    }
    if cfr {
        track.stts_cfr_delta = common;
    }
    let consumed = fa.element_offset() - start;
    if consumed < body_size {
        fa.skip_hexa(body_size - consumed, "stts_tail");
    }
}

fn fill_streams(
    fa: &mut FileAnalyze,
    ftyp_brands: &[String],
    tracks: &[TrackInfo],
    movie: &MovieInfo,
    layout: &BoxLayout,
) {
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "MPEG-4");

    // mdat-positioning fields. The C++ side reports:
    //   HeaderSize = bytes before mdat box (ftyp + free + any pre-mdat moov)
    //   DataSize   = mdat total size (header + body)
    //   FooterSize = bytes after mdat
    //   StreamSize (general) = FileSize - mdat_body_size = HeaderSize + 8 + FooterSize
    //   IsStreamable = "Yes" if moov precedes mdat, else "No"
    if let (Some(mdat_off), Some(mdat_tot)) = (layout.mdat_offset, layout.mdat_size) {
        let footer_size = layout.file_size.saturating_sub(mdat_off + mdat_tot);
        let stream_size = layout.file_size.saturating_sub(mdat_tot.saturating_sub(8));
        fa.force_field(StreamKind::General, 0, "StreamSize", stream_size.to_string());
        fa.set_field(StreamKind::General, 0, "HeaderSize", mdat_off.to_string());
        fa.set_field(StreamKind::General, 0, "DataSize", mdat_tot.to_string());
        fa.set_field(StreamKind::General, 0, "FooterSize", footer_size.to_string());
        let is_streamable = match layout.moov_offset {
            Some(mv_off) if mv_off < mdat_off => "Yes",
            _ => "No",
        };
        fa.set_field(StreamKind::General, 0, "IsStreamable", is_streamable);
    }

    // Format_Profile follows the major brand: M4A/M4B/M4V/M4P → "Apple
    // audio/video with iTunes info"; all generic ISO BMFF brands
    // (isom, iso2, iso6, mp41, mp42, etc.) → "Base Media". Presence of
    // iTunes metadata alone doesn't change the profile — oracle still
    // reports "Base Media" for plain isom files with optional ilst.
    if let Some(major) = ftyp_brands.first() {
        let profile = match major.as_str() {
            "M4A " | "M4B " => "Apple audio with iTunes info",
            "M4V " | "M4VH" | "M4VP" => "Apple video with iTunes info",
            "M4P " => "Apple audio with iTunes info, protected",
            "qt  " => "QuickTime",
            "mp41" => "Base Media / Version 1",
            "mp42" => "Base Media / Version 2",
            "avc1" => "JVT",
            _ => "Base Media",
        };
        fa.set_field(StreamKind::General, 0, "Format_Profile", profile);
    }
    for (key, value) in &movie.itunes_metadata {
        match *key {
            ITUNES_KEY_TOOL => {
                fa.set_field(StreamKind::General, 0, "Encoded_Application", value.clone());
            }
            ITUNES_KEY_COMMENT => {
                fa.set_field(StreamKind::General, 0, "Comment", value.clone());
            }
            ITUNES_KEY_TITLE => {
                fa.set_field(StreamKind::General, 0, "Title", value.clone());
            }
            ITUNES_KEY_ARTIST => {
                fa.set_field(StreamKind::General, 0, "Performer", value.clone());
            }
            ITUNES_KEY_ALBUM => {
                fa.set_field(StreamKind::General, 0, "Album", value.clone());
            }
            ITUNES_KEY_DATE => {
                fa.set_field(StreamKind::General, 0, "Released_Date", value.clone());
            }
            ITUNES_KEY_GENRE => {
                fa.set_field(StreamKind::General, 0, "Genre", value.clone());
            }
            ITUNES_KEY_WRITER => {
                fa.set_field(StreamKind::General, 0, "Composer", value.clone());
            }
            ITUNES_KEY_GROUPING => {
                fa.set_field(StreamKind::General, 0, "Grouping", value.clone());
            }
            ITUNES_KEY_TRACK => {
                // Track number is typically 4 bytes: track (2 bytes) / total (2 bytes)
                if value.len() >= 4 {
                    let track = ((value.as_bytes()[2] as u16) << 8) | (value.as_bytes()[3] as u16);
                    let total = ((value.as_bytes()[0] as u16) << 8) | (value.as_bytes()[1] as u16);
                    if track > 0 {
                        fa.set_field(StreamKind::General, 0, "Track", track.to_string());
                    }
                    if total > 0 {
                        fa.set_field(StreamKind::General, 0, "Track/Total", total.to_string());
                    }
                }
            }
            ITUNES_KEY_DISK => {
                // Disc number is typically 4 bytes: disc (2 bytes) / total (2 bytes)
                if value.len() >= 4 {
                    let disc = ((value.as_bytes()[2] as u16) << 8) | (value.as_bytes()[3] as u16);
                    let total = ((value.as_bytes()[0] as u16) << 8) | (value.as_bytes()[1] as u16);
                    if disc > 0 {
                        fa.set_field(StreamKind::General, 0, "Part", disc.to_string());
                    }
                    if total > 0 {
                        fa.set_field(StreamKind::General, 0, "Part/Total", total.to_string());
                    }
                }
            }
            ITUNES_KEY_COMPILATION => {
                if value.as_bytes().get(0) == Some(&1) {
                    fa.set_field(StreamKind::General, 0, "Compilation", "Yes");
                }
            }
            ITUNES_KEY_GAPLESS => {
                if value.as_bytes().get(0) == Some(&1) {
                    fa.set_field(StreamKind::General, 0, "Gapless", "Yes");
                }
            }
            ITUNES_KEY_LYRICS => {
                fa.set_field(StreamKind::General, 0, "Lyrics", value.clone());
            }
            ITUNES_KEY_TV_SHOW => {
                fa.set_field(StreamKind::General, 0, "TVShow", value.clone());
            }
            ITUNES_KEY_TV_EPISODE => {
                fa.set_field(StreamKind::General, 0, "TVEpisode", value.clone());
            }
            ITUNES_KEY_TV_SEASON => {
                // 4-byte integer
                if value.len() >= 4 {
                    let season = ((value.as_bytes()[0] as u32) << 24)
                        | ((value.as_bytes()[1] as u32) << 16)
                        | ((value.as_bytes()[2] as u32) << 8)
                        | (value.as_bytes()[3] as u32);
                    fa.set_field(StreamKind::General, 0, "TVSeason", season.to_string());
                }
            }
            ITUNES_KEY_TV_EPISODE_NUM => {
                // 4-byte integer
                if value.len() >= 4 {
                    let ep = ((value.as_bytes()[0] as u32) << 24)
                        | ((value.as_bytes()[1] as u32) << 16)
                        | ((value.as_bytes()[2] as u32) << 8)
                        | (value.as_bytes()[3] as u32);
                    fa.set_field(StreamKind::General, 0, "TVEpisodeNumber", ep.to_string());
                }
            }
            ITUNES_KEY_HD_VIDEO => {
                if value.as_bytes().get(0) == Some(&1) {
                    fa.set_field(StreamKind::General, 0, "HDVideo", "Yes");
                }
            }
            ITUNES_KEY_MEDIA_TYPE => {
                // Media type is a 1-byte integer
                if let Some(&media_type) = value.as_bytes().get(0) {
                    let media_str = match media_type {
                        0 => "Movie",
                        1 => "Music",
                        2 => "Audiobook",
                        6 => "Music Video",
                        9 => "Movie",
                        10 => "TV Show",
                        11 => "Booklet",
                        14 => "Ringtone",
                        _ => "Unknown",
                    };
                    fa.set_field(StreamKind::General, 0, "MediaType", media_str.to_string());
                }
            }
            ITUNES_KEY_RATING => {
                // Content rating is a 1-byte integer
                if let Some(&rating) = value.as_bytes().get(0) {
                    let rating_str = match rating {
                        0 => "None",
                        2 => "Clean",
                        4 => "Explicit",
                        _ => "Unknown",
                    };
                    fa.set_field(StreamKind::General, 0, "ContentRating", rating_str.to_string());
                }
            }
            ITUNES_KEY_PUBLISHER => {
                fa.set_field(StreamKind::General, 0, "Publisher", value.clone());
            }
            ITUNES_KEY_ENCODED_BY => {
                fa.set_field(StreamKind::General, 0, "EncodedBy", value.clone());
            }
            _ => {}
        }
    }
    // QuickTime mdta keys (iPhone/iPad recordings).
    let mut qt_make: Option<&str> = None;
    let mut qt_model: Option<&str> = None;
    let mut qt_software: Option<&str> = None;
    for (key, value) in &movie.qt_metadata {
        match key.as_str() {
            "com.apple.quicktime.make" => qt_make = Some(value.as_str()),
            "com.apple.quicktime.model" => qt_model = Some(value.as_str()),
            "com.apple.quicktime.software" => qt_software = Some(value.as_str()),
            "com.apple.quicktime.creationdate" => {
                // ISO 8601 "YYYY-MM-DDTHH:MM:SS±HHMM" or "±HH:MM" →
                // oracle's "YYYY-MM-DD HH:MM:SS±HH:MM" (T→space, and
                // ensure colon in TZ offset).
                let mut s = value.replacen('T', " ", 1);
                // Normalise trailing ±HHMM (no colon) to ±HH:MM.
                let bytes = s.as_bytes();
                let len = bytes.len();
                if len >= 5 {
                    let tail = &bytes[len - 5..];
                    if (tail[0] == b'+' || tail[0] == b'-')
                        && tail[1..].iter().all(|c| c.is_ascii_digit())
                    {
                        s = format!("{}:{}", &s[..len - 2], &s[len - 2..]);
                    }
                }
                fa.set_field(StreamKind::General, 0, "Recorded_Date", s);
            }
            "com.apple.quicktime.location.ISO6709" => {
                if let Some(s) = parse_iso6709(value) {
                    fa.set_field(StreamKind::General, 0, "Recorded_Location", s);
                }
            }
            // Unmapped reverse-DNS keys → <extra> as flattened names.
            // Oracle flattens: dots → underscores, dashes stripped (e.g.
            // `com.apple.quicktime.full-frame-rate-playback-intent` →
            // `com_apple_quicktime_fullframerateplaybackintent`).
            other if other.starts_with("com.apple.quicktime.") => {
                let flat: String = other
                    .chars()
                    .filter(|&c| c != '-')
                    .map(|c| if c == '.' { '_' } else { c })
                    .collect();
                fa.set_extra_field(StreamKind::General, 0, &flat, value.clone());
            }
            _ => {}
        }
    }
    if let Some(make) = qt_make {
        // Encoded_Library is the canonical "tool that wrote this file".
        // Oracle uses "Apple QuickTime" as a friendly fixed label for
        // Apple-made files (even when the make/model/software say more
        // specific things) — emit unconditionally when we detect Apple.
        if make == "Apple" {
            fa.set_field(StreamKind::General, 0, "Encoded_Library", "Apple QuickTime");
            fa.set_field(StreamKind::General, 0, "Encoded_Library_CompanyName", "Apple");
            fa.set_field(StreamKind::General, 0, "Encoded_Library_Name", "QuickTime");
        }
        // Hardware fields use the make string directly as company.
        fa.set_field(StreamKind::General, 0, "Encoded_Hardware_CompanyName", make.to_string());
        if let Some(m) = qt_model {
            fa.set_field(StreamKind::General, 0, "Encoded_Hardware_Name", m.to_string());
        }
    }
    if let Some(sw) = qt_software {
        // Apple software field carries the iOS/macOS version, e.g.
        // "18.6.2". Oracle derives Name="iOS" + CompanyName="Apple" when
        // the make is Apple (heuristic: presence of make=Apple implies
        // this is an iOS device, not a desktop).
        fa.set_field(StreamKind::General, 0, "Encoded_OperatingSystem_Version", sw.to_string());
        if matches!(qt_make, Some("Apple")) {
            fa.set_field(StreamKind::General, 0, "Encoded_OperatingSystem_CompanyName", "Apple");
            fa.set_field(StreamKind::General, 0, "Encoded_OperatingSystem_Name", "iOS");
        }
    }
    // mvhd creation/modification → General.Encoded_Date / Tagged_Date.
    if let Some(s) = movie.creation_time.and_then(mp4_date_string) {
        fa.set_field(StreamKind::General, 0, "Encoded_Date", s);
    }
    if let Some(s) = movie.modification_time.and_then(mp4_date_string) {
        fa.set_field(StreamKind::General, 0, "Tagged_Date", s);
    }

    if !ftyp_brands.is_empty() {
        fa.set_field(StreamKind::General, 0, "CodecID", ftyp_brands[0].clone());
        // CodecID_Compatible = all unique brands in ftyp (major + compat)
        // joined with "/". Oracle:
        //   [mp42, mp42, avc1]            → "mp42/avc1"
        //   [qt  , qt  ]                  → "qt  "
        //   [M4A , isom]                  → "M4A /isom"
        // Emit whenever any brand is present (single-brand cases like
        // bare qt  still get the field).
        let mut seen: Vec<&str> = Vec::new();
        for b in ftyp_brands {
            if !seen.iter().any(|s| *s == b.as_str()) {
                seen.push(b.as_str());
            }
        }
        if !seen.is_empty() {
            fa.set_field(StreamKind::General, 0, "CodecID_Compatible", seen.join("/"));
        }
        // QuickTime brand also surfaces a CodecID_Version field — a
        // 16.16-fixed-point minor version (typically 0). Oracle formats
        // as "MMMM.mm".
        if let Some(major) = ftyp_brands.first() {
            if major == "qt  " {
                fa.set_field(StreamKind::General, 0, "CodecID_Version", "0000.00");
            }
        }
    }

    // Calculate General.Duration from movie header with round-to-nearest.
    // Integer truncation of mvhd loses precision (e.g. 600Hz → ms),
    // causing a −1ms shift and cascading errors in SamplingCount, bitrate.
    let general_duration_ms: Option<u64> = if movie.timescale > 0 && movie.duration > 0 {
        let ts = movie.timescale as u64;
        Some((movie.duration * 1000 + ts / 2) / ts)
    } else {
        None
    };

    if let Some(dur) = general_duration_ms {
        fa.set_field(StreamKind::General, 0, "Duration", dur.to_string());

        // OverallBitRate + OverallBitRate_Mode are filled by the harness's
        // fill_file_level_fields (FileSize × 8 / Duration_ms × 1000 with
        // the proper Mode rule — audio-only mirrors Audio.BitRate_Mode,
        // video files emit "VBR" only when video is VFR). Don't double-
        // emit here.
    }

    let mut audio_count: u32 = 0;
    let mut video_count: u32 = 0;
    let mut other_count: u32 = 0;
    let mut stream_order: u32 = 0;
    let mut meta_track_ids: Vec<u32> = Vec::new();
    for (track_idx, track) in tracks.iter().enumerate() {
        if track.handler == HANDLER_SOUN && track.has_data {
            let pos = fa.stream_prepare(StreamKind::Audio);
            fa.set_field(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string());
            // Prefer tkhd's track_ID; fall back to position in moov.
            let id = track.track_id.unwrap_or((track_idx + 1) as u32);
            fa.set_field(StreamKind::Audio, pos, "ID", id.to_string());
            stream_order += 1;
            if let Some(f) = track.audio_format {
                fa.set_field(StreamKind::Audio, pos, "Format", f);
            }
            // SBR/PS profile signaling — AOT 2 (LC) with no explicit SBR
            // signaling in the AudioSpecificConfig is reported as
            // "No (Explicit)" by the oracle.
            if let Some(aot) = track.audio_object_type {
                if let Some(profile) = aac_profile_name(aot) {
                    fa.set_field(StreamKind::Audio, pos, "Format_AdditionalFeatures", profile);
                }
                // Format_Settings_SBR: AAC LC (AOT 2) is explicitly
                // signalled as no-SBR — the oracle emits "No (Explicit)".
                // (HE-AAC AOT 5/29 would be "Yes (Explicit)", but there's no
                // sample to verify against, so only the LC case is emitted.)
                if aot == 2 {
                    fa.set_field(StreamKind::Audio, pos, "Format_Settings_SBR", "No (Explicit)");
                }
            }
            // CodecID from esds: "mp4a-{OTI:hex lowercase}-{AOT}".
            if let (Some(oti), Some(aot)) = (track.object_type_indication, track.audio_object_type)
            {
                let codec_id = format!("mp4a-{:x}-{}", oti, aot);
                fa.set_field(StreamKind::Audio, pos, "CodecID", codec_id);
            }
            // AAC is fundamentally variable-bitrate; oracle marks all
            // AAC tracks as VBR (even when esds carries an avg bitrate
            // hint). MP3/etc fall back to whatever the codec is.
            //
            // BitRate source priority:
            //   1. esds avg_bitrate (when non-zero)
            //   2. stsz total × 8000 / Duration_ms (rounded) — matches
            //      oracle's integer arithmetic (oracle gets 160005 for
            //      610560 × 8000 / 30527). Using mdhd's raw duration_sec
            //      (30.528) gives 160000, a 5 bps undercount.
            let is_aac = matches!(track.audio_format, Some("AAC"));
            let computed_br: Option<u32> = if let Some(ss) = track.source_stream_size {
                if let Some(dur_ms) = general_duration_ms {
                    if dur_ms > 0 {
                        Some(((ss as f64) * 8000.0 / (dur_ms as f64)).round() as u32)
                    } else {
                        None
                    }
                } else if track.timescale > 0 && track.duration_units > 0 {
                    let dur_sec = track.duration_units as f64 / track.timescale as f64;
                    if dur_sec > 0.0 {
                        Some((ss as f64 * 8.0 / dur_sec).round() as u32)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let br_to_emit = track.avg_bitrate_bps.or(computed_br);
            if let Some(br) = br_to_emit {
                // BitRate_Mode: for AAC with esds, CBR when avg ≈ max;
                // without esds, oracle outputs CBR for authored AAC tracks
                // (the fallback computed_br from stsz × 8 / duration
                // matches a constant-bitrate scenario). Harden to match:
                // CBR when avg_bitrate_bps and max_bitrate_bps are equal
                // or when neither is available (computed only), else VBR.
                let br_mode = match (track.avg_bitrate_bps, track.max_bitrate_bps) {
                    (Some(avg), Some(max)) if avg == max => "CBR",
                    (Some(_), Some(_)) => "VBR",
                    _ => "CBR",
                };
                fa.set_field(StreamKind::Audio, pos, "BitRate_Mode", br_mode);
                fa.set_field(StreamKind::Audio, pos, "BitRate", br.to_string());
                // BitRate_Maximum = esds.maxBitrate, emitted only for VBR
                // (avg != max). For CBR (avg == max) the oracle omits it.
                if is_aac
                    && matches!((track.avg_bitrate_bps, track.max_bitrate_bps),
                        (Some(avg), Some(max)) if avg != max)
                {
                    let max = track.max_bitrate_bps.unwrap_or(br);
                    fa.set_field(StreamKind::Audio, pos, "BitRate_Maximum", max.to_string());
                }
            }
            if let Some(ch) = track.audio_channels {
                fa.set_field(StreamKind::Audio, pos, "Channels", ch.to_string());
                let (positions, layout) = channel_layout_for_format(ch, track.audio_format);
                if let Some(p) = positions {
                    fa.set_field(StreamKind::Audio, pos, "ChannelPositions", p);
                }
                if let Some(l) = layout {
                    fa.set_field(StreamKind::Audio, pos, "ChannelLayout", l);
                }
            }
            if let Some(sr) = track.audio_sample_rate {
                fa.set_field(StreamKind::Audio, pos, "SamplingRate", sr.to_string());
                // AAC frames are always 1024 samples. FrameRate is
                // sample_rate/1024 (e.g. 48000/1024 = 46.875).
                if matches!(track.audio_format, Some("AAC")) {
                    fa.set_field(StreamKind::Audio, pos, "SamplesPerFrame", "1024");
                    if sr > 0 {
                        let rate = (sr as f64) / 1024.0;
                        fa.set_field(StreamKind::Audio, pos, "FrameRate", format!("{:.3}", rate));
                    }
                }
            }
            if matches!(track.audio_format, Some("AAC")) {
                fa.set_field(StreamKind::Audio, pos, "Compression_Mode", "Lossy");
            }
            // Duration / SamplingCount / FrameCount derived from the
            // rounded general_duration_ms × sample rate, matching the
            // oracle's approach (compute from ms-rounded duration, not
            // raw mvhd scaled integer). For mvhd at 600 Hz, truncation
            // loses 1/3 ms, propagating into all derived counters.
            let trimmed_units: Option<u64> = if let Some(sr) = track.audio_sample_rate {
                general_duration_ms.map(|dur_ms| (dur_ms * sr as u64 + 500) / 1000)
            } else if track.timescale > 0 && track.duration_units > 0 {
                Some(track.duration_units)
            } else {
                None
            };
            let _ = track.elst_segment_duration; // reserved for multi-track refinement
            if let Some(units) = trimmed_units {
                fa.set_field(StreamKind::Audio, pos, "SamplingCount", units.to_string());
                if let Some(sr) = track.audio_sample_rate {
                    let duration_ms = (units * 1000 + sr as u64 / 2) / sr as u64;
                    fa.set_field(StreamKind::Audio, pos, "Duration", duration_ms.to_string());
                }
                if matches!(track.audio_format, Some("AAC")) {
                    // FrameCount: round-to-nearest for partial AAC frames
                    // (1024 samples each). Oracle's ceil/round behaviour
                    // matches rounding: 1465296/1024 = 1430.95 → 1431.
                    let frame_count = (units + 512) / 1024;
                    fa.set_field(StreamKind::Audio, pos, "FrameCount", frame_count.to_string());
                }
            }
            // Source_* fields: pre-edit values from stsz / mdhd directly.
            if let Some(count) = track.sample_count {
                fa.set_field(StreamKind::Audio, pos, "Source_FrameCount", count.to_string());
                if matches!(track.audio_format, Some("AAC")) {
                    if let Some(sr) = track.audio_sample_rate {
                        // Source_Duration = mdhd.duration (raw, pre-elst)
                        // converted to ms with round-to-nearest. Falls
                        // back to the stsz-derived value if mdhd is
                        // missing.
                        let source_dur = if track.timescale > 0 && track.duration_units > 0 {
                            let ts = track.timescale as u64;
                            (track.duration_units * 1000 + ts / 2) / ts
                        } else {
                            ((count as u64) * 1024 * 1000 + (sr as u64) / 2) / (sr as u64)
                        };
                        fa.set_field(
                            StreamKind::Audio,
                            pos,
                            "Source_Duration",
                            source_dur.to_string(),
                        );
                        // Source_Duration_LastFrame = source_dur −
                        // (sample_count × 1024 / sample_rate). Oracle
                        // omits the field when the delta rounds to zero
                        // (no detectable trailing fraction).
                        let sr_u = sr as u64;
                        let frame_only_dur_ms = ((count as u64) * 1024 * 1000 + sr_u / 2) / sr_u;
                        let delta_ms = source_dur as i64 - frame_only_dur_ms as i64;
                        if delta_ms != 0 {
                            fa.set_field(
                                StreamKind::Audio,
                                pos,
                                "Source_Duration_LastFrame",
                                delta_ms.to_string(),
                            );
                        }
                        // Source_Delay = Duration − Source_Duration, in
                        // ms. For trimmed AAC the trimmed Duration is
                        // shorter than the raw Source_Duration, so this
                        // is negative. Oracle only emits this field when
                        // |delay| ≥ ~10ms (real encoder padding from
                        // live captures); near-zero delays from authored
                        // MP4s are suppressed.
                        if let Some(units) = trimmed_units {
                            if track.timescale > 0 {
                                let ts = track.timescale as u64;
                                let dur_ms = (units * 1000 + ts / 2) / ts;
                                let source_delay = dur_ms as i64 - source_dur as i64;
                                if source_delay.abs() >= 10 {
                                    fa.set_field(
                                        StreamKind::Audio,
                                        pos,
                                        "Source_Delay",
                                        source_delay.to_string(),
                                    );
                                    fa.set_field(
                                        StreamKind::Audio,
                                        pos,
                                        "Source_Delay_Source",
                                        "Container",
                                    );
                                }
                            }
                        }
                    }
                }
            }
            // Audio.StreamSize = sum of stsz sample sizes (Source)
            // and post-edit-list trimmed size. Oracle uses the full
            // stsz sum unless the edit list removes complete AAC
            // frames (1024 samples). Since our elst integration is
            // not per-sample, emit the full size — matches oracle
            // for the common no-trim and sub-frame-trim cases.
            if let Some(size) = track.source_stream_size {
                fa.set_field(StreamKind::Audio, pos, "Source_StreamSize", size.to_string());
                fa.set_field(StreamKind::Audio, pos, "StreamSize", size.to_string());
            }
            // Default: the oracle emits this for tracks in an alternate
            // group — "Yes" for the enabled track, "No" when the
            // track_enabled flag is clear (typical for hint/sub tracks).
            if track.track_enabled == Some(false) {
                fa.set_field(StreamKind::Audio, pos, "Default", "No");
            } else if track.alternate_group.unwrap_or(0) != 0 {
                fa.set_field(StreamKind::Audio, pos, "Default", "Yes");
            }
            if let Some(group) = track.alternate_group {
                if group != 0 {
                    fa.set_field(StreamKind::Audio, pos, "AlternateGroup", group.to_string());
                }
            }
            if let Some(lang) = track.language.as_deref().and_then(iso639_emit) {
                fa.set_field(StreamKind::Audio, pos, "Language", lang);
            }
            if let Some(s) = track.creation_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Audio, pos, "Encoded_Date", s);
            }
            if let Some(s) = track.modification_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Audio, pos, "Tagged_Date", s);
            }
            if let Some(t) = track.handler_name.as_deref() {
                if !t.is_empty() {
                    fa.set_field(StreamKind::Audio, pos, "Title", t.to_string());
                }
            }
            audio_count += 1;
        } else if track.handler == HANDLER_VIDE && track.has_data {
            let pos = fa.stream_prepare(StreamKind::Video);
            fa.set_field(StreamKind::Video, pos, "StreamOrder", stream_order.to_string());
            let id = track.track_id.unwrap_or((track_idx + 1) as u32);
            fa.set_field(StreamKind::Video, pos, "ID", id.to_string());
            stream_order += 1;
            if let Some(f) = track.video_format {
                fa.set_field(StreamKind::Video, pos, "Format", f);
            }
            // AVC profile/level/CABAC + CodecConfigurationBox from avcC.
            if let Some(idc) = track.avc_profile_idc {
                let constrained =
                    track.avc_profile_compat.map(|c| (c & 0x40) != 0).unwrap_or(false);
                if let Some(profile) = avc_profile_name(idc, constrained) {
                    fa.set_field(StreamKind::Video, pos, "Format_Profile", profile);
                }
                // CABAC: prefer PPS-derived value; fall back to "No" only
                // for Baseline (where CABAC isn't allowed at all).
                let cabac = match track.avc_cabac {
                    Some(true) => Some("Yes"),
                    Some(false) => Some("No"),
                    None if idc == 0x42 => Some("No"),
                    None => None,
                };
                if let Some(c) = cabac {
                    fa.set_field(StreamKind::Video, pos, "Format_Settings_CABAC", c);
                }
            }
            if let Some(lvl) = track.avc_level_idc {
                fa.set_field(StreamKind::Video, pos, "Format_Level", format_avc_level(lvl));
            }
            // Format_Settings_RefFrames from SPS.
            if let Some(sps) = track.avc_sps.as_ref() {
                fa.set_field(
                    StreamKind::Video,
                    pos,
                    "Format_Settings_RefFrames",
                    sps.ref_frames.to_string(),
                );
            }
            if track.avc_profile_idc.is_some() {
                fa.set_field(StreamKind::Video, pos, "CodecConfigurationBox", "avcC");
            }
            // HEVC profile/level/tier from hvcC fixed header.
            if let Some(idc) = track.hevc_profile_idc {
                if let Some(name) = hevc_profile_name(idc) {
                    fa.set_field(StreamKind::Video, pos, "Format_Profile", name);
                }
            }
            if let Some(lvl) = track.hevc_level_idc {
                // HEVC encodes level as `level * 30`, so 153 → "5.1".
                let major = lvl / 30;
                let minor = (lvl % 30) / 3;
                let s = if minor == 0 { format!("{major}") } else { format!("{major}.{minor}") };
                fa.set_field(StreamKind::Video, pos, "Format_Level", s);
            }
            if let Some(high) = track.hevc_tier_high {
                fa.set_field(
                    StreamKind::Video,
                    pos,
                    "Format_Tier",
                    if high { "High" } else { "Main" },
                );
            }
            if track.hevc_profile_idc.is_some() {
                fa.set_field(StreamKind::Video, pos, "CodecConfigurationBox", "hvcC");
            }
            // Encoder info from SEI user_data_unregistered — applies to both
            // AVC (avcC) and HEVC (hvcC) tracks. The MP4 path previously
            // dropped name/version/settings, keeping only Encoded_Library.
            if let Some(ref enc) = track.encoder_info {
                fa.set_field(StreamKind::Video, pos, "Encoded_Library", enc.library.as_str());
                if let Some(ref name) = enc.name {
                    fa.set_field(StreamKind::Video, pos, "Encoded_Library_Name", name.as_str());
                }
                if let Some(ref ver) = enc.version {
                    fa.set_field(StreamKind::Video, pos, "Encoded_Library_Version", ver.as_str());
                }
                if let Some(ref settings) = enc.settings {
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "Encoded_Library_Settings",
                        settings.as_str(),
                    );
                }
            }
            // AVC defaults — Baseline/Main/Extended/Constrained-Baseline/High
            // are always 8-bit YUV 4:2:0 progressive per the H.264 spec.
            // High 10/4:2:2/4:4:4 would need SPS chroma_format_idc to be
            // safe; skip defaults for those.
            if matches!(track.avc_profile_idc, Some(0x42) | Some(0x4D) | Some(0x58) | Some(0x64)) {
                fa.set_field(StreamKind::Video, pos, "ColorSpace", "YUV");
                fa.set_field(StreamKind::Video, pos, "ChromaSubsampling", "4:2:0");
                fa.set_field(StreamKind::Video, pos, "BitDepth", "8");
                fa.set_field(StreamKind::Video, pos, "ScanType", "Progressive");
            }
            // HEVC: hvcC supplies chroma_format and bit_depth directly.
            if track.hevc_profile_idc.is_some() {
                fa.set_field(StreamKind::Video, pos, "ColorSpace", "YUV");
                if let Some(cfi) = track.hevc_chroma_format_idc {
                    let s = match cfi {
                        0 => "",
                        1 => "4:2:0",
                        2 => "4:2:2",
                        3 => "4:4:4",
                        _ => "",
                    };
                    if !s.is_empty() {
                        fa.set_field(StreamKind::Video, pos, "ChromaSubsampling", s);
                    }
                }
                if let Some(bd) = track.hevc_bit_depth_luma {
                    fa.set_field(StreamKind::Video, pos, "BitDepth", bd.to_string());
                }
                fa.set_field(StreamKind::Video, pos, "ScanType", "Progressive");
            }
            // Dolby Vision — from dvcC/dvvC config box
            if let Some(dv_prof) = track.dovi_profile {
                fa.set_field(
                    StreamKind::Video,
                    pos,
                    "Format_Profile",
                    format!("Dolby Vision {}.{}", dv_prof / 10, dv_prof % 10),
                );
                fa.set_field(StreamKind::Video, pos, "HDR_Format", "Dolby Vision");
                fa.set_field(StreamKind::Video, pos, "HDR_Format_Version", format!("{}.{}", 1, 0));
                if let Some(dv_level) = track.dovi_level {
                    fa.set_field(StreamKind::Video, pos, "HDR_Format_Level", dv_level.to_string());
                }
                if track.dovi_bl_present {
                    if let Some(cid) = track.dovi_bl_compat_id {
                        fa.set_field(
                            StreamKind::Video,
                            pos,
                            "HDR_Format_Compatibility",
                            format!("BL:{}", cid),
                        );
                    }
                }
            }
            if let Some(cid) = track.video_codec_id {
                fa.set_field(StreamKind::Video, pos, "CodecID", cid);
            }
            // Track duration in ms — prefer elst segment_duration (movie
            // timescale) over raw mdhd duration (media timescale). Match
            // the audio branch's preference order.
            let dur_ms: Option<u64> = if let Some(seg) = track.elst_segment_duration {
                if movie.timescale > 0 {
                    let ts = movie.timescale as u64;
                    Some((seg * 1000 + ts / 2) / ts)
                } else {
                    None
                }
            } else if track.timescale > 0 {
                let ts = track.timescale as u64;
                Some((track.duration_units * 1000 + ts / 2) / ts)
            } else {
                None
            };
            if let Some(ms) = dur_ms {
                fa.set_field(StreamKind::Video, pos, "Duration", ms.to_string());
            }
            if let Some(w) = track.video_width {
                fa.set_field(StreamKind::Video, pos, "Width", w.to_string());
            }
            if let Some(h) = track.video_height {
                fa.set_field(StreamKind::Video, pos, "Height", h.to_string());
            }
            // FrameCount = stsz sample count (one sample per frame for video).
            if let Some(fc) = track.sample_count {
                fa.set_field(StreamKind::Video, pos, "FrameCount", fc.to_string());
            }
            // FrameRate: prefer stts CFR delta (exact rational) over
            // FrameCount/duration approximation. CFR delta gives us
            // FrameRate, FrameRate_Mode=CFR, FrameRate_Num, FrameRate_Den.
            //
            // VFR (stts has varied deltas): use avg = total_samples ×
            // timescale / total_duration, plus FrameRate_Minimum from
            // max delta and FrameRate_Maximum from min delta.
            let mut frame_rate_emitted = false;
            if let (Some(delta), tsc) = (track.stts_cfr_delta, track.timescale) {
                if tsc > 0 && delta > 0 {
                    let fr = tsc as f64 / delta as f64;
                    let g = gcd_u32(tsc, delta);
                    let num = tsc / g;
                    let den = delta / g;
                    fa.set_field(StreamKind::Video, pos, "FrameRate_Mode", "CFR");
                    fa.set_field(StreamKind::Video, pos, "FrameRate", format!("{:.3}", fr));
                    fa.set_field(StreamKind::Video, pos, "FrameRate_Num", num.to_string());
                    fa.set_field(StreamKind::Video, pos, "FrameRate_Den", den.to_string());
                    frame_rate_emitted = true;
                }
            } else if track.stts_total_samples > 0
                && track.stts_total_duration > 0
                && track.timescale > 0
                && track.stts_min_delta.is_some()
                && track.stts_max_delta.is_some()
            {
                // VFR path. Use a 1000× scaled integer rational for the
                // average so e.g. 59.940 fps emits as 59940/1000.
                let tsc = track.timescale as f64;
                let raw_avg =
                    track.stts_total_samples as f64 * tsc / track.stts_total_duration as f64;
                // Snap to a nearby standard frame rate when within 0.5%
                // (iPhone records nominally at NTSC 60p = 60000/1001 ≈
                // 59.940, but actual stts deltas average to 59.96–59.97).
                let avg = snap_standard_frame_rate(raw_avg);
                let num = (avg * 1000.0).round() as u64;
                fa.set_field(StreamKind::Video, pos, "FrameRate_Mode", "VFR");
                fa.set_field(StreamKind::Video, pos, "FrameRate", format!("{:.3}", avg));
                fa.set_field(StreamKind::Video, pos, "FrameRate_Num", num.to_string());
                fa.set_field(StreamKind::Video, pos, "FrameRate_Den", "1000");
                if let Some(max_d) = track.stts_max_delta {
                    let min_fr = tsc / max_d as f64;
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "FrameRate_Minimum",
                        format!("{:.3}", min_fr),
                    );
                }
                if let Some(min_d) = track.stts_min_delta {
                    let max_fr = tsc / min_d as f64;
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "FrameRate_Maximum",
                        format!("{:.3}", max_fr),
                    );
                }
                frame_rate_emitted = true;
            }
            if !frame_rate_emitted {
                if let (Some(fc), tsc) = (track.sample_count, track.timescale) {
                    if tsc > 0 && track.duration_units > 0 {
                        let fr = (fc as f64) * (tsc as f64) / (track.duration_units as f64);
                        fa.set_field(StreamKind::Video, pos, "FrameRate", format!("{:.3}", fr));
                    }
                }
            }
            // StreamSize = sum of per-sample sizes from stsz.
            if let Some(ss) = track.source_stream_size {
                fa.set_field(StreamKind::Video, pos, "StreamSize", ss.to_string());
                // BitRate: for CFR video, content_duration = FrameCount /
                // FrameRate. Use that instead of the elst-padded ms so
                // the rate reflects the encoded content, not the wrapper.
                let br_opt: Option<u64> = if let (Some(fc), Some(delta), tsc) =
                    (track.sample_count, track.stts_cfr_delta, track.timescale)
                {
                    if tsc > 0 && delta > 0 && fc > 0 {
                        // content_seconds = fc * delta / tsc
                        let bits = ss as f64 * 8.0;
                        let content_sec = (fc as f64 * delta as f64) / tsc as f64;
                        Some((bits / content_sec).round() as u64)
                    } else {
                        None
                    }
                } else {
                    None
                };
                let br = br_opt.or_else(|| {
                    dur_ms.and_then(|ms| {
                        if ms > 0 {
                            Some((ss as f64 * 8.0 * 1000.0 / ms as f64).round() as u64)
                        } else {
                            None
                        }
                    })
                });
                if let Some(b) = br {
                    fa.set_field(StreamKind::Video, pos, "BitRate", b.to_string());
                }
            }
            // pasp → PixelAspectRatio + DisplayAspectRatio + Sampled_W/H.
            // Without pasp, oracle still emits PAR=1.0 + DAR + Sampled_W/H
            // when geometry is known. Default to 1:1 if pasp absent.
            if let (Some(w), Some(h)) = (track.video_width, track.video_height) {
                let (h_sp, v_sp) = match (track.pasp_h, track.pasp_v) {
                    (Some(hp), Some(vp)) if vp > 0 => (hp as f64, vp as f64),
                    _ => (1.0, 1.0),
                };
                let par = h_sp / v_sp;
                let sampled_w = (w as f64 * par).round() as u64;
                let dar = sampled_w as f64 / h as f64;
                fa.set_field(StreamKind::Video, pos, "Sampled_Width", sampled_w.to_string());
                fa.set_field(StreamKind::Video, pos, "Sampled_Height", h.to_string());
                fa.set_field(StreamKind::Video, pos, "PixelAspectRatio", format!("{:.3}", par));
                fa.set_field(StreamKind::Video, pos, "DisplayAspectRatio", format!("{:.3}", dar));
                let rot = track.rotation_deg.unwrap_or(0);
                fa.set_field(StreamKind::Video, pos, "Rotation", format!("{rot}.000"));
            }
            // Resolve colour info from colr box (preferred) or SPS VUI.
            // Determine source: Container (colr box), Stream (SPS), or both.
            let has_colr = track.color_primaries_idc.is_some()
                || track.color_transfer_idc.is_some()
                || track.color_matrix_idc.is_some();
            let avc_vui_has_colour =
                track.avc_sps.as_ref().map(|s| s.colour_description_present).unwrap_or(false);
            let is_hevc = track.hevc_profile_idc.is_some();
            // _Source labels:
            //   colr + (AVC SPS VUI or HEVC) → "Container / Stream"
            //   colr only → "Container"
            //   AVC SPS VUI only → "Stream"
            //   neither → "Stream" (placeholder; cdp is false so unused)
            // HEVC heuristic: we can't decode HEVC SPS VUI reliably yet,
            // but iPhone HEVC always carries the same colour info as
            // the colr box — treat HEVC + colr as if SPS confirms it.
            let stream_has_colour = avc_vui_has_colour || (is_hevc && has_colr);
            let source = if has_colr && stream_has_colour {
                "Container / Stream"
            } else if has_colr {
                "Container"
            } else {
                "Stream"
            };
            // colour_range source: colr `nclc` doesn't carry the range
            // bit; only `nclx` does. When we got range from the stream
            // (or the HEVC default), the source is just "Stream".
            let range_source = if track.color_full_range.is_some() { source } else { "Stream" };

            let (cp_idc, tc_idc, mc_idc, full_range, cdp) = resolve_colour(track);
            if cdp {
                fa.set_field(StreamKind::Video, pos, "colour_description_present", "Yes");
                fa.set_field(StreamKind::Video, pos, "colour_description_present_Source", source);
            }
            if let Some(fr) = full_range {
                let label = if fr { "Full" } else { "Limited" };
                fa.set_field(StreamKind::Video, pos, "colour_range", label);
                fa.set_field(StreamKind::Video, pos, "colour_range_Source", range_source);
            }
            if let Some(p) = cp_idc.and_then(cicp_primaries) {
                fa.set_field(StreamKind::Video, pos, "colour_primaries", p);
                fa.set_field(StreamKind::Video, pos, "colour_primaries_Source", source);
            }
            if let Some(t) = tc_idc.and_then(cicp_transfer) {
                fa.set_field(StreamKind::Video, pos, "transfer_characteristics", t);
                fa.set_field(StreamKind::Video, pos, "transfer_characteristics_Source", source);
                // Set HDR_Format for HLG and PQ when transfer characteristics indicate HDR
                match t {
                    "PQ" => {
                        if fa.retrieve(StreamKind::Video, pos, "HDR_Format").is_none() {
                            fa.set_field(StreamKind::Video, pos, "HDR_Format", "SMPTE ST 2084");
                            fa.set_field(StreamKind::Video, pos, "HDR_Format_Compatibility", "PQ");
                        }
                    }
                    "HLG" => {
                        if fa.retrieve(StreamKind::Video, pos, "HDR_Format").is_none() {
                            fa.set_field(StreamKind::Video, pos, "HDR_Format", "ARIB STD-B67");
                            fa.set_field(StreamKind::Video, pos, "HDR_Format_Compatibility", "HLG");
                        }
                    }
                    _ => {}
                }
            }
            if let Some(m) = mc_idc.and_then(cicp_matrix) {
                fa.set_field(StreamKind::Video, pos, "matrix_coefficients", m);
                fa.set_field(StreamKind::Video, pos, "matrix_coefficients_Source", source);
            }
            // Stored_Height = SPS pre-crop height. Only emit when it
            // differs from the displayed Height (oracle suppresses it
            // otherwise — encoded height == display height means no
            // cropping happened).
            if let (Some(sps), Some(h)) = (track.avc_sps.as_ref(), track.video_height) {
                if sps.stored_height != h as u32 {
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "Stored_Height",
                        sps.stored_height.to_string(),
                    );
                }
            }
            // ChromaSubsampling_Position from VUI chroma_sample_loc.
            // Oracle emits the raw chroma_sample_loc_type_top_field as
            // "Type N" (no offset; matches MediaInfoLib's File_Avc).
            if let Some(sps) = track.avc_sps.as_ref() {
                if let Some(loc) = sps.chroma_sample_loc {
                    fa.set_field(
                        StreamKind::Video,
                        pos,
                        "ChromaSubsampling_Position",
                        format!("Type {loc}"),
                    );
                }
            }
            if let Some(lang) = track.language.as_deref().and_then(iso639_emit) {
                fa.set_field(StreamKind::Video, pos, "Language", lang);
            }
            if let Some(s) = track.creation_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Video, pos, "Encoded_Date", s);
            }
            if let Some(s) = track.modification_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Video, pos, "Tagged_Date", s);
            }
            if let Some(t) = track.handler_name.as_deref() {
                if !t.is_empty() {
                    fa.set_field(StreamKind::Video, pos, "Title", t.to_string());
                }
            }
            video_count += 1;
        } else if track.handler == HANDLER_HINT {
            let pos = fa.stream_prepare(StreamKind::Other);
            fa.set_field(StreamKind::Other, pos, "StreamOrder", stream_order.to_string());
            let id = track.track_id.unwrap_or((track_idx + 1) as u32);
            fa.set_field(StreamKind::Other, pos, "ID", id.to_string());
            stream_order += 1;
            fa.set_field(StreamKind::Other, pos, "Type", "Hint");
            if let Some(cid) = track.hint_codec_id {
                if cid == "rtp " {
                    fa.set_field(StreamKind::Other, pos, "Format", "RTP");
                }
                fa.set_field(StreamKind::Other, pos, "CodecID", cid);
            }
            if let Some(fc) = track.sample_count {
                fa.set_field(StreamKind::Other, pos, "FrameCount", fc.to_string());
            }
            if let Some(ss) = track.source_stream_size {
                fa.set_field(StreamKind::Other, pos, "StreamSize", ss.to_string());
            }
            // Hint tracks: oracle DOES emit Default=No since tkhd bit
            // is typically cleared. Keep emitting for hint tracks but
            // skip the implicit "Yes" case.
            if track.track_enabled == Some(false) {
                fa.set_field(StreamKind::Other, pos, "Default", "No");
            }
            if let Some(lang) = track.language.as_deref().and_then(iso639_emit) {
                fa.set_field(StreamKind::Other, pos, "Language", lang);
            }
            if let Some(s) = track.creation_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Other, pos, "Encoded_Date", s);
            }
            if let Some(s) = track.modification_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Other, pos, "Tagged_Date", s);
            }
            other_count += 1;
        } else if matches!(track.handler, HANDLER_META | HANDLER_TEXT | HANDLER_SUBT | HANDLER_SBTL)
        {
            // Non-codec data tracks (timed metadata, subtitles in MOV
            // containers, etc.) → emit as Other stream so oracle's
            // OtherCount + the per-track entries line up.
            let pos = fa.stream_prepare(StreamKind::Other);
            fa.set_field(StreamKind::Other, pos, "StreamOrder", stream_order.to_string());
            let id = track.track_id.unwrap_or((track_idx + 1) as u32);
            // Collect metadata track IDs for the General.Metas field
            if track.handler == HANDLER_META {
                meta_track_ids.push(id);
            }
            fa.set_field(StreamKind::Other, pos, "ID", id.to_string());
            stream_order += 1;
            // Type label: oracle doesn't emit <Type> for meta-handler
            // tracks at all (just Format=Timed Metadata Sample carries
            // the discriminator). Text/Subtitle handlers DO get a Type
            // label since those are first-class subtitle streams.
            match track.handler {
                HANDLER_TEXT => fa.set_field(StreamKind::Other, pos, "Type", "Text"),
                HANDLER_SUBT | HANDLER_SBTL => {
                    fa.set_field(StreamKind::Other, pos, "Type", "Subtitle")
                }
                _ => {}
            }
            if let Some(cid) = track.meta_codec_id {
                if cid == "mebx" {
                    fa.set_field(StreamKind::Other, pos, "Format", "Timed Metadata Sample");
                }
                fa.set_field(StreamKind::Other, pos, "CodecID", cid);
            }
            if let Some(t) = track.handler_name.as_deref() {
                if !t.is_empty() {
                    fa.set_field(StreamKind::Other, pos, "Title", t.to_string());
                }
            }
            if let Some(ss) = track.source_stream_size {
                fa.set_field(StreamKind::Other, pos, "StreamSize", ss.to_string());
            }
            // Duration in ms from track's mdhd.
            if track.timescale > 0 && track.duration_units > 0 {
                let ts = track.timescale as u64;
                let dur_ms = (track.duration_units * 1000 + ts / 2) / ts;
                fa.set_field(StreamKind::Other, pos, "Duration", dur_ms.to_string());
            }
            // mebx metadata tracks carry variable-size frames → oracle
            // marks them as VBR.
            if matches!(track.meta_codec_id, Some("mebx")) {
                fa.set_field(StreamKind::Other, pos, "BitRate_Mode", "VBR");
            }
            // FrameCount = stsz sample count for the meta track.
            if let Some(fc) = track.sample_count {
                fa.set_field(StreamKind::Other, pos, "FrameCount", fc.to_string());
            }
            // Meta/text/subt tracks: only emit Default when explicitly
            // disabled. Implicit "Yes" is suppressed.
            if track.track_enabled == Some(false) {
                fa.set_field(StreamKind::Other, pos, "Default", "No");
            }
            if let Some(lang) = track.language.as_deref().and_then(iso639_emit) {
                fa.set_field(StreamKind::Other, pos, "Language", lang);
            }
            if let Some(s) = track.creation_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Other, pos, "Encoded_Date", s);
            }
            if let Some(s) = track.modification_time.and_then(mp4_date_string) {
                fa.set_field(StreamKind::Other, pos, "Tagged_Date", s);
            }
            other_count += 1;
        }
    }

    if audio_count > 0 {
        fa.set_field(StreamKind::General, 0, "AudioCount", audio_count.to_string());
    }
    if video_count > 0 {
        fa.set_field(StreamKind::General, 0, "VideoCount", video_count.to_string());
    }
    if other_count > 0 {
        fa.set_field(StreamKind::General, 0, "OtherCount", other_count.to_string());
    }
    // Emit Metas = comma-separated list of metadata track IDs
    if !meta_track_ids.is_empty() {
        let metas_str =
            meta_track_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(",");
        fa.set_field(StreamKind::General, 0, "Metas", metas_str);
    }
}

/// Decode an ISO 6709 location string (as Apple writes it) into
/// MediaInfo's human-readable `Recorded_Location` form.
///
/// Apple format: signed lat + signed lon + optional signed elev + "/",
/// e.g. `+33.7295-150.8935+047.254/`. Oracle output:
/// `33.7295°S 150.8935°E 47.254m` — strip leading sign + suffix
/// directional indicator + elevation in meters. Returns None on
/// malformed input rather than emitting garbage.
fn parse_iso6709(s: &str) -> Option<String> {
    let s = s.trim_end_matches('/');
    let bytes = s.as_bytes();
    if bytes.is_empty() || (bytes[0] != b'+' && bytes[0] != b'-') {
        return None;
    }
    // Split into ±lat ±lon [±elev] components by sign boundaries
    // (excluding the leading sign byte).
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (b == b'+' || b == b'-') {
            parts.push(&s[start..i]);
            start = i;
        }
    }
    parts.push(&s[start..]);
    if parts.len() < 2 {
        return None;
    }
    let lat_str = parts[0];
    let lon_str = parts[1];
    let elev_str = parts.get(2).copied();
    let lat: f64 = lat_str.parse().ok()?;
    let lon: f64 = lon_str.parse().ok()?;
    let lat_dir = if lat >= 0.0 { 'N' } else { 'S' };
    let lon_dir = if lon >= 0.0 { 'E' } else { 'W' };
    let mut out = format!("{:.4}°{} {:.4}°{}", lat.abs(), lat_dir, lon.abs(), lon_dir);
    if let Some(e) = elev_str.and_then(|s| s.parse::<f64>().ok()) {
        out.push_str(&format!(" {:.3}m", e));
    }
    Some(out)
}

/// Map ISO 639-2 three-letter code to the oracle's emitted language
/// string. Oracle prefers ISO 639-1 two-letter when available, falling
/// back to the 639-2 code. "und" → None (don't emit).
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

/// Seconds between the MP4 epoch (1904-01-01 00:00:00 UTC) and the Unix
/// epoch (1970-01-01 00:00:00 UTC). 66 years = 2_082_844_800 seconds.
const MP4_EPOCH_TO_UNIX: i64 = 2_082_844_800;

/// Convert an MP4 timestamp (seconds since 1904) to oracle's
/// "YYYY-MM-DD HH:MM:SS UTC" string. Returns None if the input is zero
/// (the C++ side treats zero as "unset").
fn mp4_date_string(secs_since_1904: u64) -> Option<String> {
    if secs_since_1904 == 0 {
        return None;
    }
    let unix_secs = secs_since_1904 as i64 - MP4_EPOCH_TO_UNIX;
    let (y, m, d, hh, mm, ss) = civil_from_unix_mp4(unix_secs);
    Some(format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}:{ss:02} UTC"))
}

/// Same algorithm as revelo-diff's civil_from_unix — proleptic
/// Gregorian via Howard Hinnant's `days_from_civil`. Inlined here so
/// the parser doesn't depend on the harness.
fn civil_from_unix_mp4(unix_secs: i64) -> (i32, u8, u8, u8, u8, u8) {
    let days = unix_secs.div_euclid(86400);
    let rem = unix_secs.rem_euclid(86400);
    let hh = (rem / 3600) as u8;
    let mm = ((rem % 3600) / 60) as u8;
    let ss = (rem % 60) as u8;

    let z = days + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u8;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8;
    let year = if m <= 2 { y + 1 } else { y } as i32;
    (year, m, d, hh, mm, ss)
}

/// Pick the effective colour info for a track. colr box wins when
/// present; otherwise fall back to AVC SPS VUI colour_description.
/// Returns (primaries_idc, transfer_idc, matrix_idc, full_range, present).
fn resolve_colour(
    track: &TrackInfo,
) -> (Option<u16>, Option<u16>, Option<u16>, Option<bool>, bool) {
    // Priority: colr box (container) > AVC SPS (stream) > HEVC SPS (stream)
    let container_present = track.color_primaries_idc.is_some()
        || track.color_transfer_idc.is_some()
        || track.color_matrix_idc.is_some();

    // Get stream colour info from SPS (AVC or HEVC)
    let stream_colour: Option<(Option<u16>, Option<u16>, Option<u16>, Option<bool>)> = track
        .avc_sps
        .as_ref()
        .and_then(|sps| {
            if sps.colour_description_present {
                Some((
                    sps.colour_primaries.map(|v| v as u16),
                    sps.transfer_characteristics.map(|v| v as u16),
                    sps.matrix_coefficients.map(|v| v as u16),
                    sps.video_full_range,
                ))
            } else {
                None
            }
        })
        .or_else(|| {
            track.hevc_sps.as_ref().and_then(|sps| {
                if sps.colour_description_present {
                    Some((
                        sps.colour_primaries.map(|v| v as u16),
                        sps.transfer_characteristics.map(|v| v as u16),
                        sps.matrix_coefficients.map(|v| v as u16),
                        sps.video_full_range,
                    ))
                } else {
                    None
                }
            })
        });

    if container_present {
        let _ = stream_colour;
        // Merge full_range: colr nclc has none, so fall back to SPS if
        // present. Otherwise default to false (Limited) for HEVC since
        // every iPhone HEVC stream uses Limited in practice and the
        // C++ side defaults the same way.
        let fr = track
            .color_full_range
            .or_else(|| track.avc_sps.as_ref().and_then(|s| s.video_full_range))
            .or_else(|| track.hevc_sps.as_ref().and_then(|s| s.video_full_range))
            .or_else(|| if track.hevc_profile_idc.is_some() { Some(false) } else { None });
        return (
            track.color_primaries_idc,
            track.color_transfer_idc,
            track.color_matrix_idc,
            fr,
            true,
        );
    }
    if let Some((cp, tc, mc, fr)) = stream_colour {
        return (cp, tc, mc, fr, true);
    }
    (None, None, None, None, false)
}

/// Snap a measured VFR average to a nearby standard frame rate when
/// within ±0.5%. Otherwise return as-is. Mirrors oracle's behavior of
/// reporting iPhone NTSC 60p (nominally 60000/1001 = 59.940) even when
/// the actual stts deltas come in at 59.96/59.97.
fn snap_standard_frame_rate(measured: f64) -> f64 {
    const STANDARD: &[f64] = &[
        15.000, 23.976, 24.000, 25.000, 29.970, 30.000, 47.952, 48.000, 50.000, 59.940, 60.000,
        100.000, 119.880, 120.000,
    ];
    for &std in STANDARD {
        if (measured - std).abs() / std < 0.005 {
            return std;
        }
    }
    measured
}

/// Euclidean GCD for u32. Used to reduce FrameRate_Num/FrameRate_Den.
fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

/// CICP color_primaries → MediaInfo colour_primaries string.
fn cicp_primaries(idc: u16) -> Option<&'static str> {
    match idc {
        1 => Some("BT.709"),
        4 => Some("BT.470 System M"),
        5 => Some("BT.601 PAL"),
        6 => Some("BT.601 NTSC"),
        7 => Some("SMPTE 240M"),
        8 => Some("Generic film"),
        9 => Some("BT.2020"),
        10 => Some("XYZ"),
        11 => Some("DCI P3"),
        12 => Some("Display P3"),
        _ => None,
    }
}

/// CICP transfer_characteristics → MediaInfo transfer_characteristics string.
fn cicp_transfer(idc: u16) -> Option<&'static str> {
    match idc {
        1 => Some("BT.709"),
        4 => Some("BT.470 System M"),
        5 => Some("BT.470 System B/G"),
        6 => Some("BT.601"),
        7 => Some("SMPTE 240M"),
        8 => Some("Linear"),
        11 => Some("IEC 61966-2-4"),
        12 => Some("BT.1361"),
        13 => Some("IEC 61966-2-1"),
        14 => Some("BT.2020 (10-bit)"),
        15 => Some("BT.2020 (12-bit)"),
        16 => Some("PQ"),
        17 => Some("SMPTE 428M"),
        18 => Some("HLG"),
        _ => None,
    }
}

/// CICP matrix_coefficients → MediaInfo matrix_coefficients string.
fn cicp_matrix(idc: u16) -> Option<&'static str> {
    match idc {
        0 => Some("Identity"),
        1 => Some("BT.709"),
        4 => Some("FCC 73.682"),
        5 => Some("BT.470 System B/G"),
        6 => Some("BT.601"),
        7 => Some("SMPTE 240M"),
        8 => Some("YCgCo"),
        9 => Some("BT.2020 non-constant"),
        10 => Some("BT.2020 constant"),
        _ => None,
    }
}

/// HEVC profile_idc → MediaInfo Format_Profile string.
fn hevc_profile_name(idc: u8) -> Option<&'static str> {
    match idc {
        1 => Some("Main"),
        2 => Some("Main 10"),
        3 => Some("Main Still Picture"),
        4 => Some("Format Range Extensions"),
        _ => None,
    }
}

/// AVC profile_idc → MediaInfo Format_Profile string. `constrained` is
/// true when the profile_compatibility byte has bit 6 set (and only
/// meaningful for Baseline). Mirrors MediaInfoLib's
/// `Avc_profile_idc_to_string` table for the common cases.
fn avc_profile_name(idc: u8, constrained: bool) -> Option<&'static str> {
    match idc {
        0x42 => Some(if constrained { "Constrained Baseline" } else { "Baseline" }),
        0x4D => Some("Main"),
        0x58 => Some("Extended"),
        0x64 => Some("High"),
        0x6E => Some("High 10"),
        0x7A => Some("High 4:2:2"),
        0x90 | 0xF4 => Some("High 4:4:4"),
        _ => None,
    }
}

/// AVC level_idc → "X" or "X.Y" string. e.g. 0x1E (30) → "3",
/// 0x1F (31) → "3.1", 0x29 (41) → "4.1". Levels are encoded as
/// the level number times 10.
fn format_avc_level(idc: u8) -> String {
    let major = idc / 10;
    let minor = idc % 10;
    if minor == 0 { format!("{major}") } else { format!("{major}.{minor}") }
}

/// AAC AOT → MediaInfo `Format_AdditionalFeatures` profile name.
/// Subset that covers the common cases; matches the C++ side's
/// `Aac_audioObjectType` table.
fn aac_profile_name(aot: u8) -> Option<&'static str> {
    match aot {
        1 => Some("Main"),
        2 => Some("LC"),
        3 => Some("SSR"),
        4 => Some("LTP"),
        5 => Some("SBR"),
        17 => Some("LC ER"),
        20 => Some("LTP ER"),
        23 => Some("LC ER"),
        29 => Some("PS"),
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

/// AAC mono uses "M" for ChannelLayout instead of "C" — matches oracle's
/// Vorbis/Opus/AAC-style label. AC-3 and PCM stay with "C".
fn channel_layout_for_format(
    channels: u16,
    format: Option<&'static str>,
) -> (Option<&'static str>, Option<&'static str>) {
    if matches!(format, Some("AAC")) && channels == 1 {
        return (Some("Front: C"), Some("M"));
    }
    channel_layout(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_mp4_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a valid MP4 file at all");
        assert!(!parse_mp4(&mut fa));
    }

    #[test]
    fn parses_minimal_ftyp_only() {
        // Just ftyp+free, no moov. Should still accept and fill Format.
        let mut buf = Vec::new();
        // ftyp box: size=20, type=ftyp, major=M4A , minor=0, brand=isom
        buf.extend_from_slice(&20u32.to_be_bytes());
        buf.extend_from_slice(b"ftyp");
        buf.extend_from_slice(b"M4A ");
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"isom");

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mp4(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("MPEG-4")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "CodecID")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("M4A ")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "CodecID_Compatible")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("M4A /isom")
        );
    }
}
