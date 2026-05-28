//! Sun/NeXT AU audio (.au, .snd) parser.
//!
//! Header layout (all big-endian, 24 bytes minimum):
//!   0x00  magic        u32  = 0x2E736E64 (".snd")
//!   0x04  data_offset  u32  bytes from start to audio data (>=24)
//!   0x08  data_size    u32  audio data size; 0xFFFFFFFF if unknown
//!   0x0C  encoding     u32  sample format code (see ENCODING_*)
//!   0x10  sample_rate  u32  Hz
//!   0x14  channels     u32
//!   0x18  ...          arbitrary annotation bytes up to data_offset

use revelio_core::{FileAnalyze, StreamKind};

const AU_MAGIC: [u8; 4] = [0x2E, 0x73, 0x6E, 0x64];

/// Returns ("Format", "CodecID/Codec long name", bit_depth, compression_mode_lossless).
/// `bit_depth` is 0 when not applicable (compressed/non-PCM).
fn map_encoding(enc: u32) -> Option<(&'static str, &'static str, u16, bool)> {
    match enc {
        1  => Some(("ADPCM", "8-bit mu-law",                                  8,  false)),
        2  => Some(("PCM",   "8-bit signed linear",                           8,  true)),
        3  => Some(("PCM",   "16-bit signed linear",                          16, true)),
        4  => Some(("PCM",   "24-bit signed linear",                          24, true)),
        5  => Some(("PCM",   "32-bit signed linear",                          32, true)),
        6  => Some(("PCM",   "floating-point",                                32, true)),
        7  => Some(("PCM",   "double precision float",                        64, true)),
        8  => Some(("fragmented sampled data", "fragmented sampled data",     0,  false)),
        10 => Some(("DSP program", "DSP program",                             0,  false)),
        11 => Some(("PCM",   "8-bit fixed-point",                             8,  true)),
        12 => Some(("PCM",   "16-bit fixed-point",                            16, true)),
        13 => Some(("PCM",   "24-bit fixed-point",                            24, true)),
        14 => Some(("PCM",   "32-bit fixed-point",                            32, true)),
        17 => Some(("ADPCM", "mu-law squelch",                                0,  false)),
        18 => Some(("PCM",   "16-bit linear with emphasis",                   16, true)),
        19 => Some(("PCM",   "16-bit linear with compression",                16, false)),
        20 => Some(("PCM",   "16-bit linear with emphasis and compression",   16, false)),
        21 => Some(("Music Kit DSP commands", "Music Kit DSP commands",       0,  false)),
        22 => Some(("Music Kit DSP samples",  "Music Kit DSP samples",        0,  false)),
        23 => Some(("ADPCM", "G.721 ADPCM",                                   0,  false)),
        24 => Some(("ADPCM", "G.722 ADPCM",                                   0,  false)),
        25 => Some(("ADPCM", "G.723 ADPCM",                                   0,  false)),
        26 => Some(("ADPCM", "5-bit G.723 ADPCM",                             0,  false)),
        27 => Some(("ADPCM", "8-bit a-law",                                   8,  false)),
        _  => None,
    }
}

