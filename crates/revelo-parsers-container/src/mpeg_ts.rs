//! MPEG-TS (ISO/IEC 13818-1) parser.
//!
//! Transport stream packets are 188 bytes, identified by sync byte 0x47
//! at the start. Two common wrappers add bytes around each packet:
//!   - BDAV (Blu-ray): 4-byte timecode prefix → 192 bytes total
//!   - TSP / ECC:      16-byte ECC suffix    → 204 bytes total
//!
//! Parse strategy:
//!   1. Detect packet size by checking sync bytes at 16 stride positions.
//!   2. Walk packets, accumulating PSI (PAT/PMT) section bytes when the
//!      packet payload_unit_start_indicator is set.
//!   3. Parse PAT (PID 0) → list of PMT PIDs.
//!   4. Parse each PMT → elementary streams (PID + stream_type).
//!   5. Emit one stream entry per elementary stream, with Format/CodecID
//!      mapped from stream_type per ITU-T H.222.0 + Bluray/ATSC overlays.

use revelo_core::{FileAnalyze, StreamKind};
use std::collections::BTreeMap;

const SYNC: u8 = 0x47;
const MPEG_TS_PROBE_LIMIT: usize = 2 * 1024 * 1024;
/// PID of the Service Description Table (DVB EN 300 468).
const SDT_PID: u16 = 0x0011;

#[derive(Clone, Copy, Debug)]
struct PacketLayout {
    packet_size: usize,
    bdav_prefix: usize,
}

fn detect_layout(buf: &[u8]) -> Option<PacketLayout> {
    // Try each candidate layout. Require sync at 16 consecutive packets.
    for (packet_size, bdav_prefix) in [(188, 0), (192, 4), (204, 0)] {
        if buf.len() < packet_size * 16 {
            continue;
        }
        // Search for a starting offset (up to packet_size bytes) where
        // sync appears at every stride for 16 packets.
        'outer: for start in 0..packet_size.min(buf.len()) {
            for i in 0..16 {
                let pos = start + i * packet_size + bdav_prefix;
                if pos >= buf.len() || buf[pos] != SYNC {
                    continue 'outer;
                }
            }
            // Found valid alignment — but we need start to be the first
            // byte of the FIRST packet (not somewhere mid-stream). We
            // tolerate a leading garbage offset by reporting the layout
            // with bdav_prefix adjusted to put the first sync at `start`.
            let _ = start;
            return Some(PacketLayout { packet_size, bdav_prefix });
        }
    }
    None
}

#[derive(Default, Debug)]
struct ElementaryStream {
    pid: u16,
    stream_type: u8,
    /// Registration descriptor format identifier (4 ASCII bytes packed
    /// big-endian, e.g. 'HDMV' = 0x48444D56).
    format_identifier: u32,
    _language: Option<String>,
    _ac3_descriptor: bool,
    /// AAC payload params extracted from first ADTS frame inside PES,
    /// when stream_type indicates AAC (0x0F/0x11/0x1C).
    aac: Option<AacInfo>,
    /// AVC elementary-stream params extracted from the first SPS/PPS/SEI
    /// NAL units inside the PES payload, when stream_type is AVC
    /// (0x1B/0x1F/0x20).
    avc: Option<AvcEs>,
}

/// AVC parameters recovered from the Annex-B elementary stream carried in
/// the PES payload: the SPS-derived [`AvcInfo`], the constraint_set1_flag
/// (needed to distinguish "Constrained Baseline" from "Baseline"), the
/// entropy_coding_mode_flag read from the PPS (CABAC), and the x264/x265
/// encoder identification recovered from an SEI user_data_unregistered NAL.
#[derive(Debug)]
struct AvcEs {
    info: revelo_parsers_video::AvcInfo,
    constrained: bool,
    cabac: Option<bool>,
    encoder: Option<revelo_parsers_video::EncoderInfo>,
}

#[derive(Debug, Clone, Copy)]
struct AacInfo {
    /// AudioObjectType: profile + 1 (e.g. 2 = LC, 5 = HE-AAC SBR).
    aot: u8,
    sample_rate: u32,
    channels: u8,
}

#[derive(Default, Debug)]
struct Program {
    program_number: u16,
    pmt_pid: u16,
    format_identifier: u32,
    streams: Vec<ElementaryStream>,
    /// `section_length` field of the PMT section (for the Menu `<extra>`).
    pmt_section_length: Option<u16>,
    /// `pointer_field` byte preceding the PMT section (Menu `<extra>`).
    pmt_pointer_field: Option<u8>,
}

/// Service description recovered from the SDT (PID 0x0011), keyed by
/// service_id (== program_number). Populates the Menu ServiceName /
/// ServiceProvider / ServiceType fields.
#[derive(Default, Debug, Clone)]
struct ServiceInfo {
    provider: Option<String>,
    name: Option<String>,
    service_type: Option<u8>,
}

