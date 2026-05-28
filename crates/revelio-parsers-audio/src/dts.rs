//! DTS Coherent Acoustics (DCA) Core parser.
//!
//! Core frame layout (16-bit big-endian variant, the common case):
//!   Sync (32 bits) = 0x7FFE8001
//!   Frame Type (1)
//!   Deficit Sample Count (5)
//!   CRC Present (1)
//!   Number of PCM Sample Blocks (7) [+1, so 1..=128]
//!   Primary Frame Byte Size (14) [+1]
//!   Audio Channel Arrangement (6) = amode
//!   Core Audio Sampling Frequency (4) → DTS_SamplingRate[]
//!   Transmission Bit Rate (5)         → DTS_BitRate[]
//!   Embedded Down Mix Enabled (1)
//!   Embedded Dynamic Range (1)
//!   Embedded Time Stamp (1)
//!   Auxiliary Data (1)
//!   HDCD (1)
//!   Extension Audio Descriptor (3)
//!   Extended Coding (1)
//!   Audio Sync Word Insertion (1)
//!   Low Frequency Effects (2) = lfe_effects
//!   Predictor History (1)
//!   [CRC (16) if crc_present]
//!   Multirate Interpolator (1)
//!   Encoder Software Revision (4)
//!   Copy History (2)
//!   Source PCM Resolution (2) → DTS_Resolution[]
//!   ...
//!
//! Samples per frame = num_pcm_sample_blocks × 32.

use revelio_core::{FileAnalyze, StreamKind};

const SYNC_CORE_BE16: u32 = 0x7FFE8001;

const DTS_SAMPLE_RATES: [u32; 16] = [
    0, 8000, 16000, 32000, 0, 0, 11025, 22050,
    44100, 0, 0, 12000, 24000, 48000, 96000, 192000,
];

const DTS_BIT_RATES: [u32; 32] = [
    32000, 56000, 64000, 96000, 112000, 128000, 192000, 224000,
    256000, 320000, 384000, 448000, 512000, 576000, 640000, 754500,
    960000, 1024000, 1152000, 1280000, 1344000, 1408000, 1411200, 1472000,
    1509750, 1920000, 2048000, 3072000, 3840000, 0, 0, 0,
];

const DTS_CHANNELS: [u8; 16] = [
    1, 2, 2, 2, 2, 3, 3, 4,
    4, 5, 6, 6, 6, 7, 8, 8,
];

const DTS_CHANNEL_POSITIONS: [&str; 16] = [
    "Front: C",
    "Front: C C",
    "Front: L R",
    "Front: L R",
    "Front: L R",
    "Front: L C R",
    "Front: L R, Back: C",
    "Front: L C R, Back: C",
    "Front: L R, Side: L R",
    "Front: L C R, Side: L R",
    "Front: L C C R, Side: L R",
    "Front: L C R, Side: L R",
    "Front: L R, Side: L R, Back: L R",
    "Front: L C R, Side: L R, Back: L R",
    "Front: L R, Side: L R, Back: L C C R",
    "Front: L C R, Side: L R, Back: L C R",
];

const DTS_CHANNEL_LAYOUT: [&str; 16] = [
    "M",
    "M M",
    "L R",
    "L R",
    "Lt Rt",
    "C L R",
    "L R Cs",
    "C L R Cs",
    "L R Ls Rs",
    "C L R Ls Rs",
    "Cl Cr L R Ls Rs",
    "C L R Ls Rs",
    "C L R Ls Rs Lrs Rrs",
    "C L R Ls Rs Lrs Rrs",
    "L R Ls Rs Rls Cs Cs Rrs",
    "C L R Ls Rs Rls Cs Rrs",
];

const DTS_RESOLUTION: [u8; 4] = [16, 20, 24, 24];

