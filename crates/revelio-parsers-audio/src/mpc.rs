//! Musepack SV7 (.mpc) parser — lossy perceptual audio codec.
//!
//! Mirrors MediaInfoLib's `File_Mpc.cpp` (SV7 path).
//! Source: <http://trac.musepack.net/trac/wiki/SV7Specification>
//!
//! Magic: "MP+" (3 bytes) followed by 1 byte whose low nibble == 7
//!        (high nibble = PNS, low nibble = StreamVersion).
//!
//! Header layout (25 bytes total, all multi-byte fields little-endian):
//!   3 bytes:  "MP+"                              // signature
//!   1 byte:   PNS (4 bits) | StreamVersion (4 bits)
//!   4 bytes LE: FrameCount
//!   2 bytes LE: MaxLevel
//!   2 bytes (bit-packed MSB-first):
//!     4 bits: Profile
//!     2 bits: Link
//!     2 bits: SampleFreq         (index into Mpc_SampleFreq[])
//!     1 bit:  IntensityStereo
//!     1 bit:  MidSideStereo
//!     6 bits: MaxBand
//!   2 bytes LE: TitlePeak
//!   2 bytes LE: TitleGain        (signed milli-dB)
//!   2 bytes LE: AlbumPeak
//!   2 bytes LE: AlbumGain        (signed milli-dB)
//!   4 bytes (bit-packed MSB-first):
//!     16 bits: unused
//!      4 bits: LastFrameLength part 1
//!      1 bit:  FastSeekingSafe
//!      3 bits: unused
//!      1 bit:  TrueGapless
//!      7 bits: LastFrameLength part 2
//!   1 byte:   EncoderVersion     (e.g. 115 → "1.15")

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{Int8u, Int16u, Int32u};

const HEADER_SIZE: u64 = 25;
const SAMPLES_PER_FRAME: u64 = 1152;

const MPC_SAMPLE_FREQ: [u32; 4] = [44100, 48000, 37800, 32000];

const MPC_PROFILE: [&str; 16] = [
    "no profile",
    "Unstable/Experimental",
    "",
    "",
    "",
    "Below Telephone (q=0)",
    "Below Telephone (q=1)",
    "Telephone (q=2)",
    "Thumb (q=3)",
    "Radio (q=4)",
    "Standard (q=5)",
    "Xtreme (q=6)",
    "Insane (q=7)",
    "BrainDead (q=8)",
    "Above BrainDead (q=9)",
    "Above BrainDead (q=10)",
];

/// Parse Musepack audio (SV7/SV8).
///
/// Detection: `MP+` (SV7) / `MPCK` (SV8) magic.
/// Fills: Profile, quality, sample rate, channels.
pub fn parse_mpc(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.remain().min(4));
    let Some(h) = head else { return false };
    if h.len() < 4 {
        return false;
    }
    if &h[0..3] != b"MP+" || (h[3] & 0x0F) != 7 {
        return false;
    }
    if fa.remain() < HEADER_SIZE as usize {
        return false;
    }

    // Capture total buffer size for BitRate denominator (matches C++ File_Size).
    let file_size = fa.remain() as u64;

    fa.element_begin("SV7 header");

    let _sig = fa.read_raw(3).to_vec();

    fa.bs_begin();
    let mut pns: Int8u = 0;
    let mut version: Int8u = 0;
    fa.get_s1(4, &mut pns, "PNS");
    fa.get_s1(4, &mut version, "Version");
    fa.bs_end();

    let mut frame_count: Int32u = 0;
    fa.get_l4(&mut frame_count, "FrameCount");

    fa.skip_l2("MaxLevel");

    fa.bs_begin();
    let mut profile: Int8u = 0;
    let mut link: Int8u = 0;
    let mut sample_freq_idx: Int8u = 0;
    fa.get_s1(4, &mut profile, "Profile");
    fa.get_s1(2, &mut link, "Link");
    fa.get_s1(2, &mut sample_freq_idx, "SampleFreq");
    fa.skip_s1(1, "IntensityStereo");
    fa.skip_s1(1, "MidSideStereo");
    fa.skip_s1(6, "MaxBand");
    fa.bs_end();

    fa.skip_l2("TitlePeak");
    let mut title_gain: Int16u = 0;
    fa.get_l2(&mut title_gain, "TitleGain");

    fa.skip_l2("AlbumPeak");
    let mut album_gain: Int16u = 0;
    fa.get_l2(&mut album_gain, "AlbumGain");

    fa.bs_begin();
    fa.skip_s2(16, "unused");
    fa.skip_s1(4, "LastFrameLength (part 1)");
    fa.skip_s1(1, "FastSeekingSafe");
    fa.skip_s1(3, "unused");
    fa.skip_s1(1, "TrueGapless");
    fa.skip_s1(7, "LastFrameLength (part 2)");
    fa.bs_end();

    let mut encoder_version: Int8u = 0;
    fa.get_l1(&mut encoder_version, "EncoderVersion");

    fa.element_end();

    let sample_rate = MPC_SAMPLE_FREQ[(sample_freq_idx as usize) & 0x03];
    if sample_rate == 0 || frame_count == 0 {
        return false;
    }

    let samples = (frame_count as u64) * SAMPLES_PER_FRAME;
    let duration_ms = samples * 1000 / (sample_rate as u64);
    // C++: (File_Size-25)*8*SampleFreq/FrameCount/1152
    let bit_rate = (file_size - HEADER_SIZE) * 8 * (sample_rate as u64)
        / (frame_count as u64)
        / SAMPLES_PER_FRAME;

    let encoder = format_encoder_version(encoder_version);

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "Musepack", false);
    fa.fill(StreamKind::General, 0, "Format_Version", "Version 7", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "Musepack", false);
    fa.fill(StreamKind::Audio, 0, "Format_Version", "Version 7", false);
    fa.fill(StreamKind::Audio, 0, "Codec", "SV7", false);
    let profile_name = MPC_PROFILE[(profile as usize) & 0x0F];
    if !profile_name.is_empty() {
        fa.fill(StreamKind::Audio, 0, "Codec_Settings", profile_name, false);
    }
    if !encoder.is_empty() {
        fa.fill(StreamKind::Audio, 0, "Encoded_Library", encoder, false);
    }
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.fill(StreamKind::Audio, 0, "BitRate", bit_rate.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Channels", "2", false);
    fa.fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "SamplingCount", samples.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "BitDepth", "16", false);
    fa.fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);

    let _ = (pns, link, title_gain, album_gain);
    true
}