/// Parse MPEG Transport Stream (ITU-T H.222.0).
///
/// Detection: sync byte 0x47 every 188 bytes. PAT→PMT→stream descriptors.
/// Fills: Program names, language codes, format from stream_type + registration.
pub fn parse_mpeg_ts(fa: &mut FileAnalyze) -> bool {
    let probe_len = fa.remain().min(MPEG_TS_PROBE_LIMIT);
    let buf = match fa.peek_raw(probe_len) {
        Some(b) => b,
        None => return false,
    };
    let layout = match detect_layout(buf) {
        Some(l) => l,
        None => return false,
    };

    // Find the first packet offset (the alignment).
    let first_offset = find_first_sync(buf, &layout).unwrap_or(0);

    // Per-PID accumulator for PSI sections. PSI uses point_field to start.
    let mut psi_buffers: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
    let mut programs_by_pmt_pid: BTreeMap<u16, Program> = BTreeMap::new();
    let mut sdt_services: BTreeMap<u16, ServiceInfo> = BTreeMap::new();
    let mut pat_seen = false;
    let mut sdt_seen = false;

    let mut pos = first_offset;
    let stride = layout.packet_size;
    while pos + 188 <= buf.len() {
        let sync_pos = pos + layout.bdav_prefix;
        if sync_pos + 188 > buf.len() || buf[sync_pos] != SYNC {
            // Lost sync — try to resync by scanning forward.
            let Some(np) = resync(buf, pos, &layout) else { break };
            pos = np;
            continue;
        }
        let pkt = &buf[sync_pos..sync_pos + 188];
        let payload_unit_start = (pkt[1] & 0x40) != 0;
        let pid = (((pkt[1] & 0x1F) as u16) << 8) | (pkt[2] as u16);
        let adaptation_control = (pkt[3] >> 4) & 0x3;
        let has_adaptation = adaptation_control == 2 || adaptation_control == 3;
        let has_payload = adaptation_control == 1 || adaptation_control == 3;

        let mut payload_off = 4usize;
        if has_adaptation {
            let af_len = pkt[4] as usize;
            payload_off = 5 + af_len;
        }
        if !has_payload || payload_off >= 188 {
            pos += stride;
            continue;
        }
        let payload = &pkt[payload_off..];

        // Only collect PSI for known PSI PIDs we care about.
        let is_pat = pid == 0;
        let is_pmt = programs_by_pmt_pid.contains_key(&pid);
        let is_sdt = pid == SDT_PID;
        if !is_pat && !is_pmt && !is_sdt {
            pos += stride;
            continue;
        }

        if payload_unit_start {
            if payload.is_empty() {
                pos += stride;
                continue;
            }
            let pointer = payload[0] as usize;
            // Record the PMT's pointer_field for the Menu <extra> section.
            if is_pmt && let Some(prog) = programs_by_pmt_pid.get_mut(&pid) {
                prog.pmt_pointer_field.get_or_insert(payload[0]);
            }
            let section_start = 1 + pointer;
            if section_start >= payload.len() {
                pos += stride;
                continue;
            }
            // Reset and start fresh PSI accumulator with bytes after pointer.
            let buf = psi_buffers.entry(pid).or_default();
            buf.clear();
            buf.extend_from_slice(&payload[section_start..]);
        } else if let Some(buf) = psi_buffers.get_mut(&pid) {
            buf.extend_from_slice(payload);
        }

        // Try to parse complete section(s) from the accumulator.
        if let Some(buf) = psi_buffers.get(&pid)
            && buf.len() >= 3
        {
            let section_length = (((buf[1] & 0x0F) as usize) << 8) | (buf[2] as usize);
            let total = 3 + section_length;
            if buf.len() >= total {
                let section = buf[..total].to_vec();
                if is_pat && !pat_seen {
                    parse_pat(&section, &mut programs_by_pmt_pid);
                    pat_seen = true;
                } else if is_sdt && !sdt_seen {
                    parse_sdt(&section, &mut sdt_services);
                    sdt_seen = true;
                } else if is_pmt
                    && let Some(prog) = programs_by_pmt_pid.get_mut(&pid)
                    && prog.streams.is_empty()
                {
                    prog.pmt_section_length = Some(section_length as u16);
                    parse_pmt(&section, prog);
                }
            }
        }
        pos += stride;
    }

    if !pat_seen {
        return false;
    }

    // Second pass: sniff first PES payload for AAC streams to extract
    // ADTS header (AOT/SamplingRate/Channels/Format_Version). Walk the
    // same packet stream, accumulating up to 1 KiB of PES bytes per AAC
    // PID, then parse the ADTS sync once we have enough.
    let mut aac_pids: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
    let mut avc_pids: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
    // First/last presentation timestamp (90 kHz) per ES PID, for Delay
    // and stream-Duration derivation.
    let mut pts_first: BTreeMap<u16, u64> = BTreeMap::new();
    let mut pts_last: BTreeMap<u16, u64> = BTreeMap::new();
    // Count of ADTS frames seen per AAC PID (for audio Duration).
    let mut adts_frames: BTreeMap<u16, u64> = BTreeMap::new();
    let mut es_pids: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
    for prog in programs_by_pmt_pid.values() {
        for es in &prog.streams {
            es_pids.insert(es.pid);
            if matches!(es.stream_type, 0x0F | 0x11 | 0x1C) {
                aac_pids.insert(es.pid, Vec::new());
            }
            if matches!(es.stream_type, 0x1B | 0x1F | 0x20) {
                avc_pids.insert(es.pid, Vec::new());
            }
        }
    }
    if !es_pids.is_empty() {
        let mut pos = first_offset;
        while pos + 188 <= buf.len() {
            let sync_pos = pos + layout.bdav_prefix;
            if sync_pos + 188 > buf.len() || buf[sync_pos] != SYNC {
                pos += stride;
                continue;
            }
            let pkt = &buf[sync_pos..sync_pos + 188];
            let pid = (((pkt[1] & 0x1F) as u16) << 8) | (pkt[2] as u16);
            let adaptation_control = (pkt[3] >> 4) & 0x3;
            let has_adaptation = adaptation_control == 2 || adaptation_control == 3;
            let has_payload = adaptation_control == 1 || adaptation_control == 3;
            if !has_payload {
                pos += stride;
                continue;
            }
            let mut payload_off = 4usize;
            if has_adaptation {
                let af_len = pkt[4] as usize;
                payload_off = 5 + af_len;
            }
            if payload_off >= 188 {
                pos += stride;
                continue;
            }
            // Capture first/last PES PTS for Delay + Duration derivation.
            let payload_unit_start = (pkt[1] & 0x40) != 0;
            if payload_unit_start
                && es_pids.contains(&pid)
                && let Some(pts) = parse_pes_pts(&pkt[payload_off..])
            {
                pts_first
                    .entry(pid)
                    .and_modify(|v| {
                        if pts < *v {
                            *v = pts
                        }
                    })
                    .or_insert(pts);
                pts_last
                    .entry(pid)
                    .and_modify(|v| {
                        if pts > *v {
                            *v = pts
                        }
                    })
                    .or_insert(pts);
            }
            if let Some(accum) = aac_pids.get_mut(&pid)
                && accum.len() < 512 * 1024
            {
                accum.extend_from_slice(&pkt[payload_off..]);
            }
            if let Some(accum) = avc_pids.get_mut(&pid)
                && accum.len() < 32 * 1024
            {
                accum.extend_from_slice(&pkt[payload_off..]);
            }
            pos += stride;
        }
        // Now parse each accumulator: skip past PES header to ES payload,
        // then find ADTS sync.
        for prog in programs_by_pmt_pid.values_mut() {
            for es in prog.streams.iter_mut() {
                if matches!(es.stream_type, 0x0F | 0x11 | 0x1C)
                    && let Some(accum) = aac_pids.get(&es.pid)
                {
                    es.aac = sniff_aac_adts(accum);
                    adts_frames.insert(es.pid, count_adts_frames(accum));
                }
                if matches!(es.stream_type, 0x1B | 0x1F | 0x20)
                    && let Some(accum) = avc_pids.get(&es.pid)
                {
                    es.avc = sniff_avc(accum);
                }
            }
        }
    }

    let container_format = match (layout.packet_size, layout.bdav_prefix) {
        (192, 4) => "BDAV",
        (204, 0) => "MPEG-TS 188+16",
        _ => "MPEG-TS",
    };

    // Measure the transport-stream bitrate from PCR-vs-byte-position and
    // derive the overall Duration from it (MediaInfo's TS approach).
    let timing = measure_ts_timing(buf, first_offset, stride, layout.bdav_prefix);

    fa.stream_prepare(StreamKind::General);
    fa.force_field(StreamKind::General, 0, "Format", container_format);

    // General.ID = program_number of the first program (matches oracle
    // for single-program files; multi-program TS would list each).
    if let Some(first_prog) = programs_by_pmt_pid.values().next() {
        fa.set_field(StreamKind::General, 0, "ID", first_prog.program_number.to_string());
    }

    if let Some(t) = &timing {
        // Duration carries sub-millisecond precision, so emit it as a
        // decimal-seconds string (the export layer only reformats values
        // that parse as integer milliseconds).
        fa.set_field(StreamKind::General, 0, "Duration", format!("{:.9}", t.duration_s));
        fa.set_field(StreamKind::General, 0, "OverallBitRate", t.overall_bitrate.to_string());
        fa.set_field(StreamKind::General, 0, "OverallBitRate_Mode", "VBR");
    }

    // Count elementary streams by kind for *Count fields.
    let mut video_count = 0u32;
    let mut audio_count = 0u32;
    let mut text_count = 0u32;
    // MenuCount tracks our emitted Menu streams (one per program with
    // at least one identifiable ES), not stream_type→Menu mapping.
    let menu_count = programs_by_pmt_pid
        .values()
        .filter(|p| {
            p.streams
                .iter()
                .any(|es| !stream_format(es.stream_type, prog_or_es_fid(p, es)).is_empty())
        })
        .count() as u32;

    for prog in programs_by_pmt_pid.values() {
        for es in &prog.streams {
            let kind = stream_kind(es.stream_type, prog_or_es_fid(prog, es));
            match kind {
                Some(StreamKind::Video) => video_count += 1,
                Some(StreamKind::Audio) => audio_count += 1,
                Some(StreamKind::Text) => text_count += 1,
                Some(StreamKind::Menu) => { /* counted separately above */ }
                _ => {}
            }
        }
    }
    if video_count > 0 {
        fa.set_field(StreamKind::General, 0, "VideoCount", video_count.to_string());
    }
    if audio_count > 0 {
        fa.set_field(StreamKind::General, 0, "AudioCount", audio_count.to_string());
    }
    if text_count > 0 {
        fa.set_field(StreamKind::General, 0, "TextCount", text_count.to_string());
    }
    if menu_count > 0 {
        fa.set_field(StreamKind::General, 0, "MenuCount", menu_count.to_string());
    }

    // First video PTS, used to express the audio track's Video_Delay
    // (audio Delay relative to video).
    let mut video_ref_pts: Option<u64> = None;
    'find_video: for prog in programs_by_pmt_pid.values() {
        for es in &prog.streams {
            if matches!(
                stream_kind(es.stream_type, prog_or_es_fid(prog, es)),
                Some(StreamKind::Video)
            ) && let Some(&p) = pts_first.get(&es.pid)
            {
                video_ref_pts = Some(p);
                break 'find_video;
            }
        }
    }

    // Emit per-stream entries in (program, ES) order. `menu_members`
    // records, per program, the (StreamKind, position-in-kind) of each
    // emitted ES so the Menu track can list its member streams.
    let mut menu_members: Vec<Vec<(u8, usize)>> = Vec::new();
    for (prog_idx, prog) in programs_by_pmt_pid.values().enumerate() {
        let mut members: Vec<(u8, usize)> = Vec::new();
        for (es_idx, es) in prog.streams.iter().enumerate() {
            let fid = prog_or_es_fid(prog, es);
            let Some(kind) = stream_kind(es.stream_type, fid) else { continue };
            let format = stream_format(es.stream_type, fid);
            let codec = stream_codec(es.stream_type, fid);
            if format.is_empty() {
                continue;
            }
            fa.stream_prepare(kind);
            let pos_in_kind = fa.stream_count(kind) - 1;
            members.push((kind as u8, pos_in_kind));
            // StreamOrder = "<program_idx>-<es_idx_in_program>" per oracle.
            fa.set_field(kind, pos_in_kind, "StreamOrder", format!("{}-{}", prog_idx, es_idx));
            // ID = PID. Oracle renders as decimal.
            fa.set_field(kind, pos_in_kind, "ID", es.pid.to_string());
            fa.set_field(kind, pos_in_kind, "MenuID", prog.program_number.to_string());
            fa.set_field(kind, pos_in_kind, "Format", format);
            // Delay = first PES presentation timestamp (container-provided).
            // Audio additionally reports Video_Delay relative to video.
            if matches!(kind, StreamKind::Video | StreamKind::Audio)
                && let Some(&pts) = pts_first.get(&es.pid)
            {
                fa.set_field(kind, pos_in_kind, "Delay", format!("{:.9}", pts as f64 / 90000.0));
                fa.set_field(kind, pos_in_kind, "Delay_Source", "Container");
                if matches!(kind, StreamKind::Audio)
                    && let Some(vref) = video_ref_pts
                {
                    let vd = (pts as f64 - vref as f64) / 90000.0;
                    fa.set_field(kind, pos_in_kind, "Video_Delay", format!("{vd:.3}"));
                }
            }
            // Video CodecID = decimal stream_type (oracle convention).
            // AAC overrides this below with its "<type>-<AOT>" form.
            if matches!(kind, StreamKind::Video) {
                fa.set_field(kind, pos_in_kind, "CodecID", es.stream_type.to_string());
                if matches!(es.stream_type, 0x1B | 0x1F | 0x20) {
                    // MediaInfo reports MPEG-TS AVC as VFR (frame timing is
                    // derived from PTS jitter, not the SPS timing_info).
                    fa.set_field(kind, pos_in_kind, "FrameRate_Mode", "VFR");
                    fa.set_field(kind, pos_in_kind, "Compression_Mode", "Lossy");
                    if let Some(avc) = &es.avc {
                        emit_avc_fields(fa, kind, pos_in_kind, avc);
                    } else {
                        // No SPS recovered — keep the safe H.264 defaults.
                        fa.set_field(kind, pos_in_kind, "BitDepth", "8");
                        fa.set_field(kind, pos_in_kind, "ScanType", "Progressive");
                    }
                }
            }
            if let Some(aac) = &es.aac {
                // AAC ADTS payload → unlocks Format_Version, AOT, CodecID,
                // MuxingMode=ADTS, Channels, SamplingRate, SamplesPerFrame.
                fa.set_field(kind, pos_in_kind, "Format_Version", "4");
                if let Some(profile) = aac_profile_name(aac.aot) {
                    fa.set_field(kind, pos_in_kind, "Format_AdditionalFeatures", profile);
                }
                fa.set_field(kind, pos_in_kind, "MuxingMode", "ADTS");
                fa.set_field(
                    kind,
                    pos_in_kind,
                    "CodecID",
                    format!("{}-{}", es.stream_type, aac.aot),
                );
                fa.set_field(kind, pos_in_kind, "BitRate_Mode", "VBR");
                fa.set_field(kind, pos_in_kind, "Channels", aac.channels.to_string());
                let (positions, layout) = aac_channel_layout(aac.channels);
                if let Some(p) = positions {
                    fa.set_field(kind, pos_in_kind, "ChannelPositions", p);
                }
                if let Some(l) = layout {
                    fa.set_field(kind, pos_in_kind, "ChannelLayout", l);
                }
                fa.set_field(kind, pos_in_kind, "SamplesPerFrame", "1024");
                fa.set_field(kind, pos_in_kind, "SamplingRate", aac.sample_rate.to_string());
                if aac.sample_rate > 0 {
                    let frame_rate = aac.sample_rate as f64 / 1024.0;
                    fa.set_field(kind, pos_in_kind, "FrameRate", format!("{:.3}", frame_rate));
                }
                fa.set_field(kind, pos_in_kind, "Compression_Mode", "Lossy");
                // Duration from the counted ADTS frames (1024 samples each).
                if aac.sample_rate > 0
                    && let Some(&frames) = adts_frames.get(&es.pid)
                    && frames > 0
                {
                    let dur = frames as f64 * 1024.0 / aac.sample_rate as f64;
                    fa.set_field(kind, pos_in_kind, "Duration", format!("{dur:.3}"));
                }
            }
            let _ = codec;
        }
        menu_members.push(members);
    }

    // Emit one Menu stream per program. Format = joined ES format names
    // (in declaration order). ID = PMT_PID. MenuID = program_number.
    for (prog_idx, prog) in programs_by_pmt_pid.values().enumerate() {
        let formats: Vec<&'static str> = prog
            .streams
            .iter()
            .map(|es| stream_format(es.stream_type, prog_or_es_fid(prog, es)))
            .filter(|f| !f.is_empty())
            .collect();
        if formats.is_empty() {
            continue;
        }
        let pos = fa.stream_prepare(StreamKind::Menu);
        fa.set_field(StreamKind::Menu, pos, "StreamOrder", prog_idx.to_string());
        fa.set_field(StreamKind::Menu, pos, "ID", prog.pmt_pid.to_string());
        fa.set_field(StreamKind::Menu, pos, "MenuID", prog.program_number.to_string());
        fa.set_field(StreamKind::Menu, pos, "Format", formats.join(" / "));

        // Menu Duration = PCR span; Menu Delay = first PCR (both 90 kHz).
        if let Some(t) = &timing {
            fa.set_field(
                StreamKind::Menu,
                pos,
                "Duration",
                format!("{:.9}", t.pcr_span_units as f64 / 90000.0),
            );
            fa.set_field(
                StreamKind::Menu,
                pos,
                "Delay",
                format!("{:.9}", t.first_pcr as f64 / 90000.0),
            );
        }

        // List the member streams of this program: StreamKind numbers and
        // positions within their kind (e.g. "1 / 2" + "0 / 0").
        if let Some(members) = menu_members.get(prog_idx)
            && !members.is_empty()
        {
            let kinds = members.iter().map(|(k, _)| k.to_string()).collect::<Vec<_>>().join(" / ");
            let poss = members.iter().map(|(_, p)| p.to_string()).collect::<Vec<_>>().join(" / ");
            fa.set_field(StreamKind::Menu, pos, "List_StreamKind", kinds);
            fa.set_field(StreamKind::Menu, pos, "List_StreamPos", poss);
        }

        // Service description from the SDT (keyed by service_id ==
        // program_number).
        if let Some(svc) = sdt_services.get(&prog.program_number) {
            if let Some(name) = &svc.name {
                fa.set_field(StreamKind::Menu, pos, "ServiceName", name.clone());
            }
            if let Some(provider) = &svc.provider {
                fa.set_field(StreamKind::Menu, pos, "ServiceProvider", provider.clone());
            }
            if let Some(t) = svc.service_type
                && let Some(name) = service_type_name(t)
            {
                fa.set_field(StreamKind::Menu, pos, "ServiceType", name);
            }
        }

        // <extra>: PMT pointer_field + section_length (mirrors the oracle).
        if let Some(ptr) = prog.pmt_pointer_field {
            fa.set_extra_field(StreamKind::Menu, pos, "pointer_field", ptr.to_string());
        }
        if let Some(len) = prog.pmt_section_length {
            fa.set_extra_field(StreamKind::Menu, pos, "section_length", len.to_string());
        }
    }

    true
}

