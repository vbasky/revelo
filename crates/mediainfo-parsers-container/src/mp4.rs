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

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::{int32u, int64u};

const BOX_FTYP: int32u = u32::from_be_bytes(*b"ftyp");
const BOX_MOOV: int32u = u32::from_be_bytes(*b"moov");
const BOX_TRAK: int32u = u32::from_be_bytes(*b"trak");
const BOX_MDIA: int32u = u32::from_be_bytes(*b"mdia");
const BOX_MDHD: int32u = u32::from_be_bytes(*b"mdhd");
const BOX_HDLR: int32u = u32::from_be_bytes(*b"hdlr");
const BOX_MINF: int32u = u32::from_be_bytes(*b"minf");
const BOX_STBL: int32u = u32::from_be_bytes(*b"stbl");
const BOX_STSD: int32u = u32::from_be_bytes(*b"stsd");
const BOX_STSZ: int32u = u32::from_be_bytes(*b"stsz");
const BOX_MVHD: int32u = u32::from_be_bytes(*b"mvhd");
const BOX_EDTS: int32u = u32::from_be_bytes(*b"edts");
const BOX_ELST: int32u = u32::from_be_bytes(*b"elst");
const BOX_UDTA: int32u = u32::from_be_bytes(*b"udta");
const BOX_META: int32u = u32::from_be_bytes(*b"meta");
const BOX_ILST: int32u = u32::from_be_bytes(*b"ilst");
const BOX_DATA: int32u = u32::from_be_bytes(*b"data");
const BOX_MDAT: int32u = u32::from_be_bytes(*b"mdat");

/// iTunes-style `©too` (tool/encoder) metadata key.
const ITUNES_KEY_TOOL: int32u = 0xA9_74_6F_6F;

const HANDLER_SOUN: int32u = u32::from_be_bytes(*b"soun");
const HANDLER_VIDE: int32u = u32::from_be_bytes(*b"vide");

const SAMPLE_ENTRY_MP4A: int32u = u32::from_be_bytes(*b"mp4a");
const BOX_ESDS: int32u = u32::from_be_bytes(*b"esds");

#[derive(Debug, Default)]
struct MovieInfo {
    timescale: u32,
    duration: u64,
    /// iTunes-style metadata items from udta > meta > ilst. Keyed by
    /// the 4-byte item type code (e.g. `©too` = 0xA9_74_6F_6F).
    itunes_metadata: Vec<(int32u, String)>,
}

#[derive(Debug, Default)]
struct TrackInfo {
    handler: int32u,
    timescale: u32,
    duration_units: u64,
    /// Effective playback duration in movie-timescale units (from
    /// first elst entry's segment_duration), if any edit list present.
    elst_segment_duration: Option<u64>,
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
    /// Sum of all per-sample sizes from `stsz` — raw byte count
    /// before any edit-list trimming.
    source_stream_size: Option<u64>,
    has_data: bool,
}

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
    let file_size = fa.Remain();

    walk_boxes(fa, file_size, 0, &mut |fa, box_type, box_size, box_start, depth| {
        match box_type {
            BOX_FTYP => {
                ftyp_brands = parse_ftyp(fa, box_size);
            }
            BOX_MDAT => {
                if mdat_offset.is_none() {
                    mdat_offset = Some(box_start);
                    mdat_size = Some(box_size);
                }
                fa.Skip_Hexa(box_size.saturating_sub(8), "mdat_body");
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
                fa.Skip_Hexa(box_size.saturating_sub(8), "BoxBody");
            }
        }
    });

    let layout = BoxLayout {
        file_size,
        mdat_offset,
        mdat_size,
        moov_offset,
    };
    fill_streams(fa, &ftyp_brands, &tracks, &movie, &layout);
    true
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
    visit: &mut dyn FnMut(&mut FileAnalyze, int32u, usize, usize, usize),
) {
    let region_end = fa.Element_Offset() + region_size;
    while fa.Element_Offset() + 8 <= region_end && fa.Remain() >= 8 {
        let start = fa.Element_Offset();
        let mut size32: int32u = 0;
        fa.Get_B4(&mut size32, "Size");
        let mut box_type: int32u = 0;
        fa.Get_C4(&mut box_type, "Type");

        let total_size: usize = if size32 == 1 {
            let mut size64: int64u = 0;
            fa.Get_B8(&mut size64, "Size64");
            size64 as usize
        } else if size32 == 0 {
            // Box extends to end of file
            region_end - start
        } else {
            size32 as usize
        };

        let header_size = fa.Element_Offset() - start;
        let body_size = total_size.saturating_sub(header_size);

        // Save position to ensure we always advance exactly `body_size`.
        let body_start = fa.Element_Offset();
        let body_end = body_start + body_size;
        if body_end > start + region_size && depth == 0 {
            // Truncated final box at top level — stop.
            break;
        }

        // Call the visitor — the visitor may either consume the body
        // (Skip_Hexa) or recurse into sub-boxes. To stay safe, snap the
        // position back to body_end after the visitor returns.
        fa.Element_Begin(box_type_name(box_type).as_str());
        let _name = box_type_name(box_type);
        visit(fa, box_type, total_size, start, depth);
        fa.Element_End();
        if fa.Element_Offset() < body_end {
            fa.Skip_Hexa(body_end - fa.Element_Offset(), "BoxTail");
        } else if fa.Element_Offset() > body_end {
            // Visitor over-consumed — should not happen, but bail if it does.
            break;
        }
    }
}

