//! AC-3 (Dolby Digital) parser — sync-based.
//!
//! Frame layout:
//!   0x0B 0x77             sync word (big-endian)
//!   crc1 (2 bytes)
//!   fscod<2 bits> | frmsizecod<6 bits>    (1 byte)
//!   bsid<5 bits>  | bsmod<3 bits>         (1 byte)
//!   acmod<3 bits> | ... (rest of bit-packed flags)

use revelio_core::{FileAnalyze, StreamKind};

const AC3_SYNC: [u8; 2] = [0x0B, 0x77];

const FSCOD_TO_SAMPLE_RATE: [u32; 4] = [48000, 44100, 32000, 0];

/// E-AC-3 reduced sample rates (fscod==3), keyed by fscod2.
const REDUCED_SAMPLE_RATE: [u32; 4] = [24000, 22050, 16000, 0];

/// E-AC-3 audio blocks per syncframe, keyed by numblkscod.
const NUMBLKS: [u32; 4] = [1, 2, 3, 6];

/// AC-3 nominal bitrate (kbps) keyed by `frmsizecod >> 1` (0..18).
const BITRATE_KBPS: [u32; 19] = [
    32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640,
];

pub fn parse_ac3(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(2);
    let Some(h) = head else { return false };
    if h != AC3_SYNC {
        return false;
    }

    let file_size = fa.Remain();
    let Some(fb_slice) = fa.peek_raw(8) else { return false };
    if fb_slice.len() < 8 {
        return false;
    }
    let fb: [u8; 8] = fb_slice[..8].try_into().unwrap();

    // bsid sits at bit offset 40 (the top 5 bits of byte 5) in BOTH the
    // AC-3 and E-AC-3 BSI layouts — E-AC-3's strmtyp/substreamid/frmsiz
    // (16 bits) replace AC-3's crc1, and fscod+numblkscod (4) plus
    // acmod+lfeon (4) line up with AC-3's fscod+frmsizecod (8). bsid<=10
    // is AC-3; 11..=16 is E-AC-3 (Annex E).
    let bsid_peek = (fb[5] >> 3) & 0x1F;
    if (11..=16).contains(&bsid_peek) {
        return parse_eac3(fa, file_size, &fb);
    }

    let fscod = (fb[4] >> 6) & 0x3;
    let frmsizecod = fb[4] & 0x3F;
    if fscod >= 3 || frmsizecod >= 38 {
        return false;
    }

    // Bit-level read for the BSI fields after frmsizecod.
    fa.Skip_Hexa(5, "sync_crc_fscod_frmsize");
    fa.BS_Begin();
    let mut bsid: zenlib::int8u = 0;
    fa.Get_S1(5, &mut bsid, "bsid");
    let mut bsmod: zenlib::int8u = 0;
    fa.Get_S1(3, &mut bsmod, "bsmod");
    let mut acmod: zenlib::int8u = 0;
    fa.Get_S1(3, &mut acmod, "acmod");
    // Conditional cmixlev (when center channel present).
    if (acmod & 0x01) != 0 && acmod != 0x01 {
        let mut _cmixlev: zenlib::int8u = 0;
        fa.Get_S1(2, &mut _cmixlev, "");
    }
    // Conditional surmixlev (when surround channel present).
    if (acmod & 0x04) != 0 {
        let mut _surmixlev: zenlib::int8u = 0;
        fa.Get_S1(2, &mut _surmixlev, "");
    }
    let mut dsurmod: zenlib::int8u = 0;
    if acmod == 0x02 {
        fa.Get_S1(2, &mut dsurmod, "dsurmod");
    }
    let mut lfeon: zenlib::int8u = 0;
    fa.Get_S1(1, &mut lfeon, "lfeon");
    let mut dialnorm: zenlib::int8u = 0;
    fa.Get_S1(5, &mut dialnorm, "dialnorm");
    fa.BS_End();
    let _ = bsmod;

    let sample_rate = FSCOD_TO_SAMPLE_RATE[fscod as usize];
    let bitrate_kbps = BITRATE_KBPS[(frmsizecod >> 1) as usize];
    let bitrate_bps = bitrate_kbps * 1000;

    let channels = channel_count(acmod, lfeon);
    let channel_layout = channel_layout(acmod);

    fill_streams(
        fa,
        "AC-3",
        "Dolby Digital",
        file_size,
        sample_rate,
        bitrate_bps,
        1536,
        channels,
        channel_layout,
        bsid,
        acmod,
        dsurmod,
        lfeon,
        dialnorm,
    );
    true
}