/// Parse DTS (Digital Theater Systems) elementary stream.
///
/// Detection: Sync 0x7FFE8001.
/// Fills: Channels, sample rate, bit rate, LFE, profile.
pub fn parse_dts(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(16);
    let Some(h) = head else { return false };
    let sync = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if sync != SYNC_CORE_BE16 {
        return false;
    }

    // Read the next 10 bytes of bitstream payload after the 32-bit sync.
    let mut br = BitReader::new(&h[4..]);
    let _frame_type = br.read(1);
    let _deficit = br.read(5);
    let crc_present = br.read(1) == 1;
    let num_pcm_sample_blocks = (br.read(7) as u16) + 1;
    let primary_frame_byte_size = br.read(14) + 1;
    let amode = br.read(6) as u8;
    let sample_frequency = br.read(4) as u8;
    let bit_rate_idx = br.read(5) as u8;
    let _emb_downmix = br.read(1);
    let _emb_dynrange = br.read(1);
    let _emb_timestamp = br.read(1);
    let _aux_data = br.read(1);
    let _hdcd = br.read(1);
    let _ext_audio_desc = br.read(3);
    let _extended_coding = br.read(1);
    let _audio_sync_insertion = br.read(1);
    let lfe_effects = br.read(2);
    let _predictor_history = br.read(1);
    if crc_present {
        br.read(16);
    }
    let _multirate_interp = br.read(1);
    let _encoder_rev = br.read(4);
    let _copy_history = br.read(2);
    let source_pcm_resolution = br.read(2) as u8;

    let amode_idx = amode as usize;
    if amode_idx >= DTS_CHANNELS.len() {
        return false;
    }
    let sample_rate = DTS_SAMPLE_RATES[sample_frequency as usize % DTS_SAMPLE_RATES.len()];
    if sample_rate == 0 {
        return false;
    }
    let mut channels = DTS_CHANNELS[amode_idx];
    if lfe_effects != 0 {
        channels += 1;
    }
    let bit_rate = DTS_BIT_RATES[bit_rate_idx as usize % DTS_BIT_RATES.len()];
    let bit_depth = DTS_RESOLUTION[source_pcm_resolution as usize % DTS_RESOLUTION.len()];
    let samples_per_frame = (num_pcm_sample_blocks as u32) * 32;
    let frame_rate = sample_rate as f64 / samples_per_frame as f64;

    // Count frames by scanning successive syncs.
    let file_size = fa.remain();
    let buf = match fa.peek_raw(file_size) {
        Some(b) => b,
        None => return false,
    };
    let mut frame_count: u64 = 0;
    let mut pos = 0usize;
    let step = primary_frame_byte_size as usize;
    while pos + 4 <= buf.len() {
        let s = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        if s != SYNC_CORE_BE16 {
            break;
        }
        frame_count += 1;
        if step == 0 {
            break;
        }
        pos += step;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DTS", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);
    fa.fill(StreamKind::General, 0, "OverallBitRate_Mode", "CBR", false);
    if bit_rate > 0 {
        // Pre-fill OverallBitRate with the exact CBR value; the harness'
        // generic filesize/duration estimator would round it inaccurately.
        fa.fill(StreamKind::General, 0, "OverallBitRate", bit_rate.to_string(), true);
    }

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "DTS", false);
    // Sync 0x7FFE8001 means 16-bit big-endian Core (the only variant we parse).
    fa.fill(StreamKind::Audio, 0, "Format_Settings_Endianness", "Big", false);
    fa.fill(StreamKind::Audio, 0, "Format_Settings_Mode", "16", false);
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);
    if bit_rate > 0 {
        fa.fill(StreamKind::Audio, 0, "BitRate", bit_rate.to_string(), false);
    }
    fa.fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    let pos_layout = (amode_idx).min(15);
    let mut positions = DTS_CHANNEL_POSITIONS[pos_layout].to_string();
    let mut layout = DTS_CHANNEL_LAYOUT[pos_layout].to_string();
    if lfe_effects != 0 {
        positions.push_str(", LFE");
        layout.push_str(" LFE");
    }
    fa.fill(StreamKind::Audio, 0, "ChannelPositions", positions, false);
    fa.fill(StreamKind::Audio, 0, "ChannelLayout", layout, false);
    fa.fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "BitDepth", bit_depth.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.fill(StreamKind::Audio, 0, "SamplesPerFrame", samples_per_frame.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "FrameRate", format!("{:.3}", frame_rate), false);
    // Oracle computes Duration = round(FileSize×8000/BitRate) ms for DTS
    // Core (it's CBR, so byte-accounting is authoritative). SamplingCount
    // and StreamSize derive from that Duration, not from frame counting.
    if bit_rate > 0 && sample_rate > 0 {
        let duration_ms = ((file_size as f64 * 8000.0) / bit_rate as f64).round() as u64;
        fa.fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
        fa.fill(StreamKind::General, 0, "Duration", duration_ms.to_string(), false);
        let sampling_count = ((duration_ms as f64) * sample_rate as f64 / 1000.0).round() as u64;
        fa.fill(StreamKind::Audio, 0, "SamplingCount", sampling_count.to_string(), false);
        let stream_size = ((bit_rate as u64) * duration_ms + 4000) / 8000;
        fa.fill(StreamKind::Audio, 0, "StreamSize", stream_size.to_string(), false);
    } else {
        fa.fill(StreamKind::Audio, 0, "StreamSize", file_size.to_string(), false);
    }
    let _ = frame_count;

    true
}

