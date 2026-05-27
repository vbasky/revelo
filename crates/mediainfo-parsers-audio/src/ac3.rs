//! AC-3 (Dolby Digital) parser — sync-based.
//!
//! Frame layout:
//!   0x0B 0x77             sync word (big-endian)
//!   crc1 (2 bytes)
//!   fscod<2 bits> | frmsizecod<6 bits>    (1 byte)
//!   bsid<5 bits>  | bsmod<3 bits>         (1 byte)
//!   acmod<3 bits> | ... (rest of bit-packed flags)

use mediainfo_core::{FileAnalyze, StreamKind};

const AC3_SYNC: [u8; 2] = [0x0B, 0x77];

const FSCOD_TO_SAMPLE_RATE: [u32; 4] = [48000, 44100, 32000, 0];

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
    let frame_bytes = fa.peek_raw(8);
    let Some(fb) = frame_bytes else { return false };

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

    let channels = match acmod {
        0 => 2,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 3,
        5 => 4,
        6 => 4,
        7 => 5,
        _ => 2,
    } + if lfeon != 0 { 1 } else { 0 };
    let channel_layout = match acmod {
        1 => Some(("Front: C", "C")),
        2 => Some(("Front: L R", "L R")),
        _ => None,
    };

    fill_streams(
        fa,
        file_size,
        sample_rate,
        bitrate_bps,
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

fn fill_streams(
    fa: &mut FileAnalyze,
    file_size: usize,
    sample_rate: u32,
    bitrate_bps: u32,
    channels: u16,
    channel_layout: Option<(&'static str, &'static str)>,
    bsid: u8,
    acmod: u8,
    dsurmod: u8,
    lfeon: u8,
    dialnorm: u8,
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "AC-3", false);
    fa.Fill(StreamKind::General, 0, "Format_Commercial_IfAny", "Dolby Digital", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
    fa.Fill(StreamKind::General, 0, "StreamSize", "0", true);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "AC-3", false);
    fa.Fill(StreamKind::Audio, 0, "Format_Commercial_IfAny", "Dolby Digital", false);
    fa.Fill(StreamKind::Audio, 0, "Format_Settings_Endianness", "Big", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate", bitrate_bps.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    if let Some((p, l)) = channel_layout {
        fa.Fill(StreamKind::Audio, 0, "ChannelPositions", p, false);
        fa.Fill(StreamKind::Audio, 0, "ChannelLayout", l, false);
    }
    fa.Fill(StreamKind::Audio, 0, "SamplesPerFrame", "1536", false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);

    // Frame count: file_size / frame_bytes. For AC-3 at 48kHz: bytes/frame
    // = bitrate_bps * 1536 / (8 * 48000) = 4 * bitrate_kbps.
    if sample_rate > 0 && bitrate_bps > 0 {
        let frame_bytes = (bitrate_bps as u64) * 1536 / (8 * sample_rate as u64);
        if frame_bytes > 0 {
            let frame_count = (file_size as u64) / frame_bytes;
            fa.Fill(StreamKind::Audio, 0, "FrameCount", frame_count.to_string(), false);
            let sampling_count = frame_count * 1536;
            fa.Fill(StreamKind::Audio, 0, "SamplingCount", sampling_count.to_string(), false);
            let frame_rate = (sample_rate as f64) / 1536.0;
            fa.Fill(
                StreamKind::Audio,
                0,
                "FrameRate",
                format!("{:.3}", frame_rate),
                false,
            );
            let duration_ms = (sampling_count * 1000) / (sample_rate as u64);
            fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
        }
    }
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.Fill(StreamKind::Audio, 0, "StreamSize", file_size.to_string(), false);
    // ServiceKind defaults to "CM" (Complete Main) for typical AC-3 —
    // bsmod=0 means main service, complete description.
    fa.Fill(StreamKind::Audio, 0, "ServiceKind", "CM", false);
    // <extra> section: BSI fields straight from the frame header.
    // dialnorm stored as the encoded 5-bit value, displayed as a
    // negative dBFS level.
    fa.Fill(StreamKind::Audio, 0, "bsid", bsid.to_string(), false);
    let dialnorm_display = if dialnorm == 0 { -31i32 } else { -(dialnorm as i32) };
    fa.Fill(StreamKind::Audio, 0, "dialnorm", dialnorm_display.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "dsurmod", dsurmod.to_string(), false);
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