/// EncoderVersion is encoded as an integer that, divided by 100, is the
/// human-readable version (e.g. 115 → "1.15"). C++ appends " Beta" when
/// `v % 10 != 0 && v % 2 == 0` and " Alpha" when `v % 2 == 1`; release
/// builds (v % 10 == 0) get no suffix.
fn format_encoder_version(v: Int8u) -> String {
    if v == 0 {
        return String::new();
    }
    let base = format!("{:.2}", (v as f32) / 100.0);
    if v.is_multiple_of(10) {
        base
    } else if v.is_multiple_of(2) {
        format!("{} Beta", base)
    } else {
        format!("{} Alpha", base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct a minimal SV7 header. `sample_freq_idx` is 0..3,
    /// `profile` is 0..15. Pads with `audio_size` bytes of zeros after
    /// the 25-byte header so BitRate math has a stream to chew on.
    fn make_mpc(
        sample_freq_idx: u8,
        profile: u8,
        frame_count: u32,
        encoder_version: u8,
        audio_size: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MP+");
        // PNS=0 (high nibble), Version=7 (low nibble)
        buf.push(0x07);
        buf.extend_from_slice(&frame_count.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // MaxLevel

        // Bit-packed 16 bits: profile(4) | link(2) | sfreq(2) | is(1) | ms(1) | maxband(6)
        let packed1: u16 = ((profile as u16 & 0x0F) << 12) // link = 0
            | ((sample_freq_idx as u16 & 0x03) << 8); // intensity/midside/maxband all 0
        buf.extend_from_slice(&packed1.to_be_bytes());

        buf.extend_from_slice(&0u16.to_le_bytes()); // TitlePeak
        buf.extend_from_slice(&0u16.to_le_bytes()); // TitleGain
        buf.extend_from_slice(&0u16.to_le_bytes()); // AlbumPeak
        buf.extend_from_slice(&0u16.to_le_bytes()); // AlbumGain

        // 32-bit gapless/seek packed field — all zeros works for the parser.
        buf.extend_from_slice(&0u32.to_be_bytes());

        buf.push(encoder_version);

        assert_eq!(buf.len(), 25);
        buf.resize(buf.len() + audio_size, 0);
        buf
    }

    #[test]
    fn rejects_non_mpc_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a Musepack file......");
        assert!(!parse_mpc(&mut fa));
    }

    #[test]
    fn rejects_mpc_with_wrong_version() {
        // "MP+" but version nibble != 7 → SV8 or garbage; SV7 parser refuses.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MP+");
        buf.push(0x08);
        buf.resize(64, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_mpc(&mut fa));
    }

    #[test]
    fn parses_minimal_sv7() {
        // 1000 frames at 44100 Hz → 1152000 samples → 26122 ms.
        // FrameCount=1000, EncoderVersion=115 → "1.15 Alpha".
        // Stream payload = 9975 bytes → file_size = 10000.
        let buf = make_mpc(0, 10, 1000, 115, 9975);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpc(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Musepack"));
        assert_eq!(g("Format_Version").as_deref(), Some("Version 7"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(a("Format").as_deref(), Some("Musepack"));
        assert_eq!(a("Format_Version").as_deref(), Some("Version 7"));
        assert_eq!(a("Codec").as_deref(), Some("SV7"));
        assert_eq!(a("Codec_Settings").as_deref(), Some("Standard (q=5)"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("SamplingCount").as_deref(), Some("1152000"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
        assert_eq!(a("Duration").as_deref(), Some("26122"));
        // BitRate = (10000-25)*8*44100/1000/1152 = 9975*8*44100/1000/1152
        //        = 3519180/1152 = 3054 (sequential integer division per C++)
        assert_eq!(a("BitRate").as_deref(), Some("3054"));
        assert_eq!(a("Encoded_Library").as_deref(), Some("1.15 Alpha"));
    }

    #[test]
    fn parses_sv7_48khz_with_release_encoder() {
        // sample_freq_idx=1 → 48000 Hz. EncoderVersion=110 → "1.10" (no suffix).
        let buf = make_mpc(1, 11, 500, 110, 1000);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpc(&mut fa));

        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        // samples = 500*1152 = 576000; duration = 576000*1000/48000 = 12000
        assert_eq!(a("SamplingCount").as_deref(), Some("576000"));
        assert_eq!(a("Duration").as_deref(), Some("12000"));
        assert_eq!(a("Codec_Settings").as_deref(), Some("Xtreme (q=6)"));
        assert_eq!(a("Encoded_Library").as_deref(), Some("1.10"));
    }
}