fn prog_or_es_fid(prog: &Program, es: &ElementaryStream) -> u32 {
    if es.format_identifier != 0 { es.format_identifier } else { prog.format_identifier }
}

fn find_first_sync(buf: &[u8], layout: &PacketLayout) -> Option<usize> {
    let max_start = layout.packet_size.min(buf.len());
    'outer: for start in 0..max_start {
        for i in 0..16 {
            let pos = start + i * layout.packet_size + layout.bdav_prefix;
            if pos >= buf.len() || buf[pos] != SYNC {
                continue 'outer;
            }
        }
        return Some(start);
    }
    None
}

fn resync(buf: &[u8], from: usize, layout: &PacketLayout) -> Option<usize> {
    let max = (from + layout.packet_size * 4).min(buf.len());
    for i in from..max {
        if buf[i] == SYNC {
            // Verify by checking next packet's sync too.
            let next = i + layout.packet_size;
            if next < buf.len() && buf[next] == SYNC {
                return Some(i.saturating_sub(layout.bdav_prefix));
            }
        }
    }
    None
}

/// Timing measured from the transport stream's PCR track.
struct TsTiming {
    /// Overall bitrate (bit/s), measured from bytes-vs-PCR and rounded.
    overall_bitrate: u64,
    /// Overall duration (seconds) = file bytes × 8 / measured bitrate.
    duration_s: f64,
    /// Span between first and last PCR (90 kHz units) — the Menu Duration.
    pcr_span_units: u64,
    /// First PCR value (90 kHz units) — the Menu Delay.
    first_pcr: u64,
}