fn box_type_name(t: int32u) -> String {
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
        fa.Skip_Hexa(body_size, "ftyp");
        return brands;
    }
    let mut major: int32u = 0;
    fa.Get_C4(&mut major, "major_brand");
    brands.push(four_cc_str(major));
    let mut minor: int32u = 0;
    fa.Get_B4(&mut minor, "minor_version");
    let remain = body_size - 8;
    let count = remain / 4;
    for _ in 0..count {
        let mut b: int32u = 0;
        fa.Get_C4(&mut b, "compatible_brand");
        brands.push(four_cc_str(b));
    }
    // Skip any tail bytes that aren't a multiple of 4.
    if remain % 4 != 0 {
        fa.Skip_Hexa(remain % 4, "ftyp_tail");
    }
    brands
}

fn four_cc_str(v: int32u) -> String {
    let bytes = v.to_be_bytes();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn handle_inner(
    fa: &mut FileAnalyze,
    box_type: int32u,
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
            // meta has a 4-byte version_flags header before its
            // nested children (hdlr, keys, ilst, etc). We don't
            // recurse through handle_inner here because meta's hdlr
            // declares the metadata namespace (e.g. "mdir" for
            // iTunes) — not the track-level audio/video handler —
            // and feeding it to parse_hdlr would clobber the audio
            // track's handler classification. Dispatch only ilst.
            let inner = box_size.saturating_sub(8);
            if inner < 4 {
                fa.Skip_Hexa(inner, "meta_short");
            } else {
                fa.Skip_Hexa(4, "version_flags");
                walk_boxes(fa, inner - 4, 1, &mut |fa, t, s, _, _| match t {
                    BOX_ILST => parse_ilst(fa, s, movie),
                    _ => fa.Skip_Hexa(s.saturating_sub(8), "meta_child"),
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
        BOX_MDHD => {
            if let Some(track) = tracks.last_mut() {
                parse_mdhd(fa, box_size, track);
            } else {
                fa.Skip_Hexa(box_size.saturating_sub(8), "mdhd");
            }
        }
        BOX_HDLR => {
            if let Some(track) = tracks.last_mut() {
                parse_hdlr(fa, box_size, track);
            } else {
                fa.Skip_Hexa(box_size.saturating_sub(8), "hdlr");
            }
        }
        BOX_STSD => {
            if let Some(track) = tracks.last_mut() {
                parse_stsd(fa, box_size, track);
            } else {
                fa.Skip_Hexa(box_size.saturating_sub(8), "stsd");
            }
        }
        BOX_STSZ => {
            if let Some(track) = tracks.last_mut() {
                parse_stsz(fa, box_size, track);
            } else {
                fa.Skip_Hexa(box_size.saturating_sub(8), "stsz");
            }
        }
        BOX_ELST => {
            if let Some(track) = tracks.last_mut() {
                parse_elst(fa, box_size, track);
            } else {
                fa.Skip_Hexa(box_size.saturating_sub(8), "elst");
            }
        }
        _ => {
            fa.Skip_Hexa(box_size.saturating_sub(8), "BoxBody");
        }
    }
}

/// Parse mvhd v0/v1 — we only need the `timescale` field for converting
/// elst.segment_duration into seconds.
fn parse_mvhd(fa: &mut FileAnalyze, box_size: usize, movie: &mut MovieInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 16 {
        fa.Skip_Hexa(body_size, "mvhd");
        return;
    }
    let start = fa.Element_Offset();
    let mut version_flags: int32u = 0;
    fa.Get_B4(&mut version_flags, "version_flags");
    let version = (version_flags >> 24) as u8;
    if version == 1 {
        fa.Skip_Hexa(8, "creation_time");
        fa.Skip_Hexa(8, "modification_time");
        let mut ts: int32u = 0;
        fa.Get_B4(&mut ts, "timescale");
        movie.timescale = ts;
        let mut dur: zenlib::int64u = 0;
        fa.Get_B8(&mut dur, "duration");
        movie.duration = dur;
    } else {
        fa.Skip_Hexa(4, "creation_time");
        fa.Skip_Hexa(4, "modification_time");
        let mut ts: int32u = 0;
        fa.Get_B4(&mut ts, "timescale");
        movie.timescale = ts;
        let mut dur: int32u = 0;
        fa.Get_B4(&mut dur, "duration");
        movie.duration = dur as u64;
    }

    let consumed = fa.Element_Offset() - start;
    if consumed < body_size {
        fa.Skip_Hexa(body_size - consumed, "mvhd_tail");
    }
}

/// Parse the iTunes `ilst` (item list) box. Each item is a `[size][4cc]`
/// box containing a nested `data` box. Captures UTF-8 string items
/// indexed by their 4cc type.
fn parse_ilst(fa: &mut FileAnalyze, box_size: usize, movie: &mut MovieInfo) {
    let inner = box_size.saturating_sub(8);
    let end = fa.Element_Offset() + inner;
    while fa.Element_Offset() + 8 <= end {
        let item_start = fa.Element_Offset();
        let mut item_size: int32u = 0;
        fa.Get_B4(&mut item_size, "item_size");
        let mut item_type: int32u = 0;
        fa.Get_C4(&mut item_type, "item_type");
        let item_total = item_size as usize;
        if item_total < 8 || item_start + item_total > end {
            break;
        }
        let body = item_total - 8;
        let body_end = fa.Element_Offset() + body;

        // Inside each item is a `data` box (and maybe other helpers).
        while fa.Element_Offset() + 8 <= body_end {
            let mut sub_size: int32u = 0;
            fa.Get_B4(&mut sub_size, "sub_size");
            let mut sub_type: int32u = 0;
            fa.Get_C4(&mut sub_type, "sub_type");
            let sub_total = sub_size as usize;
            if sub_total < 8 || sub_total > (body_end - fa.Element_Offset() + 8) {
                break;
            }
            let sub_body = sub_total - 8;
            if sub_type == BOX_DATA && sub_body >= 8 {
                // data box: 4 bytes type_indicator, 4 bytes locale, then payload.
                let mut type_indicator: int32u = 0;
                fa.Get_B4(&mut type_indicator, "type_indicator");
                fa.Skip_Hexa(4, "locale");
                let payload_size = sub_body - 8;
                let payload = fa.read_raw(payload_size).to_vec();
                if type_indicator == 1 {
                    let s = String::from_utf8_lossy(&payload).into_owned();
                    movie.itunes_metadata.push((item_type, s));
                }
            } else {
                fa.Skip_Hexa(sub_body, "sub_body");
            }
        }
        // Pin to item boundary in case the loop above bailed.
        if fa.Element_Offset() < body_end {
            fa.Skip_Hexa(body_end - fa.Element_Offset(), "item_tail");
        }
    }
    if fa.Element_Offset() < end {
        fa.Skip_Hexa(end - fa.Element_Offset(), "ilst_tail");
    }
}

/// Parse elst — capture only the first entry's segment_duration
/// (sufficient for typical single-entry edit lists used to trim AAC
/// encoder priming).
fn parse_elst(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 8 {
        fa.Skip_Hexa(body_size, "elst");
        return;
    }
    let start = fa.Element_Offset();
    let mut version_flags: int32u = 0;
    fa.Get_B4(&mut version_flags, "version_flags");
    let version = (version_flags >> 24) as u8;
    let mut entry_count: int32u = 0;
    fa.Get_B4(&mut entry_count, "entry_count");

    if entry_count > 0 {
        let segment_duration: u64 = if version == 1 {
            let mut v: zenlib::int64u = 0;
            fa.Get_B8(&mut v, "segment_duration");
            v
        } else {
            let mut v: int32u = 0;
            fa.Get_B4(&mut v, "segment_duration");
            v as u64
        };
        // Skip remaining fields of this entry + rest of entries.
        let entry_size = if version == 1 { 20 } else { 12 };
        let consumed_in_entry = if version == 1 { 8 } else { 4 };
        if entry_size > consumed_in_entry {
            fa.Skip_Hexa(entry_size - consumed_in_entry, "entry_remainder");
        }
        let other_entries = entry_count.saturating_sub(1) as usize;
        if other_entries > 0 {
            fa.Skip_Hexa(other_entries * entry_size, "other_entries");
        }
        track.elst_segment_duration = Some(segment_duration);
    }

    let consumed = fa.Element_Offset() - start;
    if consumed < body_size {
        fa.Skip_Hexa(body_size - consumed, "elst_tail");
    }
}

fn parse_mdhd(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 4 {
        fa.Skip_Hexa(body_size, "mdhd");
        return;
    }
    let start = fa.Element_Offset();
    let mut version_flags: int32u = 0;
    fa.Get_B4(&mut version_flags, "version_flags");
    let version = (version_flags >> 24) as u8;

    if version == 1 {
        let mut _created: int64u = 0;
        fa.Get_B8(&mut _created, "creation_time");
        let mut _modified: int64u = 0;
        fa.Get_B8(&mut _modified, "modification_time");
        let mut ts: int32u = 0;
        fa.Get_B4(&mut ts, "timescale");
        let mut dur: int64u = 0;
        fa.Get_B8(&mut dur, "duration");
        track.timescale = ts;
        track.duration_units = dur;
    } else {
        let mut _created: int32u = 0;
        fa.Get_B4(&mut _created, "creation_time");
        let mut _modified: int32u = 0;
        fa.Get_B4(&mut _modified, "modification_time");
        let mut ts: int32u = 0;
        fa.Get_B4(&mut ts, "timescale");
        let mut dur: int32u = 0;
        fa.Get_B4(&mut dur, "duration");
        track.timescale = ts;
        track.duration_units = dur as u64;
    }

    let consumed = fa.Element_Offset() - start;
    if consumed < body_size {
        fa.Skip_Hexa(body_size - consumed, "mdhd_tail");
    }
}

fn parse_hdlr(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 12 {
        fa.Skip_Hexa(body_size, "hdlr");
        return;
    }
    let start = fa.Element_Offset();
    fa.Skip_Hexa(4, "version_flags");
    fa.Skip_Hexa(4, "pre_defined");
    let mut handler: int32u = 0;
    fa.Get_C4(&mut handler, "handler_type");
    track.handler = handler;
    let consumed = fa.Element_Offset() - start;
    if consumed < body_size {
        fa.Skip_Hexa(body_size - consumed, "hdlr_tail");
    }
}

fn parse_stsd(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 8 {
        fa.Skip_Hexa(body_size, "stsd");
        return;
    }
    let start = fa.Element_Offset();
    fa.Skip_Hexa(4, "version_flags");
    let mut entry_count: int32u = 0;
    fa.Get_B4(&mut entry_count, "entry_count");

    // Walk the first sample entry only (sufficient for single-codec tracks).
    if entry_count > 0 && fa.Remain() >= 8 {
        let entry_start = fa.Element_Offset();
        let mut entry_size: int32u = 0;
        fa.Get_B4(&mut entry_size, "entry_size");
        let mut entry_type: int32u = 0;
        fa.Get_C4(&mut entry_type, "entry_type");
        let entry_total = entry_size as usize;

        match entry_type {
            SAMPLE_ENTRY_MP4A => parse_mp4a_entry(fa, entry_total, track),
            _ => {
                // Unknown sample entry — record nothing, just skip.
                fa.Skip_Hexa(entry_total.saturating_sub(8), "unknown_sample_entry");
            }
        }
        let entry_end = entry_start + entry_total;
        if fa.Element_Offset() < entry_end {
            fa.Skip_Hexa(entry_end - fa.Element_Offset(), "entry_tail");
        }
    }

    let consumed = fa.Element_Offset() - start;
    if consumed < body_size {
        fa.Skip_Hexa(body_size - consumed, "stsd_tail");
    }
}

fn parse_mp4a_entry(fa: &mut FileAnalyze, entry_total: usize, track: &mut TrackInfo) {
    // mp4a sample entry layout (after 4-byte size + 4-byte type already consumed):
    //   6 bytes reserved
    //   2 bytes data_reference_index
    //   8 bytes reserved (version, revision, vendor)
    //   2 bytes channel_count
    //   2 bytes sample_size
    //   2 bytes pre_defined
    //   2 bytes reserved
    //   4 bytes sample_rate (16.16 fixed point — high 16 bits = integer Hz)
    //   optional: esds box
    if entry_total < 8 + 28 {
        fa.Skip_Hexa(entry_total.saturating_sub(8), "mp4a_short");
        return;
    }
    let start_remain = entry_total - 8;
    fa.Skip_Hexa(6, "reserved");
    fa.Skip_Hexa(2, "data_reference_index");
    fa.Skip_Hexa(8, "reserved_v");
    let mut channel_count_u16: zenlib::int16u = 0;
    fa.Get_B2(&mut channel_count_u16, "channel_count");
    let channel_count = channel_count_u16 as u32;
    let mut sample_size: zenlib::int16u = 0;
    fa.Get_B2(&mut sample_size, "sample_size");
    let _ = sample_size;
    fa.Skip_Hexa(2, "pre_defined");
    fa.Skip_Hexa(2, "reserved");
    let mut sr_fixed: int32u = 0;
    fa.Get_B4(&mut sr_fixed, "sample_rate_16.16");
    let sample_rate = (sr_fixed >> 16) as u32;

    track.audio_channels = Some(channel_count as u16);
    track.audio_sample_rate = Some(sample_rate);
    track.audio_format = Some("AAC");
    track.has_data = true;

    let consumed = 28;
    let mut remaining = start_remain.saturating_sub(consumed);
    // Walk inner extension boxes (most importantly: esds).
    while remaining >= 8 {
        let mut sub_size: int32u = 0;
        fa.Get_B4(&mut sub_size, "ext_size");
        let mut sub_type: int32u = 0;
        fa.Get_C4(&mut sub_type, "ext_type");
        let sub_total = sub_size as usize;
        if sub_total < 8 || sub_total > remaining {
            // Malformed — bail to outer skip.
            break;
        }
        let body = sub_total - 8;
        match sub_type {
            BOX_ESDS => parse_esds(fa, body, track),
            _ => fa.Skip_Hexa(body, "ext_unknown"),
        }
        remaining -= sub_total;
    }
    if remaining > 0 {
        fa.Skip_Hexa(remaining, "mp4a_tail");
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
fn parse_esds(fa: &mut FileAnalyze, body_size: usize, track: &mut TrackInfo) {
    let start = fa.Element_Offset();
    let end = start + body_size;
    if body_size < 4 {
        fa.Skip_Hexa(body_size, "esds_short");
        return;
    }
    fa.Skip_Hexa(4, "version_flags");

    // ES_Descriptor (tag 0x03) — walk descriptors until we find the
    // decoder config + its DecoderSpecificInfo child.
    parse_descriptor_chain(fa, end - fa.Element_Offset(), track);

    if fa.Element_Offset() < end {
        fa.Skip_Hexa(end - fa.Element_Offset(), "esds_tail");
    }
}

/// Read a single MPEG-4 BER-style descriptor length (1-4 bytes,
/// each with a continuation bit in the high bit).
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

fn parse_descriptor_chain(fa: &mut FileAnalyze, region_size: usize, track: &mut TrackInfo) {
    let region_end = fa.Element_Offset() + region_size;
    while fa.Element_Offset() + 2 <= region_end {
        let bytes = fa.peek_raw(1);
        let Some(b) = bytes else { break };
        let tag = b[0];
        let _ = fa.read_raw(1);
        let size = read_descriptor_length(fa);
        let body_start = fa.Element_Offset();
        let body_end = body_start + size;
        if body_end > region_end {
            break;
        }
        match tag {
            0x03 => parse_es_descriptor(fa, size, track),
            0x04 => parse_decoder_config(fa, size, track),
            0x05 => parse_decoder_specific_info(fa, size, track),
            _ => fa.Skip_Hexa(size, "unknown_descriptor"),
        }
        if fa.Element_Offset() < body_end {
            fa.Skip_Hexa(body_end - fa.Element_Offset(), "descriptor_tail");
        } else if fa.Element_Offset() > body_end {
            break;
        }
    }
}

fn parse_es_descriptor(fa: &mut FileAnalyze, size: usize, track: &mut TrackInfo) {
    if size < 3 {
        fa.Skip_Hexa(size, "es_descriptor_short");
        return;
    }
    let start = fa.Element_Offset();
    fa.Skip_Hexa(2, "ES_ID");
    let mut flags: zenlib::int8u = 0;
    fa.Get_B1(&mut flags, "flags");
    let stream_dep = (flags & 0x80) != 0;
    let url_flag = (flags & 0x40) != 0;
    let ocr_flag = (flags & 0x20) != 0;
    if stream_dep {
        fa.Skip_Hexa(2, "dependsOnESID");
    }
    if url_flag {
        let url_bytes = fa.peek_raw(1);
        if let Some(b) = url_bytes {
            let url_len = b[0] as usize;
            let _ = fa.read_raw(1);
            fa.Skip_Hexa(url_len, "URL");
        }
    }
    if ocr_flag {
        fa.Skip_Hexa(2, "OCR_ES_Id");
    }
    let consumed = fa.Element_Offset() - start;
    // Remaining body bytes contain nested descriptors (DecoderConfig etc).
    let inner = size.saturating_sub(consumed);
    parse_descriptor_chain(fa, inner, track);
}

fn parse_decoder_config(fa: &mut FileAnalyze, size: usize, track: &mut TrackInfo) {
    if size < 13 {
        fa.Skip_Hexa(size, "decoder_config_short");
        return;
    }
    let start = fa.Element_Offset();
    let mut oti: zenlib::int8u = 0;
    fa.Get_B1(&mut oti, "objectTypeIndication");
    track.object_type_indication = Some(oti);
    fa.Skip_Hexa(1, "streamType_upStream");
    fa.Skip_Hexa(3, "bufferSizeDB");
    let mut max_br: int32u = 0;
    fa.Get_B4(&mut max_br, "maxBitrate");
    let mut avg_br: int32u = 0;
    fa.Get_B4(&mut avg_br, "avgBitrate");
    if avg_br > 0 {
        track.avg_bitrate_bps = Some(avg_br);
    }
    let consumed = fa.Element_Offset() - start;
    let inner = size.saturating_sub(consumed);
    parse_descriptor_chain(fa, inner, track);
}

fn parse_decoder_specific_info(fa: &mut FileAnalyze, size: usize, track: &mut TrackInfo) {
    if size < 2 {
        fa.Skip_Hexa(size, "dsi_short");
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
        fa.Skip_Hexa(size - 2, "dsi_tail");
    }
}

fn parse_stsz(fa: &mut FileAnalyze, box_size: usize, track: &mut TrackInfo) {
    let body_size = box_size.saturating_sub(8);
    if body_size < 12 {
        fa.Skip_Hexa(body_size, "stsz");
        return;
    }
    let start = fa.Element_Offset();
    fa.Skip_Hexa(4, "version_flags");
    let mut sample_size: int32u = 0;
    fa.Get_B4(&mut sample_size, "sample_size");
    let mut sample_count: int32u = 0;
    fa.Get_B4(&mut sample_count, "sample_count");
    track.sample_count = Some(sample_count);

    if sample_size != 0 {
        // Uniform sample size — no per-sample table follows.
        track.source_stream_size = Some((sample_count as u64) * (sample_size as u64));
    } else {
        // Per-sample size table: 4 bytes per entry, sample_count entries.
        let table_bytes = (sample_count as usize) * 4;
        let available = body_size.saturating_sub(fa.Element_Offset() - start);
        let read_bytes = table_bytes.min(available);
        let entries = read_bytes / 4;
        let mut total: u64 = 0;
        for _ in 0..entries {
            let mut entry: int32u = 0;
            fa.Get_B4(&mut entry, "sample_size_entry");
            total = total.saturating_add(entry as u64);
        }
        track.source_stream_size = Some(total);
    }

    let consumed = fa.Element_Offset() - start;
    if consumed < body_size {
        fa.Skip_Hexa(body_size - consumed, "stsz_tail");
    }
}

fn fill_streams(
    fa: &mut FileAnalyze,
    ftyp_brands: &[String],
    tracks: &[TrackInfo],
    movie: &MovieInfo,
    layout: &BoxLayout,
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MPEG-4", false);

    // mdat-positioning fields. The C++ side reports:
    //   HeaderSize = bytes before mdat box (ftyp + free + any pre-mdat moov)
    //   DataSize   = mdat total size (header + body)
    //   FooterSize = bytes after mdat
    //   StreamSize (general) = FileSize - mdat_body_size = HeaderSize + 8 + FooterSize
    //   IsStreamable = "Yes" if moov precedes mdat, else "No"
    if let (Some(mdat_off), Some(mdat_tot)) = (layout.mdat_offset, layout.mdat_size) {
        let footer_size = layout.file_size.saturating_sub(mdat_off + mdat_tot);
        let stream_size = layout.file_size.saturating_sub(mdat_tot.saturating_sub(8));
        fa.Fill(StreamKind::General, 0, "StreamSize", stream_size.to_string(), true);
        fa.Fill(StreamKind::General, 0, "HeaderSize", mdat_off.to_string(), false);
        fa.Fill(StreamKind::General, 0, "DataSize", mdat_tot.to_string(), false);
        fa.Fill(StreamKind::General, 0, "FooterSize", footer_size.to_string(), false);
        let is_streamable = match layout.moov_offset {
            Some(mv_off) if mv_off < mdat_off => "Yes",
            _ => "No",
        };
        fa.Fill(StreamKind::General, 0, "IsStreamable", is_streamable, false);
    }

    // Presence of any iTunes metadata triggers the "Apple audio with iTunes
    // info" profile descriptor, regardless of which keys are populated.
    if !movie.itunes_metadata.is_empty() {
        fa.Fill(
            StreamKind::General,
            0,
            "Format_Profile",
            "Apple audio with iTunes info",
            false,
        );
        for (key, value) in &movie.itunes_metadata {
            if *key == ITUNES_KEY_TOOL {
                fa.Fill(
                    StreamKind::General,
                    0,
                    "Encoded_Application",
                    value.clone(),
                    false,
                );
            }
        }
    }

    if !ftyp_brands.is_empty() {
        fa.Fill(
            StreamKind::General,
            0,
            "CodecID",
            ftyp_brands[0].clone(),
            false,
        );
        // CodecID_Compatible = major_brand + compatible_brands joined,
        // de-duplicated to match the oracle (it emits each unique brand
        // once even when the major is repeated in the brand list).
        let mut seen: Vec<&str> = Vec::new();
        for b in ftyp_brands {
            if !seen.iter().any(|s| *s == b.as_str()) {
                seen.push(b.as_str());
            }
        }
        if seen.len() > 1 {
            fa.Fill(
                StreamKind::General,
                0,
                "CodecID_Compatible",
                seen.join("/"),
                false,
            );
        }
    }

    let mut audio_count: u32 = 0;
    let mut video_count: u32 = 0;
    let mut stream_order: u32 = 0;
    for (track_idx, track) in tracks.iter().enumerate() {
        if track.handler == HANDLER_SOUN && track.has_data {
            let pos = fa.Stream_Prepare(StreamKind::Audio);
            fa.Fill(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string(), false);
            // ID in MP4 tracks is 1-based; we don't yet parse tkhd's
            // track_ID, so derive from position in the moov box.
            fa.Fill(StreamKind::Audio, pos, "ID", (track_idx + 1).to_string(), false);
            stream_order += 1;
            if let Some(f) = track.audio_format {
                fa.Fill(StreamKind::Audio, pos, "Format", f, false);
            }
            // SBR/PS profile signaling — AOT 2 (LC) with no explicit SBR
            // signaling in the AudioSpecificConfig is reported as
            // "No (Explicit)" by the oracle.
            if let Some(aot) = track.audio_object_type {
                if let Some(profile) = aac_profile_name(aot) {
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "Format_AdditionalFeatures",
                        profile,
                        false,
                    );
                }
                if aot == 2 {
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "Format_Settings_SBR",
                        "No (Explicit)",
                        false,
                    );
                }
            }
            // CodecID from esds: "mp4a-{OTI:hex lowercase}-{AOT}".
            if let (Some(oti), Some(aot)) = (track.object_type_indication, track.audio_object_type) {
                let codec_id = format!("mp4a-{:x}-{}", oti, aot);
                fa.Fill(StreamKind::Audio, pos, "CodecID", codec_id, false);
            }
            if let Some(br) = track.avg_bitrate_bps {
                fa.Fill(StreamKind::Audio, pos, "BitRate_Mode", "CBR", false);
                fa.Fill(StreamKind::Audio, pos, "BitRate", br.to_string(), false);
            }
            if let Some(ch) = track.audio_channels {
                fa.Fill(StreamKind::Audio, pos, "Channels", ch.to_string(), false);
                let (positions, layout) = channel_layout(ch);
                if let Some(p) = positions {
                    fa.Fill(StreamKind::Audio, pos, "ChannelPositions", p, false);
                }
                if let Some(l) = layout {
                    fa.Fill(StreamKind::Audio, pos, "ChannelLayout", l, false);
                }
            }
            if let Some(sr) = track.audio_sample_rate {
                fa.Fill(StreamKind::Audio, pos, "SamplingRate", sr.to_string(), false);
                // AAC frames are always 1024 samples. FrameRate is
                // sample_rate/1024 (e.g. 48000/1024 = 46.875).
                if matches!(track.audio_format, Some("AAC")) {
                    fa.Fill(StreamKind::Audio, pos, "SamplesPerFrame", "1024", false);
                    if sr > 0 {
                        let rate = (sr as f64) / 1024.0;
                        fa.Fill(
                            StreamKind::Audio,
                            pos,
                            "FrameRate",
                            format!("{:.3}", rate),
                            false,
                        );
                    }
                }
            }
            if matches!(track.audio_format, Some("AAC")) {
                fa.Fill(StreamKind::Audio, pos, "Compression_Mode", "Lossy", false);
            }
            // Duration / SamplingCount / FrameCount come from the
            // movie-level duration converted into the track's media
            // timescale — this is mvhd.duration scaled by the ratio of
            // media-to-movie timescales. mvhd.duration already accounts
            // for any edit-list trimming, so for single-track files
            // this matches the oracle's effective values. For
            // multi-track files where an audio track is shorter than
            // the longest track, this gives an overshoot; correcting
            // requires per-track elst + stts integration.
            let trimmed_units: Option<u64> = if movie.timescale > 0
                && movie.duration > 0
                && track.timescale > 0
            {
                Some(movie.duration * track.timescale as u64 / movie.timescale as u64)
            } else if track.timescale > 0 && track.duration_units > 0 {
                Some(track.duration_units)
            } else {
                None
            };
            let _ = track.elst_segment_duration; // reserved for multi-track refinement
            if let Some(units) = trimmed_units {
                fa.Fill(
                    StreamKind::Audio,
                    pos,
                    "SamplingCount",
                    units.to_string(),
                    false,
                );
                if track.timescale > 0 {
                    let duration_ms = (units * 1000) / (track.timescale as u64);
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "Duration",
                        duration_ms.to_string(),
                        false,
                    );
                }
                if matches!(track.audio_format, Some("AAC")) {
                    let frame_count = units.div_ceil(1024);
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "FrameCount",
                        frame_count.to_string(),
                        false,
                    );
                }
            }
            // Source_* fields: pre-edit values from stsz directly.
            if let Some(count) = track.sample_count {
                fa.Fill(
                    StreamKind::Audio,
                    pos,
                    "Source_FrameCount",
                    count.to_string(),
                    false,
                );
                if matches!(track.audio_format, Some("AAC")) {
                    if let Some(sr) = track.audio_sample_rate {
                        // Truncated milliseconds: oracle formats with
                        // `Ztring::ToZtring(float64, 0)` which is round-to-
                        // even of %.0f, but for integer-divisible cases
                        // truncation matches.
                        let total_samples = (count as u64) * 1024;
                        let source_dur = (total_samples * 1000) / (sr as u64);
                        fa.Fill(
                            StreamKind::Audio,
                            pos,
                            "Source_Duration",
                            source_dur.to_string(),
                            false,
                        );
                    }
                }
            }
            if let Some(size) = track.source_stream_size {
                fa.Fill(
                    StreamKind::Audio,
                    pos,
                    "Source_StreamSize",
                    size.to_string(),
                    false,
                );
            }
            audio_count += 1;
        } else if track.handler == HANDLER_VIDE && track.has_data {
            video_count += 1;
        }
    }

    if audio_count > 0 {
        fa.Fill(StreamKind::General, 0, "AudioCount", audio_count.to_string(), false);
    }
    if video_count > 0 {
        fa.Fill(StreamKind::General, 0, "VideoCount", video_count.to_string(), false);
    }
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
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("MPEG-4")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "CodecID").map(|z| z.as_str().to_owned()).as_deref(),
            Some("M4A ")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "CodecID_Compatible").map(|z| z.as_str().to_owned()).as_deref(),
            Some("M4A /isom")
        );
    }
}
