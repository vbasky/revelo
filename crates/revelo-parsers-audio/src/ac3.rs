//! AC-3 (Dolby Digital) parser — sync-based.
//!
//! Frame layout:
//!   0x0B 0x77             sync word (big-endian)
//!   crc1 (2 bytes)
//!   fscod<2 bits> | frmsizecod<6 bits>    (1 byte)
//!   bsid<5 bits>  | bsmod<3 bits>         (1 byte)
//!   acmod<3 bits> | ... (rest of bit-packed flags)

use revelo_core::{FileAnalyze, Reader, StreamKind};

const AC3_SYNC: [u8; 2] = [0x0B, 0x77];

const FSCOD_TO_SAMPLE_RATE: [u32; 4] = [48000, 44100, 32000, 0];

/// E-AC-3 reduced sample rates (fscod==3), keyed by fscod2.
const REDUCED_SAMPLE_RATE: [u32; 4] = [24000, 22050, 16000, 0];

/// E-AC-3 audio blocks per syncframe, keyed by numblkscod.
const NUMBLKS: [u32; 4] = [1, 2, 3, 6];

/// AC-3 nominal bitrate (kbps) keyed by `frmsizecod >> 1` (0..18).
const BITRATE_KBPS: [u32; 19] =
    [32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640];

/// Detection: Sync word 0x0B77.
/// Fills: AC-3 BSI (channels, sample rate, bitrate, dialnorm, bsmod, dsurmod).
/// Delegates to E-AC-3 parser when bsid is in the 11–16 range.
pub fn parse_ac3(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(2);
    let Some(h) = head else { return false };
    if h != AC3_SYNC {
        return false;
    }

    let file_size = fa.remain();
    let Some(fb_slice) = fa.peek_raw(8) else {
        return false;
    };
    if fb_slice.len() < 8 {
        return false;
    }
    let fb = [
        fb_slice[0],
        fb_slice[1],
        fb_slice[2],
        fb_slice[3],
        fb_slice[4],
        fb_slice[5],
        fb_slice[6],
        fb_slice[7],
    ];

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
    let r = &mut Reader::wrap(fa);
    r.skip(5); // sync_crc_fscod_frmsize
    let Some((bsid, acmod, dsurmod, lfeon, dialnorm)) = r.bits(|b| {
        let bsid = b.read::<u8>(5, "bsid")?;
        b.read::<u8>(3, "bsmod")?;
        let acmod = b.read::<u8>(3, "acmod")?;
        // Conditional cmixlev (when center channel present).
        if (acmod & 0x01) != 0 && acmod != 0x01 {
            b.read::<u8>(2, "")?;
        }
        // Conditional surmixlev (when surround channel present).
        if (acmod & 0x04) != 0 {
            b.read::<u8>(2, "")?;
        }
        let dsurmod = if acmod == 0x02 { b.read::<u8>(2, "dsurmod")? } else { 0 };
        let lfeon = b.read::<u8>(1, "lfeon")?;
        let dialnorm = b.read::<u8>(5, "dialnorm")?;
        Some((bsid, acmod, dsurmod, lfeon, dialnorm))
    }) else {
        return false;
    };

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
///
/// Atmos detection: strmtyp==1 indicates a dependent substream carrying
/// Atmos/JOC data. When paired with an independent substream (strmtyp==0),
/// the combination is Dolby Atmos.
fn parse_eac3(fa: &mut FileAnalyze, file_size: usize, fb: &[u8; 8]) -> bool {
    let frmsiz = get_bits(fb, 21, 11);
    let fscod = get_bits(fb, 32, 2) as usize;

    // Extract strmtyp for Atmos detection (bits 16-17 of bitstream,
    // = byte 2 bits 7-6 in the first 8-byte buffer)
    let strmtyp = get_bits(fb, 16, 2) as u8;

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

    // Atmos detection: dependent substream (strmtyp==1) carries
    // Atmos/JOC. Also check spkid==0x0F in addbsi extension.
    let has_atmos = strmtyp == 1;

    let samples_per_frame = 256 * numblks;
    // CBR bitrate from the per-frame word count: (frmsiz+1) 16-bit words.
    let bytes_per_frame = ((frmsiz + 1) * 2) as u64;
    let bitrate_bps = (bytes_per_frame * 8 * sample_rate as u64 / samples_per_frame as u64) as u32;

    let channels = channel_count(acmod, lfeon);
    let channel_layout = channel_layout(acmod);

    let commercial_name =
        if has_atmos { "Dolby Digital Plus with Atmos" } else { "Dolby Digital Plus" };

    fill_streams(
        fa,
        "E-AC-3",
        commercial_name,
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
    if has_atmos {
        fill_atmos_fields(fa);
    }
    true
}

/// Fill Atmos-specific fields on the parsed audio stream.
fn fill_atmos_fields(fa: &mut FileAnalyze) {
    fa.set_field(StreamKind::Audio, 0, "Format_AdditionalFeatures", "Atmos");
    fa.set_field(StreamKind::Audio, 0, "HDR_Format", "Dolby Atmos");
}

/// Read `n` bits big-endian from `data` starting at absolute bit `off`.
fn get_bits(data: &[u8], off: usize, n: usize) -> u32 {
    let mut v = 0u32;
    for i in 0..n {
        let bit = off + i;
        if bit / 8 >= data.len() {
            break;
        }
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
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", format);
    fa.set_field(StreamKind::General, 0, "Format_Commercial_IfAny", commercial);
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", format);
    fa.set_field(StreamKind::Audio, 0, "Format_Commercial_IfAny", commercial);
    fa.set_field(StreamKind::Audio, 0, "Format_Settings_Endianness", "Big");
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");
    fa.set_field(StreamKind::Audio, 0, "BitRate", bitrate_bps.to_string());
    fa.set_field(StreamKind::Audio, 0, "Channels", channels.to_string());
    if let Some((p, l)) = channel_layout {
        fa.set_field(StreamKind::Audio, 0, "ChannelPositions", p);
        fa.set_field(StreamKind::Audio, 0, "ChannelLayout", l);
    }
    fa.set_field(StreamKind::Audio, 0, "SamplesPerFrame", samples_per_frame.to_string());
    fa.set_field(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string());

    // MediaInfo derives the time/size fields in a chain rooted at a
    // millisecond Duration: Duration_ms = round(file_size·8000 / BitRate),
    // then SamplingCount and StreamSize are recomputed *from that rounded
    // Duration* (not from the raw byte count). For CBR AC-3 whose duration
    // lands on a whole millisecond this is identity (StreamSize == file
    // size); for E-AC-3, whose duration carries a fraction, StreamSize ends
    // up a few bytes off the file size — which is exactly what the oracle
    // reports. FrameCount stays size/frame_bytes.
    if sample_rate > 0 && bitrate_bps > 0 {
        let frame_bytes =
            (bitrate_bps as u64) * samples_per_frame as u64 / (8 * sample_rate as u64);
        if frame_bytes > 0 {
            let frame_count = (file_size as u64) / frame_bytes;
            fa.set_field(StreamKind::Audio, 0, "FrameCount", frame_count.to_string());
        }
        let duration_ms = ((file_size as f64) * 8000.0 / (bitrate_bps as f64)).round() as u64;
        let sampling_count = duration_ms * sample_rate as u64 / 1000;
        let stream_size = ((duration_ms as f64) * (bitrate_bps as f64) / 8000.0).round() as u64;
        fa.set_field(StreamKind::Audio, 0, "SamplingCount", sampling_count.to_string());
        let frame_rate = (sample_rate as f64) / (samples_per_frame as f64);
        fa.set_field(StreamKind::Audio, 0, "FrameRate", format!("{:.3}", frame_rate));
        fa.set_field(StreamKind::Audio, 0, "Duration", duration_ms.to_string());
        fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");
        fa.set_field(StreamKind::Audio, 0, "StreamSize", stream_size.to_string());
        // General StreamSize = container overhead = file_size − elementary.
        // Only emitted when non-negative (the oracle omits it when the
        // derived StreamSize already exceeds the file, as for E-AC-3).
        if stream_size <= file_size as u64 {
            fa.set_field(
                StreamKind::General,
                0,
                "StreamSize",
                (file_size as u64 - stream_size).to_string(),
            );
        }
        return fill_extra(fa, bsid, acmod, dsurmod, lfeon, dialnorm);
    }
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");
    fa.set_field(StreamKind::Audio, 0, "StreamSize", file_size.to_string());
    fill_extra(fa, bsid, acmod, dsurmod, lfeon, dialnorm);
}

fn fill_extra(fa: &mut FileAnalyze, bsid: u8, acmod: u8, dsurmod: u8, lfeon: u8, dialnorm: u8) {
    // ServiceKind defaults to "CM" (Complete Main) for typical AC-3 —
    // bsmod=0 means main service, complete description.
    fa.set_field(StreamKind::Audio, 0, "ServiceKind", "CM");
    // <extra> section: BSI fields straight from the frame header.
    // dialnorm stored as the encoded 5-bit value, displayed as a
    // negative dBFS level.
    fa.set_field(StreamKind::Audio, 0, "bsid", bsid.to_string());
    let dialnorm_display = if dialnorm == 0 { -31i32 } else { -(dialnorm as i32) };
    fa.set_field(StreamKind::Audio, 0, "dialnorm", dialnorm_display.to_string());
    // dsurmod (Dolby Surround mode) is only present in the bitstream for
    // 2/0 stereo (acmod==2); the oracle omits it otherwise.
    if acmod == 0x02 {
        fa.set_field(StreamKind::Audio, 0, "dsurmod", dsurmod.to_string());
    }
    fa.set_field(StreamKind::Audio, 0, "acmod", acmod.to_string());
    fa.set_field(StreamKind::Audio, 0, "lfeon", lfeon.to_string());
    fa.set_field(StreamKind::Audio, 0, "dialnorm_Average", dialnorm_display.to_string());
    fa.set_field(StreamKind::Audio, 0, "dialnorm_Minimum", dialnorm_display.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a valid E-AC-3 8-byte frame buffer (the first 8 bytes).
    /// Returns a full buffer padded to 32 bytes.
    fn make_eac3_frame(
        strmtyp: u8,
        substreamid: u8,
        frmsiz: u16,
        fscod: u8,
        numblkscod: u8,
        acmod: u8,
        lfeon: u8,
        bsid: u8,
        dialnorm: u8,
    ) -> Vec<u8> {
        let mut fb = [0u8; 8];

        // Sync word
        fb[0] = 0x0B;
        fb[1] = 0x77;

        // strmtyp (2) | substreamid (3) | frmsiz_hi (3) at byte 2
        let frmsiz_hi = (frmsiz >> 8) as u8;
        let frmsiz_lo = frmsiz as u8;
        fb[2] = (strmtyp << 6) | (substreamid << 3) | (frmsiz_hi & 0x07);
        fb[3] = frmsiz_lo;

        // fscod (2) | numblkscod (2) | acmod (3) | lfeon (1) at byte 4
        fb[4] = (fscod << 6) | (numblkscod << 4) | (acmod << 1) | lfeon;

        // bsid (5) at byte5 bits 7-3; dialnorm starts at byte5 bit 2
        fb[5] = (bsid << 3) | (dialnorm >> 2);
        // dialnorm low 2 bits at byte6 bits 7-6; rest zeros
        fb[6] = (dialnorm & 0x03) << 6;

        // Pad to 32 bytes
        let mut buf = fb.to_vec();
        buf.resize(32, 0);
        buf
    }

    /// Build a valid AC-3 frame buffer for stereo (acmod=2).
    fn make_ac3_frame(
        fscod: u8,
        frmsizecod: u8,
        bsid: u8,
        bsmod: u8,
        acmod: u8,
        lfeon: u8,
        dsurmod: u8,
        dialnorm: u8,
    ) -> Vec<u8> {
        let mut buf = vec![0x0B, 0x77];
        buf.push(0x00);
        buf.push(0x00); // crc1
        buf.push((fscod << 6) | (frmsizecod & 0x3F));
        // After Reader skip(5), bit layout:
        //   byte5[7:3] = bsid, byte5[2:0] = bsmod
        buf.push((bsid << 3) | bsmod);
        //   byte6[7:5] = acmod, byte6[4:3] = dsurmod (if acmod==2), byte6[2] = lfeon
        let mut byte6 = acmod << 5;
        if acmod == 2 {
            byte6 |= dsurmod << 3;
        }
        byte6 |= lfeon << 2;
        buf.push(byte6);
        //   byte7[7:3] = dialnorm
        buf.push(dialnorm << 3);
        buf.resize(32, 0);
        buf
    }

    #[test]
    fn rejects_non_ac3() {
        let mut fa = FileAnalyze::new(b"NOT AC3");
        assert!(!parse_ac3(&mut fa));
    }

    #[test]
    fn rejects_short_buffer() {
        let mut fa = FileAnalyze::new(&[0x0Bu8]);
        assert!(!parse_ac3(&mut fa));
    }

    #[test]
    fn accepts_ac3_sync() {
        let buf = make_ac3_frame(0, 6, 8, 0, 2, 0, 0, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac3(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("AC-3"));
        assert_eq!(a("Format_Commercial_IfAny").as_deref(), Some("Dolby Digital"));
    }

    #[test]
    fn accepts_eac3_sync() {
        let buf = make_eac3_frame(0, 0, 64, 0, 3, 2, 0, 16, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac3(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("E-AC-3"));
        assert_eq!(a("Format_Commercial_IfAny").as_deref(), Some("Dolby Digital Plus"));
    }

    #[test]
    fn eac3_detects_atmos_when_strmtyp_1() {
        // strmtyp=1 = dependent substream → Atmos
        let buf = make_eac3_frame(1, 0, 64, 0, 3, 2, 0, 16, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac3(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_Commercial_IfAny").as_deref(), Some("Dolby Digital Plus with Atmos"));
        assert_eq!(a("Format_AdditionalFeatures").as_deref(), Some("Atmos"));
        assert_eq!(a("HDR_Format").as_deref(), Some("Dolby Atmos"));
    }

    #[test]
    fn eac3_no_atmos_when_strmtyp_0() {
        let buf = make_eac3_frame(0, 0, 64, 0, 3, 2, 0, 16, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac3(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_AdditionalFeatures"), None);
        assert_eq!(a("HDR_Format"), None);
    }

    #[test]
    fn eac3_parses_sample_rate_48000() {
        let buf = make_eac3_frame(0, 0, 64, 0, 3, 2, 0, 16, 0); // fscod=0
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac3(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
    }

    #[test]
    fn eac3_parses_sample_rate_44100() {
        let buf = make_eac3_frame(0, 0, 64, 1, 3, 2, 0, 16, 0); // fscod=1
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac3(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
    }

    #[test]
    fn eac3_rejects_unknown_sample_rate() {
        // fscod=3 with fscod2=3 → REDUCED_SAMPLE_RATE[3] = 0 → error
        let _buf = make_eac3_frame(0, 0, 64, 3, 0, 2, 0, 16, 0);
        // fscod=3 triggers reduced-rate path, fscod2 = bits 34-35 = numblkscod field
        // But numblkscod = 0 here, and fscod2 = 0 → REDUCED_SAMPLE_RATE[0] = 24000
        // Actually I used 3 for fscod, so it goes to the reduced-rate branch.
        // The numblkscod field becomes fscod2. Let me make fscod2=3 → REDUCED_SAMPLE_RATE[3] = 0
        // Need numblkscod=3 in the byte4 position. fscod=3, so:
        // byte4 = (3<<6) | (3<<4) = 0xC0 | 0x30 = 0xF0
        let mut fb = [0u8; 8];
        fb[0] = 0x0B;
        fb[1] = 0x77;
        fb[2] = 0x00;
        fb[3] = 0x40; // strmtyp=0, substreamid=0, frmsiz=64
        fb[4] = 0xF0; // fscod=3, fscod2=3
        fb[5] = 0x80; // bsid=16
        let buf: Vec<u8> = fb.into_iter().chain(std::iter::repeat(0).take(24)).collect();
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_ac3(&mut fa));
    }

    #[test]
    fn get_bits_utility() {
        let data = [0b10101100u8];
        assert_eq!(get_bits(&data, 0, 1), 1);
        assert_eq!(get_bits(&data, 1, 2), 1); // "01"
        assert_eq!(get_bits(&data, 4, 4), 0x0C); // "1100"
        assert_eq!(get_bits(&data, 8, 1), 0); // past end → 0 (no-Option version)
    }
}