/// Measure duration and bitrate from PCR (Program Clock Reference) values.
///
/// PCR is a 33-bit 90 kHz clock in adaptation fields. MediaInfo derives the
/// TS bitrate from the number of bytes carried between the first and last
/// PCR divided by the elapsed PCR time, then computes the overall duration
/// as `file_size × 8 / bitrate`.
fn measure_ts_timing(
    buf: &[u8],
    first_offset: usize,
    stride: usize,
    bdav_prefix: usize,
) -> Option<TsTiming> {
    let mut first: Option<(usize, u64)> = None;
    let mut last: Option<(usize, u64)> = None;

    let mut pos = first_offset;
    while pos + 188 <= buf.len() {
        let sync_pos = pos + bdav_prefix;
        if sync_pos + 188 > buf.len() || buf[sync_pos] != SYNC {
            pos += stride;
            continue;
        }
        let pkt = &buf[sync_pos..sync_pos + 188];
        let adaptation_control = (pkt[3] >> 4) & 0x3;
        let has_adaptation = adaptation_control == 2 || adaptation_control == 3;
        if has_adaptation {
            let af_len = pkt[4] as usize;
            if af_len >= 7 && (pkt[5] & 0x10) != 0 {
                let pcr_base = ((pkt[6] as u64) << 25)
                    | ((pkt[7] as u64) << 17)
                    | ((pkt[8] as u64) << 9)
                    | ((pkt[9] as u64) << 1)
                    | ((pkt[10] as u64) >> 7);
                if first.is_none() {
                    first = Some((pos, pcr_base));
                }
                last = Some((pos, pcr_base));
            }
        }
        pos += stride;
    }

    let (first_pos, first_pcr) = first?;
    let (last_pos, last_pcr) = last?;
    if last_pos <= first_pos {
        return None;
    }
    // PCR span (90 kHz), wrap-safe over the 33-bit clock.
    let pcr_span_units = if last_pcr >= first_pcr {
        last_pcr - first_pcr
    } else {
        (0x1_FFFF_FFFF - first_pcr) + last_pcr
    };
    if pcr_span_units == 0 {
        return None;
    }
    let bytes_between = (last_pos - first_pos) as f64;
    let span_s = pcr_span_units as f64 / 90000.0;
    let bitrate_f = bytes_between * 8.0 / span_s;
    let overall_bitrate = bitrate_f.round() as u64;
    let duration_s = buf.len() as f64 * 8.0 / bitrate_f;

    Some(TsTiming { overall_bitrate, duration_s, pcr_span_units, first_pcr })
}

/// Count ADTS frames in an accumulated AAC elementary stream. Each ADTS
/// frame is 1024 PCM samples; the caller multiplies to obtain a sample
/// count / duration. PES headers embedded in the buffer never match the
/// 12-bit ADTS sync (`0xFFF`), so a plain scan suffices.
fn count_adts_frames(buf: &[u8]) -> u64 {
    let mut i = 0usize;
    let mut frames = 0u64;
    while i + 7 <= buf.len() {
        if buf[i] == 0xFF && (buf[i + 1] & 0xF6) == 0xF0 {
            let frame_len = (((buf[i + 3] & 0x03) as usize) << 11)
                | ((buf[i + 4] as usize) << 3)
                | ((buf[i + 5] as usize) >> 5);
            if frame_len < 7 {
                i += 1;
                continue;
            }
            frames += 1;
            i += frame_len;
        } else {
            i += 1;
        }
    }
    frames
}

fn parse_pat(section: &[u8], programs: &mut BTreeMap<u16, Program>) {
    // PAT: table_id(8) + section_syntax_indicator/etc(8) + section_length(8) <— first 3 bytes already counted
    //   + transport_stream_id(16) + version/etc(8) + section_number(8) + last_section_number(8)
    //   then N * (program_number(16) + reserved(3)+PID(13))
    //   then CRC32(32)
    if section.len() < 12 {
        return;
    }
    if section[0] != 0x00 {
        // Not a PAT.
        return;
    }
    let section_length = (((section[1] & 0x0F) as usize) << 8) | (section[2] as usize);
    let end = 3 + section_length - 4; // exclude 4-byte CRC
    if end > section.len() {
        return;
    }
    let mut i = 8; // skip table_id(1) + section_length(2) + tsid(2) + version_byte(1) + section_number(1) + last_section_number(1)
    while i + 4 <= end {
        let program_number = ((section[i] as u16) << 8) | (section[i + 1] as u16);
        let pid = (((section[i + 2] & 0x1F) as u16) << 8) | (section[i + 3] as u16);
        i += 4;
        if program_number == 0 {
            // Network PID — skip.
            continue;
        }
        programs.entry(pid).or_insert(Program {
            program_number,
            pmt_pid: pid,
            format_identifier: 0,
            streams: Vec::new(),
            pmt_section_length: None,
            pmt_pointer_field: None,
        });
    }
}

fn parse_pmt(section: &[u8], prog: &mut Program) {
    // PMT: table_id(8) + flags+section_length(16) + program_number(16) + version_byte(8)
    //   + section_number(8) + last_section_number(8) + reserved+PCR_PID(16)
    //   + reserved+program_info_length(16) + program_descriptors(N)
    //   + N * (stream_type(8) + reserved+ES_PID(16) + reserved+ES_info_length(16) + descriptors(...))
    //   + CRC32(32)
    if section.len() < 16 {
        return;
    }
    if section[0] != 0x02 {
        return;
    }
    let section_length = (((section[1] & 0x0F) as usize) << 8) | (section[2] as usize);
    let end = 3 + section_length - 4;
    if end > section.len() {
        return;
    }
    let program_info_length = (((section[10] & 0x0F) as usize) << 8) | (section[11] as usize);
    let prog_desc_start = 12;
    let prog_desc_end = prog_desc_start + program_info_length;
    if prog_desc_end > end {
        return;
    }
    // Parse program-level descriptors for registration_descriptor (0x05).
    prog.format_identifier = scan_registration(&section[prog_desc_start..prog_desc_end]);

    let mut i = prog_desc_end;
    while i + 5 <= end {
        let stream_type = section[i];
        let es_pid = (((section[i + 1] & 0x1F) as u16) << 8) | (section[i + 2] as u16);
        let es_info_length = (((section[i + 3] & 0x0F) as usize) << 8) | (section[i + 4] as usize);
        let desc_start = i + 5;
        let desc_end = desc_start + es_info_length;
        if desc_end > end {
            break;
        }
        let es_fid = scan_registration(&section[desc_start..desc_end]);
        let (es_lang, es_ac3) = scan_descriptors(&section[desc_start..desc_end]);
        prog.streams.push(ElementaryStream {
            pid: es_pid,
            stream_type,
            format_identifier: es_fid,
            _language: es_lang,
            _ac3_descriptor: es_ac3,
            aac: None,
            avc: None,
        });
        i = desc_end;
    }
}

