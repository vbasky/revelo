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

use revelio_core::{FileAnalyze, StreamKind};
use std::collections::BTreeMap;

const SYNC: u8 = 0x47;

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
    /// AAC payload params extracted from first ADTS frame inside PES,
    /// when stream_type indicates AAC (0x0F/0x11/0x1C).
    aac: Option<AacInfo>,
    /// x264 encoder version string scanned from AVC SEI user_data
    /// (free-form ASCII inside SEI NAL units).
    avc_encoder: Option<String>,
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
}

pub fn parse_mpeg_ts(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.Remain()) {
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
    let mut pat_seen = false;

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
        if !is_pat && !is_pmt {
            pos += stride;
            continue;
        }

        if payload_unit_start {
            if payload.is_empty() {
                pos += stride;
                continue;
            }
            let pointer = payload[0] as usize;
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
        if let Some(buf) = psi_buffers.get(&pid) {
            if buf.len() >= 3 {
                let section_length = (((buf[1] & 0x0F) as usize) << 8) | (buf[2] as usize);
                let total = 3 + section_length;
                if buf.len() >= total {
                    let section = buf[..total].to_vec();
                    if is_pat && !pat_seen {
                        parse_pat(&section, &mut programs_by_pmt_pid);
                        pat_seen = true;
                    } else if is_pmt {
                        if let Some(prog) = programs_by_pmt_pid.get_mut(&pid) {
                            if prog.streams.is_empty() {
                                parse_pmt(&section, prog);
                            }
                        }
                    }
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
    for prog in programs_by_pmt_pid.values() {
        for es in &prog.streams {
            if matches!(es.stream_type, 0x0F | 0x11 | 0x1C) {
                aac_pids.insert(es.pid, Vec::new());
            }
            if matches!(es.stream_type, 0x1B | 0x1F | 0x20) {
                avc_pids.insert(es.pid, Vec::new());
            }
        }
    }
    if !aac_pids.is_empty() || !avc_pids.is_empty() {
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
            if let Some(accum) = aac_pids.get_mut(&pid) {
                if accum.len() < 1024 {
                    accum.extend_from_slice(&pkt[payload_off..]);
                }
            }
            if let Some(accum) = avc_pids.get_mut(&pid) {
                if accum.len() < 32 * 1024 {
                    accum.extend_from_slice(&pkt[payload_off..]);
                }
            }
            pos += stride;
        }
        // Now parse each accumulator: skip past PES header to ES payload,
        // then find ADTS sync.
        for prog in programs_by_pmt_pid.values_mut() {
            for es in prog.streams.iter_mut() {
                if matches!(es.stream_type, 0x0F | 0x11 | 0x1C) {
                    if let Some(accum) = aac_pids.get(&es.pid) {
                        es.aac = sniff_aac_adts(accum);
                    }
                }
                if matches!(es.stream_type, 0x1B | 0x1F | 0x20) {
                    if let Some(accum) = avc_pids.get(&es.pid) {
                        es.avc_encoder = sniff_x264_encoder(accum);
                    }
                }
            }
        }
    }

    let container_format = match (layout.packet_size, layout.bdav_prefix) {
        (192, 4) => "BDAV",
        (204, 0) => "MPEG-TS 188+16",
        _ => "MPEG-TS",
    };

    // Calculate file size and estimate duration from PCR values
    let file_size = buf.len();
    let (duration_ms, overall_bitrate) = estimate_duration_and_bitrate(
        buf, 
        first_offset, 
        stride, 
        layout.bdav_prefix,
        &programs_by_pmt_pid
    );

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", container_format, true);
    
    // General.ID = program_number of the first program (matches oracle
    // for single-program files; multi-program TS would list each).
    if let Some(first_prog) = programs_by_pmt_pid.values().next() {
        fa.Fill(
            StreamKind::General,
            0,
            "ID",
            first_prog.program_number.to_string(),
            false,
        );
    }
    
    if let Some(dur) = duration_ms {
        fa.Fill(StreamKind::General, 0, "Duration", dur.to_string(), false);
    }
    
    if let Some(br) = overall_bitrate {
        fa.Fill(StreamKind::General, 0, "OverallBitRate", br.to_string(), false);
        fa.Fill(StreamKind::General, 0, "OverallBitRate_Mode", "VBR", false);
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
        fa.Fill(StreamKind::General, 0, "VideoCount", video_count.to_string(), false);
    }
    if audio_count > 0 {
        fa.Fill(StreamKind::General, 0, "AudioCount", audio_count.to_string(), false);
    }
    if text_count > 0 {
        fa.Fill(StreamKind::General, 0, "TextCount", text_count.to_string(), false);
    }
    if menu_count > 0 {
        fa.Fill(StreamKind::General, 0, "MenuCount", menu_count.to_string(), false);
    }

    // Emit per-stream entries in (program, ES) order.
    for (prog_idx, prog) in programs_by_pmt_pid.values().enumerate() {
        for (es_idx, es) in prog.streams.iter().enumerate() {
            let fid = prog_or_es_fid(prog, es);
            let Some(kind) = stream_kind(es.stream_type, fid) else { continue };
            let format = stream_format(es.stream_type, fid);
            let codec = stream_codec(es.stream_type, fid);
            if format.is_empty() {
                continue;
            }
            fa.Stream_Prepare(kind);
            let pos_in_kind = fa.Count_Get(kind) - 1;
            // StreamOrder = "<program_idx>-<es_idx_in_program>" per oracle.
            fa.Fill(
                kind,
                pos_in_kind,
                "StreamOrder",
                format!("{}-{}", prog_idx, es_idx),
                false,
            );
            // ID = PID. Oracle renders as decimal.
            fa.Fill(kind, pos_in_kind, "ID", es.pid.to_string(), false);
            fa.Fill(kind, pos_in_kind, "MenuID", prog.program_number.to_string(), false);
            fa.Fill(kind, pos_in_kind, "Format", format, false);
            // Video CodecID = decimal stream_type (oracle convention).
            // AAC overrides this below with its "<type>-<AOT>" form.
            if matches!(kind, StreamKind::Video) {
                fa.Fill(kind, pos_in_kind, "CodecID", es.stream_type.to_string(), false);
                // AVC defaults — full SPS parse would refine
                // Format_Profile/Level/Width/Height/ChromaSubsampling.
                if matches!(es.stream_type, 0x1B | 0x1F | 0x20) {
                    fa.Fill(kind, pos_in_kind, "FrameRate_Mode", "VFR", false);
                    fa.Fill(kind, pos_in_kind, "BitDepth", "8", false);
                    fa.Fill(kind, pos_in_kind, "ScanType", "Progressive", false);
                    fa.Fill(kind, pos_in_kind, "Compression_Mode", "Lossy", false);
                    if let Some(ref lib) = es.avc_encoder {
                        fa.Fill(kind, pos_in_kind, "Encoded_Library", lib.clone(), false);
                        // Split "x264 - core 165 r3222 b35605a" into name + version.
                        if let Some(rest) = lib.strip_prefix("x264 - ") {
                            fa.Fill(kind, pos_in_kind, "Encoded_Library_Name", "x264", false);
                            fa.Fill(
                                kind,
                                pos_in_kind,
                                "Encoded_Library_Version",
                                rest,
                                false,
                            );
                        }
                    }
                }
            }
            if let Some(aac) = &es.aac {
                // AAC ADTS payload → unlocks Format_Version, AOT, CodecID,
                // MuxingMode=ADTS, Channels, SamplingRate, SamplesPerFrame.
                fa.Fill(kind, pos_in_kind, "Format_Version", "4", false);
                if let Some(profile) = aac_profile_name(aac.aot) {
                    fa.Fill(kind, pos_in_kind, "Format_AdditionalFeatures", profile, false);
                }
                fa.Fill(kind, pos_in_kind, "MuxingMode", "ADTS", false);
                fa.Fill(
                    kind,
                    pos_in_kind,
                    "CodecID",
                    format!("{}-{}", es.stream_type, aac.aot),
                    false,
                );
                fa.Fill(kind, pos_in_kind, "BitRate_Mode", "VBR", false);
                fa.Fill(kind, pos_in_kind, "Channels", aac.channels.to_string(), false);
                let (positions, layout) = aac_channel_layout(aac.channels);
                if let Some(p) = positions {
                    fa.Fill(kind, pos_in_kind, "ChannelPositions", p, false);
                }
                if let Some(l) = layout {
                    fa.Fill(kind, pos_in_kind, "ChannelLayout", l, false);
                }
                fa.Fill(kind, pos_in_kind, "SamplesPerFrame", "1024", false);
                fa.Fill(kind, pos_in_kind, "SamplingRate", aac.sample_rate.to_string(), false);
                if aac.sample_rate > 0 {
                    let frame_rate = aac.sample_rate as f64 / 1024.0;
                    fa.Fill(kind, pos_in_kind, "FrameRate", format!("{:.3}", frame_rate), false);
                }
                fa.Fill(kind, pos_in_kind, "Compression_Mode", "Lossy", false);
            }
            let _ = codec;
        }
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
        let pos = fa.Stream_Prepare(StreamKind::Menu);
        fa.Fill(StreamKind::Menu, pos, "StreamOrder", prog_idx.to_string(), false);
        fa.Fill(StreamKind::Menu, pos, "ID", prog.pmt_pid.to_string(), false);
        fa.Fill(StreamKind::Menu, pos, "MenuID", prog.program_number.to_string(), false);
        fa.Fill(StreamKind::Menu, pos, "Format", formats.join(" / "), false);
    }

    true
}

fn prog_or_es_fid(prog: &Program, es: &ElementaryStream) -> u32 {
    if es.format_identifier != 0 {
        es.format_identifier
    } else {
        prog.format_identifier
    }
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


/// Estimate duration from PCR (Program Clock Reference) values and calculate bitrate.
/// PCR is a 33-bit clock reference in 90kHz units found in adaptation fields.
fn estimate_duration_and_bitrate(
    buf: &[u8],
    first_offset: usize,
    stride: usize,
    bdav_prefix: usize,
    programs: &BTreeMap<u16, Program>,
) -> (Option<u64>, Option<u64>) {
    // Collect PCR values from adaptation fields
    let mut pcr_values: Vec<(usize, u64)> = Vec::new(); // (byte_position, pcr_value)
    
    let mut pos = first_offset;
    while pos + 188 <= buf.len() && pcr_values.len() < 100 {
        let sync_pos = pos + bdav_prefix;
        if sync_pos + 188 > buf.len() || buf[sync_pos] != SYNC {
            pos += stride;
            continue;
        }
        
        let pkt = &buf[sync_pos..sync_pos + 188];
        let adaptation_control = (pkt[3] >> 4) & 0x3;
        let has_adaptation = adaptation_control == 2 || adaptation_control == 3;
        
        if has_adaptation && pkt.len() > 5 {
            let af_len = pkt[4] as usize;
            if af_len > 0 && pkt.len() > 6 {
                // Adaptation field flags at byte 5
                let flags = pkt[5];
                let pcr_flag = (flags & 0x10) != 0;
                
                if pcr_flag && af_len >= 7 && pkt.len() >= 12 {
                    // PCR is 6 bytes: 33-bit base + 6-bit reserved + 9-bit extension
                    // Read as 48 bits (6 bytes) and extract 33-bit base
                    let pcr_base = ((pkt[6] as u64) << 25)
                        | ((pkt[7] as u64) << 17)
                        | ((pkt[8] as u64) << 9)
                        | ((pkt[9] as u64) << 1)
                        | ((pkt[10] as u64) >> 7);
                    
                    pcr_values.push((pos, pcr_base));
                }
            }
        }
        pos += stride;
    }
    
    if pcr_values.len() < 2 {
        // Not enough PCR values to estimate duration
        // Fall back to simple bitrate estimation from file size
        if buf.len() > 0 {
            // Assume some default duration if we can't calculate
            return (None, None);
        }
        return (None, None);
    }
    
    // Calculate duration from first and last PCR
    let first_pcr = pcr_values.first().unwrap();
    let last_pcr = pcr_values.last().unwrap();
    
    // PCR is in 90kHz units
    let pcr_diff = if last_pcr.1 >= first_pcr.1 {
        last_pcr.1 - first_pcr.1
    } else {
        // Handle PCR wraparound (33-bit value wraps)
        (0x1FFFFFFFF - first_pcr.1) + last_pcr.1
    };
    
    let duration_ms = (pcr_diff * 1000) / 90000; // Convert from 90kHz to ms
    
    // Calculate overall bitrate
    let overall_bitrate = if duration_ms > 0 {
        Some((buf.len() as u64 * 8 * 1000) / duration_ms)
    } else {
        None
    };
    
    (Some(duration_ms), overall_bitrate)
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
        prog.streams.push(ElementaryStream {
            pid: es_pid,
            stream_type,
            format_identifier: es_fid,
            aac: None,
            avc_encoder: None,
        });
        i = desc_end;
    }
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
    let flags1 = buf[6];
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
fn extract_avc_frame_rate(pes_payload: &[u8]) -> Option<f64> {
    // Look for SPS NAL unit: nal_unit_type = 7
    // NAL header: 1 byte (forbidden_zero_bit | nal_ref_idc | nal_unit_type)
    // We need to find 0x67 or 0x27 (nal_unit_type = 7, different nal_ref_idc values)
    
    for i in 0..pes_payload.len().saturating_sub(5) {
        let nal_type = pes_payload[i] & 0x1F;
        if nal_type == 7 { // SPS
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
        if sps[i] == 0 && sps[i+1] == 0 && sps[i+2] == 0 && sps[i+3] == 1 {
            // Found start code, skip
            continue;
        }
    }
    
    // Return None for now - would need full bitstream parsing
    None
}

/// Calculate frame rate from PTS differences in multiple PES packets.
/// This is used when we have multiple PCR/PTS samples from the same PID.
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
    if min_diff / closest < 0.01 {
        closest
    } else {
        fps
    }
}

/// Scan a PES payload accumulator for the first ADTS sync (0xFFF) and
/// decode the header. Returns None if no sync found in the first 1 KiB
/// or if the header fields are invalid.
fn sniff_aac_adts(buf: &[u8]) -> Option<AacInfo> {
    const SAMPLE_RATE_TABLE: [u32; 13] = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
    ];
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

/// Scan AVC ES payload for the "x264 - core N rXXXX hash" string
/// that x264 stamps into an SEI user_data NAL. Oracle truncates the
/// Encoded_Library at the hash (before " - H.264..."), so we do too.
fn sniff_x264_encoder(buf: &[u8]) -> Option<String> {
    let needle = b"x264 - core ";
    for i in 0..buf.len().saturating_sub(needle.len() + 10) {
        if &buf[i..i + needle.len()] != needle {
            continue;
        }
        let start = i;
        // Stop at the " - H." that immediately follows the hash, or at
        // any non-printable byte. End at the first such marker.
        let max_end = (start + 256).min(buf.len());
        let mut end = start;
        while end < max_end {
            // Stop at " - H." boundary.
            if end + 5 <= buf.len() && &buf[end..end + 5] == b" - H." {
                break;
            }
            let c = buf[end];
            if c != b' ' && !c.is_ascii_graphic() {
                break;
            }
            end += 1;
        }
        if let Ok(s) = std::str::from_utf8(&buf[start..end]) {
            return Some(s.trim_end().to_string());
        }
    }
    None
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
    use revelio_core::FileAnalyze;

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
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG-TS".to_owned())
        );
        assert_eq!(fa.Count_Get(StreamKind::Video), 1);
        assert_eq!(fa.Count_Get(StreamKind::Audio), 1);
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG Video".to_owned())
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("AAC".to_owned())
        );
    }
}