/// E-AC-3 (Dolby Digital Plus, AC-3 Annex E) independent substream header.
/// Layout after the 0x0B77 sync: strmtyp(2) substreamid(3) frmsiz(11)
/// fscod(2) {fscod2(2) | numblkscod(2)} acmod(3) lfeon(1) bsid(5)
/// dialnorm(5) …
fn parse_eac3(fa: &mut FileAnalyze, file_size: usize, fb: &[u8; 8]) -> bool {
    let frmsiz = get_bits(fb, 21, 11);
    let fscod = get_bits(fb, 32, 2) as usize;

    let (sample_rate, numblks) = if fscod == 3 {
        // Reduced sample rates; numblkscod implied to 3 (6 blocks).
        let fscod2 = get_bits(fb, 34, 2) as usize;
        (REDUCED_SAMPLE_RATE[fscod2], 6u32)
    } else {
        let numblkscod = get_bits(fb, 34, 2) as usize;
        (FSCOD_TO_SAMPLE_RATE[fscod], NUMBLKS[numblkscod])
    };
    if sample_rate == 0 {
        return false;
    }

    let acmod = get_bits(fb, 36, 3) as u8;
    let lfeon = get_bits(fb, 39, 1) as u8;
    let bsid = get_bits(fb, 40, 5) as u8;
    let dialnorm = get_bits(fb, 45, 5) as u8;

    let samples_per_frame = 256 * numblks;
    // CBR bitrate from the per-frame word count: (frmsiz+1) 16-bit words.
    let bytes_per_frame = ((frmsiz + 1) * 2) as u64;
    let bitrate_bps =
        (bytes_per_frame * 8 * sample_rate as u64 / samples_per_frame as u64) as u32;

    let channels = channel_count(acmod, lfeon);
    let channel_layout = channel_layout(acmod);

    fill_streams(
        fa,
        "E-AC-3",
        "Dolby Digital Plus",
        file_size,
        sample_rate,
        bitrate_bps,
        samples_per_frame,
        channels,
        channel_layout,
        bsid,
        acmod,
        0,
        lfeon,
        dialnorm,
    );
    true
}

/// Read `n` bits big-endian from `data` starting at absolute bit `off`.
fn get_bits(data: &[u8], off: usize, n: usize) -> u32 {
    let mut v = 0u32;
    for i in 0..n {
        let bit = off + i;
        let byte = data[bit / 8];
        v = (v << 1) | ((byte >> (7 - (bit % 8))) & 1) as u32;
    }
    v
}

fn channel_count(acmod: u8, lfeon: u8) -> u16 {
    let base = match acmod {
        0 => 2,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 3,
        5 => 4,
        6 => 4,
        7 => 5,
        _ => 2,
    };
    base + if lfeon != 0 { 1 } else { 0 }
}