/// Parse the SDT (Service Description Table, DVB EN 300 468 §5.2.3) into a
/// map keyed by service_id. Only the `service_descriptor` (tag 0x48) is
/// interpreted, yielding service_type + provider + service name.
///
/// Layout after the 3-byte section header:
///   transport_stream_id(16) + version_byte(8) + section_number(8)
///   + last_section_number(8) + original_network_id(16) + reserved(8)
///   then N services:
///     service_id(16) + reserved/EIT flags(8)
///     + running_status(3)/free_CA(1)/descriptors_loop_length(12)
///     + descriptors[]
///   then CRC32(32).
fn parse_sdt(section: &[u8], services: &mut BTreeMap<u16, ServiceInfo>) {
    // table_id 0x42 = SDT for the actual transport stream.
    if section.len() < 12 || section[0] != 0x42 {
        return;
    }
    let section_length = (((section[1] & 0x0F) as usize) << 8) | (section[2] as usize);
    let end = match (3 + section_length).checked_sub(4) {
        Some(e) if e <= section.len() => e,
        _ => return,
    };
    // Services begin after the 8-byte header (tsid..original_network_id +
    // reserved) that follows the 3-byte section header — i.e. at offset 11.
    let mut i = 11;
    while i + 5 <= end {
        let service_id = ((section[i] as u16) << 8) | (section[i + 1] as u16);
        let loop_len = (((section[i + 3] & 0x0F) as usize) << 8) | (section[i + 4] as usize);
        let desc_start = i + 5;
        let desc_end = desc_start + loop_len;
        if desc_end > end {
            break;
        }
        if let Some(info) = scan_service_descriptor(&section[desc_start..desc_end]) {
            services.entry(service_id).or_insert(info);
        }
        i = desc_end;
    }
}

/// Extract service_type / provider / name from a `service_descriptor`
/// (tag 0x48) within an SDT service descriptor loop.
fn scan_service_descriptor(desc_block: &[u8]) -> Option<ServiceInfo> {
    let mut i = 0;
    while i + 2 <= desc_block.len() {
        let tag = desc_block[i];
        let len = desc_block[i + 1] as usize;
        let payload_end = i + 2 + len;
        if payload_end > desc_block.len() {
            break;
        }
        if tag == 0x48 && len >= 3 {
            let data = &desc_block[i + 2..payload_end];
            let service_type = data[0];
            let provider_len = data[1] as usize;
            let provider_end = 2 + provider_len;
            if provider_end > data.len() {
                return None;
            }
            let provider = dvb_text(&data[2..provider_end]);
            let name_len_pos = provider_end;
            if name_len_pos >= data.len() {
                return None;
            }
            let name_len = data[name_len_pos] as usize;
            let name_start = name_len_pos + 1;
            let name_end = name_start + name_len;
            if name_end > data.len() {
                return None;
            }
            let name = dvb_text(&data[name_start..name_end]);
            return Some(ServiceInfo { provider, name, service_type: Some(service_type) });
        }
        i = payload_end;
    }
    None
}

/// Decode a DVB text string. FFmpeg-written SDTs use plain ASCII; a
/// leading control byte (< 0x20) selects an alternate character table and
/// is skipped. Returns None for empty strings.
fn dvb_text(bytes: &[u8]) -> Option<String> {
    let start = if bytes.first().is_some_and(|&b| b < 0x20) { 1 } else { 0 };
    let s: String = bytes[start..].iter().map(|&b| b as char).collect();
    if s.is_empty() { None } else { Some(s) }
}

/// DVB service_type (EN 300 468 Table 87) → MediaInfo ServiceType string.
fn service_type_name(t: u8) -> Option<&'static str> {
    match t {
        0x01 => Some("digital television"),
        0x02 => Some("digital radio sound"),
        0x03 => Some("Teletext"),
        0x0C => Some("data broadcast"),
        0x16 => Some("advanced codec digital SD television"),
        0x19 => Some("advanced codec digital HD television"),
        _ => None,
    }
}

fn scan_descriptors(desc_block: &[u8]) -> (Option<String>, bool) {
    let mut lang = None;
    let mut ac3 = false;
    let mut i = 0;
    while i + 2 <= desc_block.len() {
        let tag = desc_block[i];
        let len = desc_block[i + 1] as usize;
        let payload_end = i + 2 + len;
        if payload_end > desc_block.len() {
            break;
        }
        let data = &desc_block[i + 2..payload_end];
        match tag {
            0x0A if len >= 4 => {
                if let Ok(s) = std::str::from_utf8(&data[0..3]) {
                    lang = Some(s.to_string());
                }
            }
            0x6A | 0x7A => {
                ac3 = true;
            }
            _ => {}
        }
        i = payload_end;
    }
    (lang, ac3)
}

fn scan_registration(desc_block: &[u8]) -> u32 {
    let mut i = 0;
    while i + 2 <= desc_block.len() {
        let tag = desc_block[i];
        let len = desc_block[i + 1] as usize;
        let payload_end = i + 2 + len;
        if payload_end > desc_block.len() {
            break;
        }
        if tag == 0x05 && len >= 4 {
            return ((desc_block[i + 2] as u32) << 24)
                | ((desc_block[i + 3] as u32) << 16)
                | ((desc_block[i + 4] as u32) << 8)
                | (desc_block[i + 5] as u32);
        }
        i = payload_end;
    }
    0
}

const FID_HDMV: u32 = 0x48444D56; // 'HDMV' Bluray
const FID_GA94: u32 = 0x47413934; // 'GA94' ATSC A/53
const FID_S14A: u32 = 0x53313441; // 'S14A' ATSC
const FID_SCTE: u32 = 0x53435445; // 'SCTE'
const FID_CUEI: u32 = 0x43554549; // 'CUEI'
const FID_AVSV: u32 = 0x41565356; // 'AVSV'

fn stream_kind(stream_type: u8, fid: u32) -> Option<StreamKind> {
    match stream_type {
        0x01 | 0x02 | 0x10 | 0x1B | 0x1E | 0x1F | 0x20 | 0x21 | 0x24 | 0x27 | 0x32 | 0x33
        | 0x34 | 0x35 => Some(StreamKind::Video),
        0x03 | 0x04 | 0x0F | 0x11 | 0x1C | 0x2D | 0x2E => Some(StreamKind::Audio),
        0x1D => Some(StreamKind::Text),
        _ => match fid {
            FID_CUEI | FID_SCTE | FID_GA94 | FID_S14A => match stream_type {
                0x80 => Some(StreamKind::Video),
                0x81 | 0x87 => Some(StreamKind::Audio),
                0x82 => Some(StreamKind::Text),
                _ => None,
            },
            FID_HDMV => match stream_type {
                0x80..=0x86 | 0xA1 | 0xA2 => Some(StreamKind::Audio),
                0x90..=0x92 => Some(StreamKind::Text),
                0xEA => Some(StreamKind::Video),
                _ => None,
            },
            _ => match stream_type {
                0x80 => Some(StreamKind::Video),
                0x81 | 0x87 => Some(StreamKind::Audio),
                0x88 | 0xD1 => Some(StreamKind::Video),
                _ => None,
            },
        },
    }
}

fn stream_format(stream_type: u8, fid: u32) -> &'static str {
    match stream_type {
        0x01 | 0x02 => "MPEG Video",
        0x03 | 0x04 => "MPEG Audio",
        0x0F | 0x11 | 0x1C => "AAC",
        0x10 => "MPEG-4 Visual",
        0x1B | 0x1F | 0x20 => "AVC",
        0x1D => "Timed Text",
        0x1E => "MPEG Video",
        0x21 | 0x24 => "JPEG 2000",
        0x27 => "HEVC",
        0x2D | 0x2E => "MPEG-H 3D Audio",
        0x32 => "JPEG XS",
        0x33 | 0x34 => "VVC",
        0x35 => "EVC",
        _ => match fid {
            FID_AVSV => match stream_type {
                0xD0 => "AVS Video",
                0xD2 => "AVS2 Video",
                0xD4 => "AVS3 Video",
                _ => "",
            },
            FID_CUEI | FID_SCTE | FID_GA94 | FID_S14A => match stream_type {
                0x80 => "MPEG Video",
                0x81 => "AC-3",
                0x82 => "Text",
                0x86 => "SCTE 35",
                0x87 => "E-AC-3",
                _ => "",
            },
            FID_HDMV => match stream_type {
                0x80 => "PCM",
                0x81 | 0x83 | 0xA1 => "AC-3",
                0x82 | 0x85 | 0x86 | 0xA2 => "DTS",
                0x84 => "E-AC-3",
                0x90 | 0x91 => "PGS",
                0x92 => "TEXTST",
                0xEA => "VC-1",
                _ => "",
            },
            _ => match stream_type {
                0x80 => "MPEG Video",
                0x81 => "AC-3",
                0x87 => "E-AC-3",
                0x88 => "VC-1",
                0xD1 => "Dirac",
                _ => "",
            },
        },
    }
}