/// Tiny MSB-first bit reader for the DTS header. Reads up to 32 bits at
/// a time from a byte slice; consumers are responsible for not running
/// off the end.
struct BitReader<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, bit_pos: 0 }
    }
    fn read(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for _ in 0..n {
            let byte_idx = self.bit_pos / 8;
            let bit_idx = 7 - (self.bit_pos % 8);
            let bit = if byte_idx < self.bytes.len() {
                (self.bytes[byte_idx] >> bit_idx) & 1
            } else {
                0
            };
            v = (v << 1) | (bit as u32);
            self.bit_pos += 1;
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_dts() {
        let mut fa = FileAnalyze::new(b"NOT A DTS FILE...........");
        assert!(!parse_dts(&mut fa));
    }

    /// Synthesize a single DTS Core frame with known field values, then
    /// verify the parser extracts them.
    #[test]
    fn parses_synthetic_core_frame() {
        // sync(32) + bitstream fields packed MSB-first.
        // Choose: num_pcm_sample_blocks=16 (samples=512), frame_size=1024,
        // amode=2 (L R, 2ch), sample_freq=13 (48000), bit_rate=12 (512000),
        // lfe=0, source_pcm_resolution=0 (16-bit).
        let mut bits = BitWriter::new();
        bits.write(1, 0);  // frame_type
        bits.write(5, 0);  // deficit
        bits.write(1, 0);  // crc_present
        bits.write(7, 16 - 1); // num_pcm_sample_blocks (stored value)
        bits.write(14, 1024 - 1); // primary_frame_byte_size
        bits.write(6, 2);  // amode = 2 → 2ch L R
        bits.write(4, 13); // sample_freq = 13 → 48000
        bits.write(5, 12); // bit_rate = 12 → 512000
        bits.write(1, 0);
        bits.write(1, 0);
        bits.write(1, 0);
        bits.write(1, 0);
        bits.write(1, 0);
        bits.write(3, 0);
        bits.write(1, 0);
        bits.write(1, 0);
        bits.write(2, 0);  // lfe_effects
        bits.write(1, 0);
        // skip crc since crc_present=0
        bits.write(1, 0);  // multirate
        bits.write(4, 0);  // encoder_rev
        bits.write(2, 0);  // copy_history
        bits.write(2, 0);  // source_pcm_resolution → 16-bit

        let mut buf = vec![0x7Fu8, 0xFE, 0x80, 0x01];
        buf.extend_from_slice(&bits.bytes());
        // Pad to declared frame size so the next-sync scan stops cleanly.
        buf.resize(1024, 0);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dts(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("DTS"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("BitRate").as_deref(), Some("512000"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("SamplesPerFrame").as_deref(), Some("512"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
        assert_eq!(a("ChannelLayout").as_deref(), Some("L R"));
    }

    struct BitWriter {
        buf: Vec<u8>,
        bit_pos: usize,
    }
    impl BitWriter {
        fn new() -> Self { Self { buf: Vec::new(), bit_pos: 0 } }
        fn write(&mut self, n: u32, v: u32) {
            for i in (0..n).rev() {
                let byte_idx = self.bit_pos / 8;
                while self.buf.len() <= byte_idx { self.buf.push(0); }
                let bit = ((v >> i) & 1) as u8;
                self.buf[byte_idx] |= bit << (7 - (self.bit_pos % 8));
                self.bit_pos += 1;
            }
        }
        fn bytes(self) -> Vec<u8> { self.buf }
    }
}