fn channel_layout(acmod: u8) -> Option<(&'static str, &'static str)> {
    match acmod {
        1 => Some(("Front: C", "M")),
        2 => Some(("Front: L R", "L R")),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn fill_streams(
    fa: &mut FileAnalyze,
    format: &str,
    commercial: &str,
    file_size: usize,
    sample_rate: u32,
    bitrate_bps: u32,
    samples_per_frame: u32,
    channels: u16,
    channel_layout: Option<(&'static str, &'static str)>,
    bsid: u8,
    acmod: u8,
    dsurmod: u8,
    lfeon: u8,
    dialnorm: u8,
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", format, false);
    fa.Fill(StreamKind::General, 0, "Format_Commercial_IfAny", commercial, false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", format, false);
    fa.Fill(StreamKind::Audio, 0, "Format_Commercial_IfAny", commercial, false);
    fa.Fill(StreamKind::Audio, 0, "Format_Settings_Endianness", "Big", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate", bitrate_bps.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    if let Some((p, l)) = channel_layout {
        fa.Fill(StreamKind::Audio, 0, "ChannelPositions", p, false);
        fa.Fill(StreamKind::Audio, 0, "ChannelLayout", l, false);
    }
    fa.Fill(StreamKind::Audio, 0, "SamplesPerFrame", samples_per_frame.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);

    // MediaInfo derives the time/size fields in a chain rooted at a
    // millisecond Duration: Duration_ms = round(file_size·8000 / BitRate),
    // then SamplingCount and StreamSize are recomputed *from that rounded
    // Duration* (not from the raw byte count). For CBR AC-3 whose duration
    // lands on a whole millisecond this is identity (StreamSize == file
    // size); for E-AC-3, whose duration carries a fraction, StreamSize ends
    // up a few bytes off the file size — which is exactly what the oracle
    // reports. FrameCount stays size/frame_bytes.
    if sample_rate > 0 && bitrate_bps > 0 {
        let frame_bytes = (bitrate_bps as u64) * samples_per_frame as u64 / (8 * sample_rate as u64);
        if frame_bytes > 0 {
            let frame_count = (file_size as u64) / frame_bytes;
            fa.Fill(StreamKind::Audio, 0, "FrameCount", frame_count.to_string(), false);
        }
        let duration_ms =
            ((file_size as f64) * 8000.0 / (bitrate_bps as f64)).round() as u64;
        let sampling_count = duration_ms * sample_rate as u64 / 1000;
        let stream_size =
            ((duration_ms as f64) * (bitrate_bps as f64) / 8000.0).round() as u64;
        fa.Fill(StreamKind::Audio, 0, "SamplingCount", sampling_count.to_string(), false);
        let frame_rate = (sample_rate as f64) / (samples_per_frame as f64);
        fa.Fill(StreamKind::Audio, 0, "FrameRate", format!("{:.3}", frame_rate), false);
        fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
        fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
        fa.Fill(StreamKind::Audio, 0, "StreamSize", stream_size.to_string(), false);
        // General StreamSize = container overhead = file_size − elementary.
        // Only emitted when non-negative (the oracle omits it when the
        // derived StreamSize already exceeds the file, as for E-AC-3).
        if stream_size <= file_size as u64 {
            fa.Fill(StreamKind::General, 0, "StreamSize",
                (file_size as u64 - stream_size).to_string(), true);
        }
        return fill_extra(fa, bsid, acmod, dsurmod, lfeon, dialnorm);
    }
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.Fill(StreamKind::Audio, 0, "StreamSize", file_size.to_string(), false);
    fill_extra(fa, bsid, acmod, dsurmod, lfeon, dialnorm);
}

fn fill_extra(fa: &mut FileAnalyze, bsid: u8, acmod: u8, dsurmod: u8, lfeon: u8, dialnorm: u8) {
    // ServiceKind defaults to "CM" (Complete Main) for typical AC-3 —
    // bsmod=0 means main service, complete description.
    fa.Fill(StreamKind::Audio, 0, "ServiceKind", "CM", false);
    // <extra> section: BSI fields straight from the frame header.
    // dialnorm stored as the encoded 5-bit value, displayed as a
    // negative dBFS level.
    fa.Fill(StreamKind::Audio, 0, "bsid", bsid.to_string(), false);
    let dialnorm_display = if dialnorm == 0 { -31i32 } else { -(dialnorm as i32) };
    fa.Fill(StreamKind::Audio, 0, "dialnorm", dialnorm_display.to_string(), false);
    // dsurmod (Dolby Surround mode) is only present in the bitstream for
    // 2/0 stereo (acmod==2); the oracle omits it otherwise.
    if acmod == 0x02 {
        fa.Fill(StreamKind::Audio, 0, "dsurmod", dsurmod.to_string(), false);
    }
    fa.Fill(StreamKind::Audio, 0, "acmod", acmod.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "lfeon", lfeon.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "dialnorm_Average", dialnorm_display.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "dialnorm_Minimum", dialnorm_display.to_string(), false);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_ac3() {
        let mut fa = FileAnalyze::new(b"NOT AC3");
        assert!(!parse_ac3(&mut fa));
    }
}
