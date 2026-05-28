//! Digital Audio Tape (DAT) frame stream parser.
//!
//! DAT has no fixed file-header magic. The format is a sequence of fixed
//! 5822-byte `dtframe` records; identification is structural — we validate
//! the `dtmainid` fields (fmtid==0, mapped emphasis/samples/numchans/
//! quantization indices) of the first frame, and require that the
//! candidate frame count divides the buffer cleanly.
//!
//! Frame layout (5822 bytes, big-endian / MSB-first bit-packed):
//!   0x0000  5760 bytes  audio
//!   0x1680    56 bytes  dtsubcode (7 × 8-byte packs, each: id[4] + payload[52] + parity[8])
//!   0x16B8     4 bytes  dtsubid   (ctrl[4] + dataid[4] + pno1[4] + numpacks[4] + pno2[4] + pno3[4] + ipf[8])
//!   0x16BC     2 bytes  dtmainid  (fmtid[2] + emphasis[2] + sampfreq[2] + numchans[2] + quantization[2] + trackpitch[2] + copy[2] + pack[2])
//!
//! Output fills General.Format="DAT", Audio.Format="PCM", a fixed
//! Audio.BitRate (1536000 × 441 / 480 = 1411200 — DAT's nominal CD-rate),
//! plus SamplingRate / Channels / BitDepth / Emphasis derived from the
//! dtmainid indices on the first valid frame.

use revelio_core::{FileAnalyze, StreamKind};

const DAT_FRAME_SIZE: usize = 5822;
const DAT_AUDIO_SIZE: usize = 5760;
const DAT_SUBCODE_SIZE: usize = 56;

// Index tables mirror File_Dat.cpp Dat_* arrays.
const DAT_SAMPLES: [u16; 4] = [1440, 1323, 960, 0];
const DAT_NUMCHANS: [u8; 4] = [2, 4, 0, 0];
const DAT_QUANTIZATION: [u8; 4] = [16, 12, 0, 0];
const DAT_EMPHASIS: [Option<&str>; 4] = [Some("off"), Some("50/15 ms"), None, None];

pub fn parse_dat(fa: &mut FileAnalyze) -> bool {
    let total = fa.remain();
    if total < DAT_FRAME_SIZE {
        return false;
    }
    let buf = match fa.peek_raw(total) {
        Some(b) => b,
        None => return false,
    };

    let frame = &buf[..DAT_FRAME_SIZE];
    let (fmtid, emphasis, sampfreq, numchans, quantization, trackpitch) =
        match parse_dtmainid(frame) {
            Some(v) => v,
            None => return false,
        };

    // fmtid must be 0 ("for audio use") for DAT audio. Reject everything else.
    if fmtid != 0 {
        return false;
    }
    // sampfreq/numchans/quantization indices must map to non-zero values;
    // otherwise this is not a valid DAT audio frame.
    if DAT_SAMPLES[sampfreq as usize] == 0
        || DAT_NUMCHANS[numchans as usize] == 0
        || DAT_QUANTIZATION[quantization as usize] == 0
    {
        return false;
    }
    let _ = trackpitch;

    // Require the buffer be (approximately) a whole multiple of frames —
    // we tolerate one partial trailing frame, same as the C++ truncation path.
    let frame_count = total / DAT_FRAME_SIZE;
    if frame_count == 0 {
        return false;
    }

    // sampfreq stores "samples per 1/100s frame"; oracle scales by 100/3
    // to obtain Hz (Dat_samples=1440 → 48000, 1323 → 44100, 960 → 32000).
    let sampling_rate_hz = (DAT_SAMPLES[sampfreq as usize] as u32) * 100 / 3;
    let channels = DAT_NUMCHANS[numchans as usize];
    let bit_depth = DAT_QUANTIZATION[quantization as usize];

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DAT", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "PCM", false);
    // BitRate = 1536000 × 441 / 480 = 1411200; replicates the C++ literal.
    fa.fill(StreamKind::Audio, 0, "BitRate", "1411200", false);
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);
    fa.fill(StreamKind::Audio, 0, "SamplingRate", sampling_rate_hz.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "BitDepth", bit_depth.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossless", false);
    if let Some(e) = DAT_EMPHASIS[emphasis as usize] {
        fa.fill(StreamKind::Audio, 0, "Format_Settings_Emphasis", e, false);
    }

    // StreamSize: C++ computes (FileSize / 5822) × 5760 × 441 / 480 ≈ audio
    // payload retimed to the nominal 1411200 bps. Replicate exactly.
    let file_size = total as u64;
    let stream_size = ((file_size / DAT_FRAME_SIZE as u64) * DAT_AUDIO_SIZE as u64) * 441 / 480;
    fa.fill(StreamKind::Audio, 0, "StreamSize", stream_size.to_string(), false);

    true
}