fn stream_codec(stream_type: u8, fid: u32) -> &'static str {
    match stream_type {
        0x01 => "MPEG-1V",
        0x02 | 0x1E => "MPEG-2V",
        0x03 => "MPEG-1A",
        0x04 => "MPEG-2A",
        0x0F | 0x11 | 0x1C => "AAC",
        0x10 => "MPEG-4V",
        0x1B | 0x1F | 0x20 => "AVC",
        0x1D => "Text",
        0x24 | 0x27 => "HEVC",
        _ => match fid {
            FID_CUEI | FID_SCTE | FID_GA94 | FID_S14A => match stream_type {
                0x80 => "MPEG-2V",
                0x81 => "AC3",
                0x82 => "Text",
                0x87 => "AC3+",
                _ => "",
            },
            FID_HDMV => match stream_type {
                0x80 => "PCM",
                0x81 | 0x83 => "AC3",
                0x82 | 0x86 => "DTS",
                0x90 | 0x91 => "PGS",
                0x92 => "TEXTST",
                0xEA => "VC1",
                _ => "",
            },
            _ => match stream_type {
                0x80 => "MPEG-2V",
                0x81 => "AC3",
                0x87 => "AC3+",
                0x88 => "VC-1",
                0xD1 => "Dirac",
                _ => "",
            },
        },
    }
}

/// Parse PES header to extract PTS (Presentation Timestamp).
/// PES header format:
///   0-2: packet_start_code_prefix (0x000001)
///   3: stream_id
///   4-5: PES_packet_length (can be 0 for video)
///   6: flags (10, 11 bits reserved, 2 bits scrambling, 1 bit priority, 1 bit alignment, 1 bit copyright, 1 bit original)
///   7: more flags (PTS_DTS_flags, ESCR_flag, ES_rate_flag, DSM_trick_mode_flag, etc.)
///   8: PES_header_data_length
///   9+: optional fields based on flags
#[allow(dead_code)]
fn parse_pes_pts(buf: &[u8]) -> Option<u64> {
    if buf.len() < 9 {
        return None;
    }
    // Check packet_start_code_prefix
    if buf[0] != 0x00 || buf[1] != 0x00 || buf[2] != 0x01 {
        return None;
    }
    let stream_id = buf[3];
    // Video streams: 0xE0-0xEF, Audio streams: 0xC0-0xDF
    let is_video = (0xE0..=0xEF).contains(&stream_id);
    let is_audio = (0xC0..=0xDF).contains(&stream_id);
    if !is_video && !is_audio {
        return None;
    }

    // Skip PES_packet_length (2 bytes) to get to flags
    let _flags1 = buf[6];
    let flags2 = buf[7];
    let pes_header_len = buf[8] as usize;

    if buf.len() < 9 + pes_header_len {
        return None;
    }

    // Check PTS_DTS_flags (bits 5-6 of flags2)
    let pts_dts_flags = (flags2 >> 6) & 0x3;
    if pts_dts_flags == 0 {
        return None; // No PTS present
    }

    // PTS is in bytes 9-13 (5 bytes) when present
    // Format: 4 bits '0010' or '0011' + 33-bit PTS value
    if buf.len() < 14 {
        return None;
    }

    let pts_byte1 = buf[9];
    let pts_byte2 = buf[10];
    let pts_byte3 = buf[11];
    let pts_byte4 = buf[12];
    let pts_byte5 = buf[13];

    // Extract 33-bit PTS: marker bits at positions
    // Bits: [4 marker bits][3 bits][1 marker][15 bits][1 marker][15 bits][1 marker]
    let pts: u64 = (((pts_byte1 as u64) & 0x0E) << 29)
        | ((pts_byte2 as u64) << 22)
        | (((pts_byte3 as u64) & 0xFE) << 14)
        | ((pts_byte4 as u64) << 7)
        | ((pts_byte5 as u64) >> 1);

    Some(pts)
}

/// Extract frame rate from AVC/H.264 sequence parameter set in PES payload.
/// Returns frame rate as f64 (frames per second).
#[allow(dead_code)]
fn extract_avc_frame_rate(pes_payload: &[u8]) -> Option<f64> {
    // Look for SPS NAL unit: nal_unit_type = 7
    // NAL header: 1 byte (forbidden_zero_bit | nal_ref_idc | nal_unit_type)
    // We need to find 0x67 or 0x27 (nal_unit_type = 7, different nal_ref_idc values)

    for i in 0..pes_payload.len().saturating_sub(5) {
        let nal_type = pes_payload[i] & 0x1F;
        if nal_type == 7 {
            // SPS
            // Skip NAL header (1 byte) and start parsing SPS
            let sps = &pes_payload[i..];
            if sps.len() < 5 {
                continue;
            }

            // Try to extract frame rate from VUI if present
            // This is a simplified extraction - full SPS parsing is complex
            // Look for VUI presence and timing_info_present_flag

            // For now, return common frame rates based on profile/level
            // or extract from VUI if we can find it
            return parse_sps_for_frame_rate(sps);
        }
    }
    None
}

/// Parse SPS to extract frame rate from VUI timing_info.
#[allow(dead_code)]
fn parse_sps_for_frame_rate(sps: &[u8]) -> Option<f64> {
    // Simplified: skip to VUI parameters
    // Real implementation would need full Exp-Golomb decoding

    // Common frame rates for broadcast
    // If we can't parse, return None and let the caller use defaults

    // Try to find VUI and timing_info
    // This is a heuristic search for timing_info_present_flag pattern
    for i in 10..sps.len().saturating_sub(10) {
        // Look for patterns that suggest timing info
        // time_scale and num_units_in_tick are key values
        if sps[i] == 0 && sps[i + 1] == 0 && sps[i + 2] == 0 && sps[i + 3] == 1 {
            // Found start code, skip
            continue;
        }
    }

    // Return None for now - would need full bitstream parsing
    None
}

/// Calculate frame rate from PTS differences in multiple PES packets.
/// This is used when we have multiple PCR/PTS samples from the same PID.
#[allow(dead_code)]
fn calculate_frame_rate_from_pts(pts_samples: &[(usize, u64)]) -> Option<f64> {
    if pts_samples.len() < 2 {
        return None;
    }

    // PTS is in 90kHz units
    // Calculate average frame duration
    let mut total_duration_pts: u64 = 0;
    let mut count = 0;

    for window in pts_samples.windows(2) {
        let pts_diff = if window[1].1 >= window[0].1 {
            window[1].1 - window[0].1
        } else {
            // PTS wraparound (33-bit)
            (0x1FFFFFFFF - window[0].1) + window[1].1
        };
        total_duration_pts += pts_diff;
        count += 1;
    }

    if count == 0 {
        return None;
    }

    let avg_duration_pts = total_duration_pts / count as u64;
    // Frame rate = 90000 / avg_duration_pts
    if avg_duration_pts > 0 {
        let fps = 90000.0 / avg_duration_pts as f64;
        // Round to common frame rates
        return Some(round_to_common_fps(fps));
    }
    None
}

/// Round calculated FPS to common broadcast frame rates.
#[allow(dead_code)]
fn round_to_common_fps(fps: f64) -> f64 {
    const COMMON_RATES: [f64; 8] = [23.976, 24.0, 25.0, 29.97, 30.0, 50.0, 59.94, 60.0];

    let mut closest = fps;
    let mut min_diff = f64::MAX;

    for &rate in &COMMON_RATES {
        let diff = (fps - rate).abs();
        if diff < min_diff {
            min_diff = diff;
            closest = rate;
        }
    }

    // Only accept if within 1% of common rate
    if min_diff / closest < 0.01 { closest } else { fps }
}

/// Scan a PES payload accumulator for the first ADTS sync (0xFFF) and
/// decode the header. Returns None if no sync found in the first 1 KiB
/// or if the header fields are invalid.
fn sniff_aac_adts(buf: &[u8]) -> Option<AacInfo> {
    const SAMPLE_RATE_TABLE: [u32; 13] =
        [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
    for i in 0..buf.len().saturating_sub(7) {
        if buf[i] == 0xFF && (buf[i + 1] & 0xF0) == 0xF0 {
            let profile = (buf[i + 2] >> 6) & 0x3;
            let sample_rate_idx = ((buf[i + 2] >> 2) & 0xF) as usize;
            let channel_config = ((buf[i + 2] & 0x1) << 2) | ((buf[i + 3] >> 6) & 0x3);
            if sample_rate_idx >= SAMPLE_RATE_TABLE.len() {
                return None;
            }
            let sample_rate = SAMPLE_RATE_TABLE[sample_rate_idx];
            let channels = match channel_config {
                0 => 0,
                1..=6 => channel_config,
                7 => 8,
                _ => 0,
            };
            if channels == 0 || sample_rate == 0 {
                return None;
            }
            return Some(AacInfo { aot: profile + 1, sample_rate, channels });
        }
    }
    None
}

fn aac_profile_name(aot: u8) -> Option<&'static str> {
    match aot {
        1 => Some("Main"),
        2 => Some("LC"),
        3 => Some("SSR"),
        4 => Some("LTP"),
        5 => Some("SBR"),
        _ => None,
    }
}