pub fn parse_au(fa: &mut FileAnalyze) -> bool {
    let file_size = fa.Remain();
    let head = fa.peek_raw(fa.Remain().min(24));
    let Some(h) = head else { return false };
    if h.len() < 24 || h[..4] != AU_MAGIC {
        return false;
    }

    let data_offset  = u32::from_be_bytes([h[4],  h[5],  h[6],  h[7]]);
    let data_size    = u32::from_be_bytes([h[8],  h[9],  h[10], h[11]]);
    let encoding     = u32::from_be_bytes([h[12], h[13], h[14], h[15]]);
    let sample_rate  = u32::from_be_bytes([h[16], h[17], h[18], h[19]]);
    let channels     = u32::from_be_bytes([h[20], h[21], h[22], h[23]]);

    if data_offset < 24 {
        return false;
    }

    let (format, codec, bit_depth, lossless) =
        map_encoding(encoding).unwrap_or(("PCM", "", 0, true));

    // Prefer file-size-derived data_size when available (matches C++:
    // File_Size!=(int64u)-1 ⇒ data_size = File_Size - data_start).
    let effective_data_size: u64 = if (file_size as u64) >= data_offset as u64 {
        (file_size as u64) - (data_offset as u64)
    } else if data_size != 0 && data_size != 0xFFFF_FFFF {
        data_size as u64
    } else {
        0
    };

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "AU", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", format, false);
    if !codec.is_empty() {
        fa.Fill(StreamKind::Audio, 0, "CodecID", codec, false);
        fa.Fill(StreamKind::Audio, 0, "Codec", codec, false);
    }
    fa.Fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    if bit_depth > 0 {
        fa.Fill(StreamKind::Audio, 0, "BitDepth", bit_depth.to_string(), false);
    }
    // AU PCM and companding codecs are big-endian by definition.
    if format == "PCM" || format == "ADPCM" {
        fa.Fill(StreamKind::Audio, 0, "Format_Settings_Endianness", "Big", false);
    }
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);
    fa.Fill(
        StreamKind::Audio,
        0,
        "Compression_Mode",
        if lossless { "Lossless" } else { "Lossy" },
        false,
    );

    // Duration: C++ uses data_size*1000/sample_rate, which is bytes/Hz —
    // valid only because all AU "PCM" entries are 8-bit-per-sample-per-channel
    // in the original Sun definition. Replicate exactly for byte parity.
    if sample_rate > 0 && effective_data_size > 0 {
        let duration_ms = effective_data_size * 1000 / sample_rate as u64;
        fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);

        // BitRate derivable for PCM where bit_depth & channels known.
        if bit_depth > 0 && channels > 0 {
            let bitrate = (sample_rate as u64) * (bit_depth as u64) * (channels as u64);
            fa.Fill(StreamKind::Audio, 0, "BitRate", bitrate.to_string(), false);
        }
    }

    let stream_size = (file_size as u64).saturating_sub(data_offset as u64);
    fa.Fill(StreamKind::Audio, 0, "StreamSize", stream_size.to_string(), false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_au(encoding: u32, sample_rate: u32, channels: u32, data: &[u8]) -> Vec<u8> {
        let data_offset: u32 = 24;
        let data_size: u32 = data.len() as u32;
        let mut v = Vec::new();
        v.extend_from_slice(&AU_MAGIC);
        v.extend_from_slice(&data_offset.to_be_bytes());
        v.extend_from_slice(&data_size.to_be_bytes());
        v.extend_from_slice(&encoding.to_be_bytes());
        v.extend_from_slice(&sample_rate.to_be_bytes());
        v.extend_from_slice(&channels.to_be_bytes());
        v.extend_from_slice(data);
        v
    }

    #[test]
    fn rejects_non_au() {
        let mut fa = FileAnalyze::new(b"NOT AN AU FILE..........");
        assert!(!parse_au(&mut fa));
    }

    #[test]
    fn parses_pcm16_stereo_44100() {
        // 16-bit signed linear, 44100 Hz, stereo, 4 bytes/sample-frame.
        let data = vec![0u8; 44100 * 4]; // 1 second
        let buf = build_au(3, 44100, 2, &data);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_au(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("AU"));
        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Big"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("CodecID").as_deref(), Some("16-bit signed linear"));
    }

    #[test]
    fn parses_mulaw_and_alaw() {
        // mu-law 8000 Hz mono.
        let mu_buf = build_au(1, 8000, 1, &vec![0u8; 8000]);
        let mut fa = FileAnalyze::new(&mu_buf);
        assert!(parse_au(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("ADPCM")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, 0, "CodecID").map(|z| z.as_str().to_owned()).as_deref(),
            Some("8-bit mu-law")
        );

        // a-law 8000 Hz mono (encoding 27).
        let a_buf = build_au(27, 8000, 1, &vec![0u8; 8000]);
        let mut fa2 = FileAnalyze::new(&a_buf);
        assert!(parse_au(&mut fa2));
        assert_eq!(
            fa2.Retrieve(StreamKind::Audio, 0, "CodecID").map(|z| z.as_str().to_owned()).as_deref(),
            Some("8-bit a-law")
        );
        assert_eq!(
            fa2.Retrieve(StreamKind::Audio, 0, "Compression_Mode").map(|z| z.as_str().to_owned()).as_deref(),
            Some("Lossy")
        );
    }
}
