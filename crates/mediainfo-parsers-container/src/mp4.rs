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

const HANDLER_SOUN: int32u = u32::from_be_bytes(*b"soun");
const HANDLER_VIDE: int32u = u32::from_be_bytes(*b"vide");

const SAMPLE_ENTRY_MP4A: int32u = u32::from_be_bytes(*b"mp4a");

#[derive(Debug, Default)]
struct TrackInfo {
    handler: int32u,
    timescale: u32,
    duration_units: u64,
    audio_channels: Option<u16>,
    audio_sample_rate: Option<u32>,
    audio_format: Option<&'static str>,
    sample_count: Option<u32>,
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

    let buffer_len = fa.Remain();
    walk_boxes(fa, buffer_len, 0, &mut |fa, box_type, box_size, depth| {
        match box_type {
            BOX_FTYP => {
                ftyp_brands = parse_ftyp(fa, box_size);
            }
            BOX_MOOV | BOX_MDIA | BOX_MINF | BOX_STBL => {
                // Pure container — recurse.
                let inner = box_size.saturating_sub(8);
                walk_boxes(fa, inner, depth + 1, &mut |fa, t, s, _| {
                    handle_inner(fa, t, s, &mut tracks);
                });
            }
            BOX_TRAK => {
                tracks.push(TrackInfo::default());
                let inner = box_size.saturating_sub(8);
                walk_boxes(fa, inner, depth + 1, &mut |fa, t, s, _| {
                    handle_inner(fa, t, s, &mut tracks);
                });
            }
            _ => {
                fa.Skip_Hexa(box_size.saturating_sub(8), "BoxBody");
            }
        }
    });

    fill_streams(fa, &ftyp_brands, &tracks);
    true
}

/// Iterate boxes from the current position for up to `region_size`
/// bytes (or to end of buffer if 0). Stops early on truncated reads.
fn walk_boxes(
    fa: &mut FileAnalyze,
    region_size: usize,
    depth: usize,
    visit: &mut dyn FnMut(&mut FileAnalyze, int32u, usize, usize),
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
        visit(fa, box_type, total_size, depth);
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
) {
    match box_type {
        BOX_MOOV | BOX_MDIA | BOX_MINF | BOX_STBL => {
            let inner = box_size.saturating_sub(8);
            walk_boxes(fa, inner, 1, &mut |fa, t, s, _| {
                handle_inner(fa, t, s, tracks);
            });
        }
        BOX_TRAK => {
            tracks.push(TrackInfo::default());
            let inner = box_size.saturating_sub(8);
            walk_boxes(fa, inner, 1, &mut |fa, t, s, _| {
                handle_inner(fa, t, s, tracks);
            });
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
        _ => {
            fa.Skip_Hexa(box_size.saturating_sub(8), "BoxBody");
        }
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
    let remaining = start_remain.saturating_sub(consumed);
    if remaining > 0 {
        fa.Skip_Hexa(remaining, "mp4a_extensions");
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
    let mut _sample_size: int32u = 0;
    fa.Get_B4(&mut _sample_size, "sample_size");
    let mut sample_count: int32u = 0;
    fa.Get_B4(&mut sample_count, "sample_count");
    track.sample_count = Some(sample_count);

    let consumed = fa.Element_Offset() - start;
    if consumed < body_size {
        fa.Skip_Hexa(body_size - consumed, "stsz_tail");
    }
}

fn fill_streams(fa: &mut FileAnalyze, ftyp_brands: &[String], tracks: &[TrackInfo]) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MPEG-4", false);

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
    for track in tracks {
        if track.handler == HANDLER_SOUN && track.has_data {
            let pos = fa.Stream_Prepare(StreamKind::Audio);
            if let Some(f) = track.audio_format {
                fa.Fill(StreamKind::Audio, pos, "Format", f, false);
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
            }
            // Duration and SamplingCount intentionally not filled in this
            // commit. The numbers derivable from stsz (sample_count *
            // samples_per_frame) match oracle's Source_Duration /
            // Source_FrameCount, but oracle's primary Duration /
            // SamplingCount apply the edit list (`elst`) to back out the
            // last partial frame. Adding `elst` parsing closes that gap.
            let _ = track.timescale;
            let _ = track.duration_units;
            let _ = track.sample_count;
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