/// Find the next Annex-B start code (`00 00 01`) at or after `offset`.
/// A 4-byte start code (`00 00 00 01`) is located at its `00 00 01`
/// suffix; the leading zero is left as trailing data of the prior NAL,
/// which the RBSP parsers ignore.
fn next_start_code(data: &[u8], offset: usize) -> Option<usize> {
    let mut i = offset;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Recover AVC parameters from an accumulated PES payload.
///
/// The buffer holds concatenated PES packet payloads for one AVC PID: the
/// first packet begins with a PES header (`00 00 01 E0 …`) and every
/// subsequent same-PID packet appends raw Annex-B elementary-stream bytes.
/// PES headers decode as NAL type 0 (`0xE0 & 0x1F`) and are ignored, so a
/// plain start-code scan cleanly isolates the SPS/PPS/SEI NAL units.
fn sniff_avc(buf: &[u8]) -> Option<AvcEs> {
    let mut sps: Option<&[u8]> = None;
    let mut pps: Option<&[u8]> = None;
    let mut seis: Vec<&[u8]> = Vec::new();

    let mut off = 0usize;
    while let Some(sc) = next_start_code(buf, off) {
        let nal_start = sc + 3;
        if nal_start >= buf.len() {
            break;
        }
        let nal_end = next_start_code(buf, nal_start).unwrap_or(buf.len());
        let nal = &buf[nal_start..nal_end];
        if !nal.is_empty() {
            match nal[0] & 0x1F {
                7 if sps.is_none() => sps = Some(nal),
                8 if pps.is_none() => pps = Some(nal),
                6 => seis.push(nal),
                _ => {}
            }
        }
        off = nal_end;
    }

    let sps = sps?;
    let info = revelo_parsers_video::parse_avc_sps(sps)?;
    // constraint_set1_flag lives in the byte after profile_idc; the first
    // four SPS bytes (header, profile_idc, constraint flags, level_idc)
    // never contain emulation-prevention sequences.
    let constrained = sps.len() > 2 && (sps[2] & 0x40) != 0;
    let cabac = pps.and_then(parse_pps_cabac);
    let encoder = if seis.is_empty() {
        None
    } else {
        revelo_parsers_video::extract_encoder_from_avc_sei_nalus(&seis)
    };
    Some(AvcEs { info, constrained, cabac, encoder })
}

/// Read the entropy_coding_mode_flag from a PPS NAL (1 = CABAC). Layout:
/// NAL header (1 byte) + ue(pic_parameter_set_id) + ue(seq_parameter_set_id)
/// + 1 bit entropy_coding_mode_flag.
fn parse_pps_cabac(pps: &[u8]) -> Option<bool> {
    if pps.len() < 2 {
        return None;
    }
    let clean = remove_epb(&pps[1..]);
    let mut off = 0usize;
    read_ue(&clean, &mut off)?; // pic_parameter_set_id
    read_ue(&clean, &mut off)?; // seq_parameter_set_id
    read_bit(&clean, &mut off).map(|b| b == 1)
}

/// Strip 0x000003 emulation-prevention bytes (collapse to 0x0000).
fn remove_epb(rbsp: &[u8]) -> Vec<u8> {
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

/// Read one MSB-first bit from `buf` at bit offset `off`.
fn read_bit(buf: &[u8], off: &mut usize) -> Option<u32> {
    let byte = *off / 8;
    if byte >= buf.len() {
        return None;
    }
    let bit = 7 - (*off % 8);
    *off += 1;
    Some(((buf[byte] >> bit) & 1) as u32)
}

/// Read an unsigned Exp-Golomb `ue(v)` value (H.264 §9.1).
fn read_ue(buf: &[u8], off: &mut usize) -> Option<u32> {
    let mut zeros = 0u32;
    while read_bit(buf, off)? == 0 {
        zeros += 1;
        if zeros > 31 {
            return None;
        }
    }
    let mut val = 0u32;
    for _ in 0..zeros {
        val = (val << 1) | read_bit(buf, off)?;
    }
    Some(val + (1u32 << zeros) - 1)
}

/// AVC profile_idc → MediaInfo Format_Profile string. `constrained` is
/// the constraint_set1_flag, which promotes Baseline to "Constrained
/// Baseline". Mirrors the avcC mapping in `mp4.rs`.
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

/// AVC level_idc → "X" or "X.Y" (level number is encoded ×10):
/// 13 → "1.3", 30 → "3", 41 → "4.1".
fn format_avc_level(idc: u8) -> String {
    let major = idc / 10;
    let minor = idc % 10;
    if minor == 0 { format!("{major}") } else { format!("{major}.{minor}") }
}

/// Map SPS/PPS/SEI-derived [`AvcEs`] to MediaInfo Video fields. Mirrors
/// the avcC→Video mapping in `mp4.rs` so MPEG-TS and MP4 agree.
fn emit_avc_fields(fa: &mut FileAnalyze, kind: StreamKind, pos: usize, avc: &AvcEs) {
    let info = &avc.info;
    if let Some(profile) = avc_profile_name(info.profile, avc.constrained) {
        fa.set_field(kind, pos, "Format_Profile", profile);
    }
    fa.set_field(kind, pos, "Format_Level", format_avc_level(info.level));
    // CABAC from the PPS; Baseline cannot use CABAC, so fall back to "No".
    let cabac = match avc.cabac {
        Some(true) => Some("Yes"),
        Some(false) => Some("No"),
        None if info.profile == 0x42 => Some("No"),
        None => None,
    };
    if let Some(c) = cabac {
        fa.set_field(kind, pos, "Format_Settings_CABAC", c);
    }
    fa.set_field(kind, pos, "Format_Settings_RefFrames", info.ref_frames.to_string());

    if info.width > 0 && info.height > 0 {
        fa.set_field(kind, pos, "Width", info.width.to_string());
        fa.set_field(kind, pos, "Height", info.height.to_string());
        // PixelAspectRatio from VUI SAR (default 1:1 when absent).
        let (sar_w, sar_h) = match info.sar {
            Some((w, h)) if h > 0 => (w as f64, h as f64),
            _ => (1.0, 1.0),
        };
        let par = sar_w / sar_h;
        let sampled_w = (info.width as f64 * par).round() as u64;
        let dar = sampled_w as f64 / info.height as f64;
        fa.set_field(kind, pos, "Sampled_Width", sampled_w.to_string());
        fa.set_field(kind, pos, "Sampled_Height", info.height.to_string());
        fa.set_field(kind, pos, "PixelAspectRatio", format!("{par:.3}"));
        fa.set_field(kind, pos, "DisplayAspectRatio", format!("{dar:.3}"));
    }

    fa.set_field(kind, pos, "ColorSpace", "YUV");
    let chroma = match info.chroma_format {
        0 => "4:0:0",
        2 => "4:2:2",
        3 => "4:4:4",
        _ => "4:2:0",
    };
    fa.set_field(kind, pos, "ChromaSubsampling", chroma);
    let bit_depth = if info.bit_depth > 0 { info.bit_depth } else { 8 };
    fa.set_field(kind, pos, "BitDepth", bit_depth.to_string());
    fa.set_field(kind, pos, "ScanType", "Progressive");

    if let Some(enc) = &avc.encoder {
        fa.set_field(kind, pos, "Encoded_Library", enc.library.clone());
        if let Some(name) = &enc.name {
            fa.set_field(kind, pos, "Encoded_Library_Name", name.clone());
        }
        if let Some(ver) = &enc.version {
            fa.set_field(kind, pos, "Encoded_Library_Version", ver.clone());
        }
        if let Some(settings) = &enc.settings {
            fa.set_field(kind, pos, "Encoded_Library_Settings", settings.clone());
        }
    }
}

fn aac_channel_layout(channels: u8) -> (Option<&'static str>, Option<&'static str>) {
    match channels {
        1 => (Some("Front: C"), Some("M")),
        2 => (Some("Front: L R"), Some("L R")),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_core::FileAnalyze;

    const MPEG_TS_METADATA_ONLY_BUDGET: u64 = 8 * 1024 * 1024;

    #[test]
    fn rejects_non_ts() {
        let mut fa = FileAnalyze::new(b"NOT A TS FILE AT ALL");
        assert!(!parse_mpeg_ts(&mut fa));
    }

    /// Builds a minimal MPEG-TS file with one PAT and one PMT declaring
    /// one MPEG-2 video and one AAC audio elementary stream. Used only
    /// for unit coverage — not byte-equal to a real-world TS.
    fn build_synthetic_ts() -> Vec<u8> {
        // Build PAT section
        let pat_payload = build_pat_section(1, 0x1000);
        let pmt_payload = build_pmt_section(1, 0x100, &[(0x02, 0x101), (0x0F, 0x102)]);
        let mut out = Vec::new();
        out.extend(build_psi_packet(0x0000, &pat_payload));
        out.extend(build_psi_packet(0x1000, &pmt_payload));
        // Add 14 more null packets so the layout detector sees 16 syncs.
        for _ in 0..14 {
            out.extend(build_null_packet());
        }
        out
    }

    fn build_psi_packet(pid: u16, section: &[u8]) -> Vec<u8> {
        let mut pkt = vec![0u8; 188];
        pkt[0] = 0x47;
        pkt[1] = 0x40 | ((pid >> 8) as u8 & 0x1F); // payload_unit_start = 1
        pkt[2] = pid as u8;
        pkt[3] = 0x10; // adaptation_field_control = 01 (payload only), continuity_counter = 0
        pkt[4] = 0x00; // pointer_field
        let copy = section.len().min(183);
        pkt[5..5 + copy].copy_from_slice(&section[..copy]);
        pkt
    }

    fn build_null_packet() -> Vec<u8> {
        let mut pkt = vec![0xFFu8; 188];
        pkt[0] = 0x47;
        pkt[1] = 0x1F; // PID = 0x1FFF (null)
        pkt[2] = 0xFF;
        pkt[3] = 0x10;
        pkt
    }

    fn build_pat_section(program_number: u16, pmt_pid: u16) -> Vec<u8> {
        // table_id(1) + 2(section_length+flags) + 5(header) + 4(program loop) + 4(CRC) = 16
        let section_length: u16 = 9 + 4; // header(5) + body(4) + crc(4)
        let mut s = Vec::with_capacity(3 + section_length as usize);
        s.push(0x00); // table_id
        s.push(0xB0 | ((section_length >> 8) as u8 & 0x0F)); // section_syntax_indicator=1, '0', reserved
        s.push(section_length as u8);
        s.extend_from_slice(&[0x00, 0x01]); // transport_stream_id
        s.push(0xC1); // reserved(2) + version_number(0) + current_next_indicator(1)
        s.push(0x00); // section_number
        s.push(0x00); // last_section_number
        s.extend_from_slice(&program_number.to_be_bytes());
        s.extend_from_slice(&((0xE000u16 | (pmt_pid & 0x1FFF)).to_be_bytes()));
        s.extend_from_slice(&[0, 0, 0, 0]); // CRC placeholder (ignored by parser)
        s
    }

    fn build_pmt_section(program_number: u16, pcr_pid: u16, streams: &[(u8, u16)]) -> Vec<u8> {
        let body_len: u16 = streams.iter().map(|_| 5u16).sum();
        let section_length: u16 = 9 + body_len + 4;
        let mut s = Vec::with_capacity(3 + section_length as usize);
        s.push(0x02);
        s.push(0xB0 | ((section_length >> 8) as u8 & 0x0F));
        s.push(section_length as u8);
        s.extend_from_slice(&program_number.to_be_bytes());
        s.push(0xC1);
        s.push(0x00);
        s.push(0x00);
        s.extend_from_slice(&((0xE000u16 | (pcr_pid & 0x1FFF)).to_be_bytes()));
        s.extend_from_slice(&[0xF0, 0x00]); // program_info_length = 0
        for &(stype, pid) in streams {
            s.push(stype);
            s.extend_from_slice(&((0xE000u16 | (pid & 0x1FFF)).to_be_bytes()));
            s.extend_from_slice(&[0xF0, 0x00]); // ES_info_length = 0
        }
        s.extend_from_slice(&[0, 0, 0, 0]);
        s
    }

    #[test]
    fn parses_synthetic_ts() {
        let buf = build_synthetic_ts();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpeg_ts(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG-TS".to_owned())
        );
        assert_eq!(fa.stream_count(StreamKind::Video), 1);
        assert_eq!(fa.stream_count(StreamKind::Audio), 1);
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG Video".to_owned())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("AAC".to_owned())
        );
    }

    #[test]
    fn ts_probe_is_bounded_on_large_inputs() {
        let mut buf = build_synthetic_ts();
        buf.resize(MPEG_TS_PROBE_LIMIT + 188, 0);
        let mut fa = FileAnalyze::new(&buf);

        assert!(parse_mpeg_ts(&mut fa));
        let stats = fa.access_stats();
        assert!(stats.bytes_requested < MPEG_TS_METADATA_ONLY_BUDGET, "{stats:?}");
        assert!(stats.bytes_returned < MPEG_TS_METADATA_ONLY_BUDGET, "{stats:?}");
        assert_eq!(stats.max_request_len, MPEG_TS_PROBE_LIMIT);
    }

    // Real Constrained-Baseline SPS/PPS (320x240, level 1.3) extracted
    // from an x264/FFmpeg MPEG-TS elementary stream.
    const REAL_SPS: [u8; 23] = [
        0x67, 0x42, 0xc0, 0x0d, 0xda, 0x05, 0x07, 0xec, 0x04, 0x40, 0x00, 0x00, 0x03, 0x00, 0x40,
        0x00, 0x00, 0x0c, 0x83, 0xc5, 0x0a, 0xa8, 0x00,
    ];
    const REAL_PPS: [u8; 4] = [0x68, 0xce, 0x0f, 0xc8];

    #[test]
    fn sniff_avc_maps_sps_pps_from_annex_b() {
        // Annex-B stream: SPS then PPS, each preceded by a start code.
        let mut es = Vec::new();
        es.extend_from_slice(&[0, 0, 1]);
        es.extend_from_slice(&REAL_SPS);
        es.extend_from_slice(&[0, 0, 1]);
        es.extend_from_slice(&REAL_PPS);

        let avc = sniff_avc(&es).expect("SPS should parse");
        assert_eq!(avc.info.profile, 0x42);
        assert!(avc.constrained, "constraint_set1_flag set → Constrained Baseline");
        assert_eq!(avc.info.level, 13);
        assert_eq!(avc.info.width, 320);
        assert_eq!(avc.info.height, 240);
        assert_eq!(avc.info.chroma_format, 1); // 4:2:0
        assert_eq!(avc.info.ref_frames, 1);
        assert_eq!(avc.cabac, Some(false)); // entropy_coding_mode_flag = 0

        // Field mapping mirrors the mp4.rs avcC template.
        assert_eq!(
            avc_profile_name(avc.info.profile, avc.constrained),
            Some("Constrained Baseline")
        );
        assert_eq!(format_avc_level(avc.info.level), "1.3");
    }

    #[test]
    fn sniff_avc_handles_4byte_start_codes_and_leading_pes_header() {
        // Prepend a PES header (start code + stream_id 0xE0) which must be
        // ignored (NAL type 0), and use 4-byte start codes.
        let mut es = Vec::new();
        es.extend_from_slice(&[0, 0, 1, 0xE0, 0x00, 0x00, 0x80, 0x80, 0x05, 0, 0, 0, 0, 0]);
        es.extend_from_slice(&[0, 0, 0, 1]);
        es.extend_from_slice(&REAL_SPS);
        es.extend_from_slice(&[0, 0, 0, 1]);
        es.extend_from_slice(&REAL_PPS);

        let avc = sniff_avc(&es).expect("SPS should parse past PES header");
        assert_eq!(avc.info.width, 320);
        assert_eq!(avc.info.height, 240);
    }

    #[test]
    fn count_adts_frames_counts_syncs() {
        // Two minimal ADTS frames of length 8 bytes each.
        let frame = [0xFF, 0xF1, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
        let mut buf = Vec::new();
        buf.extend_from_slice(&frame);
        buf.extend_from_slice(&frame);
        assert_eq!(count_adts_frames(&buf), 2);
    }
}