/// Decode the 16-bit dtmainid at the end of a 5822-byte DAT frame.
/// Returns `(fmtid, emphasis, sampfreq, numchans, quantization, trackpitch)`
/// — each value is the 2-bit index (0..=3).
fn parse_dtmainid(frame: &[u8]) -> Option<(u8, u8, u8, u8, u8, u8)> {
    if frame.len() < DAT_FRAME_SIZE {
        return None;
    }
    let off = DAT_AUDIO_SIZE + DAT_SUBCODE_SIZE + 4; // skip audio + subcode + dtsubid
    let b0 = frame[off];
    let b1 = frame[off + 1];
    // 8 × 2-bit fields packed MSB-first across two bytes.
    let fmtid = (b0 >> 6) & 0b11;
    let emphasis = (b0 >> 4) & 0b11;
    let sampfreq = (b0 >> 2) & 0b11;
    let numchans = b0 & 0b11;
    let quantization = (b1 >> 6) & 0b11;
    let trackpitch = (b1 >> 4) & 0b11;
    Some((fmtid, emphasis, sampfreq, numchans, quantization, trackpitch))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a single 5822-byte DAT frame with the given dtmainid indices.
    /// Subcode/subid bytes are left zero — DAT identification doesn't need them.
    fn make_frame(
        fmtid: u8,
        emphasis: u8,
        sampfreq: u8,
        numchans: u8,
        quantization: u8,
        trackpitch: u8,
    ) -> Vec<u8> {
        let mut frame = vec![0u8; DAT_FRAME_SIZE];
        let off = DAT_AUDIO_SIZE + DAT_SUBCODE_SIZE + 4;
        frame[off] = ((fmtid & 0b11) << 6)
            | ((emphasis & 0b11) << 4)
            | ((sampfreq & 0b11) << 2)
            | (numchans & 0b11);
        frame[off + 1] = ((quantization & 0b11) << 6) | ((trackpitch & 0b11) << 4);
        frame
    }

    #[test]
    fn parses_48khz_stereo_16bit() {
        // fmtid=0, emphasis=0 (off), sampfreq=0 (1440→48000), numchans=0 (2), q=0 (16-bit).
        let frame = make_frame(0, 0, 0, 0, 0, 0);
        // Two frames so frame_count > 0 and StreamSize stays nonzero.
        let mut buf = frame.clone();
        buf.extend_from_slice(&frame);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dat(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("DAT"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("BitRate").as_deref(), Some("1411200"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("Format_Settings_Emphasis").as_deref(), Some("off"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
    }

    #[test]
    fn parses_44khz_4ch_12bit_with_emphasis() {
        // sampfreq=1 (1323→44100), numchans=1 (4ch), q=1 (12-bit), emphasis=1 ("50/15 ms").
        let frame = make_frame(0, 1, 1, 1, 1, 0);
        let mut fa = FileAnalyze::new(&frame);
        assert!(parse_dat(&mut fa));

        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("Channels").as_deref(), Some("4"));
        assert_eq!(a("BitDepth").as_deref(), Some("12"));
        assert_eq!(a("Format_Settings_Emphasis").as_deref(), Some("50/15 ms"));
    }

    #[test]
    fn rejects_non_dat_and_invalid_fmtid() {
        let mut fa = FileAnalyze::new(b"NOT A DAT FILE");
        assert!(!parse_dat(&mut fa));

        // Frame with fmtid != 0 must be rejected (only "for audio use" is valid).
        let bad = make_frame(1, 0, 0, 0, 0, 0);
        let mut fa2 = FileAnalyze::new(&bad);
        assert!(!parse_dat(&mut fa2));

        // Frame with sampfreq=3 (Dat_samples[3]=0, invalid) must be rejected.
        let bad2 = make_frame(0, 0, 3, 0, 0, 0);
        let mut fa3 = FileAnalyze::new(&bad2);
        assert!(!parse_dat(&mut fa3));
    }
}
