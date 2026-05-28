//! WAV / RIFF-WAVE parser.
//!
//! Subset of MediaInfoLib's `File_Riff.cpp` targeting plain PCM WAV. The
//! C++ parser handles AVI, RF64, BW64, DV-in-AVI, and many other RIFF
//! variants — this is just the PCM-WAV slice, enough to validate the
//! engine architecture end-to-end against the oracle.
//!
//! Layout walked:
//!   "RIFF" <u32 LE total-size-minus-8> "WAVE"
//!     chunks:
//!       "fmt " <u32 LE size> <format details>
//!       "data" <u32 LE size> <samples>
//!       (other chunks ignored)

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{int16u, int32u, int8u};

const FOURCC_RIFF: int32u = u32::from_be_bytes(*b"RIFF");
const FOURCC_WAVE: int32u = u32::from_be_bytes(*b"WAVE");
const FOURCC_FMT: int32u = u32::from_be_bytes(*b"fmt ");
const FOURCC_DATA: int32u = u32::from_be_bytes(*b"data");

// Common WAVEFORMATEX format codes — only the ones we handle by name.
const WAVE_FORMAT_PCM: int16u = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: int16u = 0x0003;
const WAVE_FORMAT_ALAW: int16u = 0x0006;
const WAVE_FORMAT_MULAW: int16u = 0x0007;
const WAVE_FORMAT_EXTENSIBLE: int16u = 0xFFFE;

#[derive(Debug, Default)]
struct FmtChunk {
    audio_format: int16u,
    num_channels: int16u,
    sample_rate: int32u,
    #[allow(dead_code)]
    byte_rate: int32u,
    block_align: int16u,
    bits_per_sample: int16u,
}

/// Parse a WAV file buffer, filling the General and Audio streams on the
/// provided FileAnalyze. Returns `true` if a valid RIFF/WAVE container
/// was recognized.
pub fn parse_wav(fa: &mut FileAnalyze) -> bool {
    let mut magic: int32u = 0;
    fa.peek_b4(&mut magic);
    if magic != FOURCC_RIFF {
        return false;
    }

    fa.element_begin("RIFF");
    let mut riff_id: int32u = 0;
    fa.get_c4(&mut riff_id, "ID");
    let mut riff_size: int32u = 0;
    fa.get_l4(&mut riff_size, "Size");
    let mut form_type: int32u = 0;
    fa.get_c4(&mut form_type, "Type");

    if form_type != FOURCC_WAVE {
        fa.element_end();
        return false;
    }

    let mut fmt: Option<FmtChunk> = None;
    let mut data_size: u32 = 0;

    while fa.remain() >= 8 {
        let mut chunk_id: int32u = 0;
        fa.get_c4(&mut chunk_id, "ChunkID");
        let mut chunk_size: int32u = 0;
        fa.get_l4(&mut chunk_size, "ChunkSize");

        let chunk_size_usize = chunk_size as usize;
        if fa.remain() < chunk_size_usize {
            break;
        }

        match chunk_id {
            FOURCC_FMT => {
                fa.element_begin("fmt");
                let mut audio_format: int16u = 0;
                fa.get_l2(&mut audio_format, "AudioFormat");
                let mut num_channels: int16u = 0;
                fa.get_l2(&mut num_channels, "NumChannels");
                let mut sample_rate: int32u = 0;
                fa.get_l4(&mut sample_rate, "SampleRate");
                let mut byte_rate: int32u = 0;
                fa.get_l4(&mut byte_rate, "ByteRate");
                let mut block_align: int16u = 0;
                fa.get_l2(&mut block_align, "BlockAlign");
                let mut bits_per_sample: int16u = 0;
                fa.get_l2(&mut bits_per_sample, "BitsPerSample");

                // Consume any trailing extension bytes within this chunk.
                let consumed_in_fmt: usize = 16;
                if chunk_size_usize > consumed_in_fmt {
                    fa.skip_hexa(chunk_size_usize - consumed_in_fmt, "Extension");
                }
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: int8u = 0;
                    fa.get_b1(&mut _pad, "Padding");
                }

                fa.element_end();
                fmt = Some(FmtChunk {
                    audio_format,
                    num_channels,
                    sample_rate,
                    byte_rate,
                    block_align,
                    bits_per_sample,
                });
            }
            FOURCC_DATA => {
                fa.element_begin("data");
                data_size = chunk_size;
                fa.skip_hexa(chunk_size_usize, "Samples");
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: int8u = 0;
                    fa.get_b1(&mut _pad, "Padding");
                }
                fa.element_end();
            }
            _ => {
                // Unknown chunk — skip it, honoring word-alignment.
                fa.skip_hexa(chunk_size_usize, "Unknown");
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: int8u = 0;
                    fa.get_b1(&mut _pad, "Padding");
                }
            }
        }
    }

    fa.element_end();

    if let Some(fmt) = fmt {
        fill_streams(fa, &fmt, data_size);
        true
    } else {
        false
    }
}

fn fill_streams(fa: &mut FileAnalyze, fmt: &FmtChunk, data_size: u32) {
    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "Wave", false);

    if fmt.audio_format == WAVE_FORMAT_PCM {
        fa.fill(StreamKind::General, 0, "Format_Settings", "PcmWaveformat", false);
    }

    fa.stream_prepare(StreamKind::Audio);
    let (fmt_name, endianness, sign) = wav_format_descriptors(fmt);
    fa.fill(StreamKind::Audio, 0, "Format", fmt_name, false);
    if let Some(e) = endianness {
        fa.fill(StreamKind::Audio, 0, "Format_Settings_Endianness", e, false);
    }
    if let Some(s) = sign {
        fa.fill(StreamKind::Audio, 0, "Format_Settings_Sign", s, false);
    }
    fa.fill(
        StreamKind::Audio,
        0,
        "CodecID",
        fmt.audio_format.to_string(),
        false,
    );

    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);

    let bitrate = (fmt.sample_rate as u64) * (fmt.num_channels as u64) * (fmt.bits_per_sample as u64);
    if bitrate > 0 {
        fa.fill(StreamKind::Audio, 0, "BitRate", bitrate.to_string(), false);
    }

    fa.fill(StreamKind::Audio, 0, "Channels", fmt.num_channels.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "SamplingRate", fmt.sample_rate.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "BitDepth", fmt.bits_per_sample.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "StreamSize", data_size.to_string(), false);

    if fmt.block_align > 0 {
        let sample_count = data_size as u64 / fmt.block_align as u64;
        fa.fill(StreamKind::Audio, 0, "SamplingCount", sample_count.to_string(), false);
        if fmt.sample_rate > 0 {
            // Duration in milliseconds, matching C++ output convention.
            let duration_ms = (sample_count * 1000) / fmt.sample_rate as u64;
            fa.fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
        }
    }

    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);
}

fn wav_format_descriptors(fmt: &FmtChunk) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match fmt.audio_format {
        WAVE_FORMAT_PCM => {
            let sign = if fmt.bits_per_sample <= 8 {
                Some("Unsigned")
            } else {
                Some("Signed")
            };
            ("PCM", Some("Little"), sign)
        }
        WAVE_FORMAT_IEEE_FLOAT => ("PCM", Some("Little"), Some("Signed")),
        WAVE_FORMAT_ALAW => ("A-law", None, None),
        WAVE_FORMAT_MULAW => ("µ-law", None, None),
        WAVE_FORMAT_EXTENSIBLE => ("PCM", Some("Little"), None),
        _ => ("Unknown", None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid PCM WAV: 1 channel, 8000 Hz, 16-bit, with
    /// `frame_count` audio frames (each 2 bytes). Returns the buffer.
    fn make_pcm_wav(channels: u16, sample_rate: u32, bits: u16, frame_count: u32) -> Vec<u8> {
        let block_align = channels * (bits / 8);
        let data_size = frame_count * block_align as u32;
        let byte_rate = sample_rate * (block_align as u32);
        let mut buf = Vec::with_capacity(44 + data_size as usize);

        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_size).to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits.to_le_bytes());

        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(buf.len() + data_size as usize, 0);
        buf
    }

    #[test]
    fn parse_minimal_pcm_wav() {
        let buf = make_pcm_wav(1, 8000, 16, 1600); // 1ch, 8kHz, 16-bit, 0.2s of silence
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));

        let g = |key: &str| fa.retrieve(StreamKind::General, 0, key).map(|z| z.as_str().to_owned());
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Wave"));
        assert_eq!(g("Format_Settings").as_deref(), Some("PcmWaveformat"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Little"));
        assert_eq!(a("Format_Settings_Sign").as_deref(), Some("Signed"));
        assert_eq!(a("CodecID").as_deref(), Some("1"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("BitRate").as_deref(), Some("128000")); // 8000*1*16
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("SamplingRate").as_deref(), Some("8000"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("SamplingCount").as_deref(), Some("1600"));
        assert_eq!(a("Duration").as_deref(), Some("200")); // 1600/8000 = 0.2s = 200ms
        assert_eq!(a("StreamSize").as_deref(), Some("3200"));
    }

    #[test]
    fn parse_stereo_48khz_24bit() {
        let buf = make_pcm_wav(2, 48000, 24, 48000); // 2ch, 48kHz, 24-bit, 1s
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("BitRate").as_deref(), Some("2304000"));
        assert_eq!(a("Duration").as_deref(), Some("1000"));
    }

    #[test]
    fn eight_bit_pcm_is_unsigned() {
        let buf = make_pcm_wav(1, 11025, 8, 100);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format_Settings_Sign")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("Unsigned")
        );
    }

    #[test]
    fn non_riff_buffer_returns_false() {
        let buf = b"This is not a WAV file at all";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_wav(&mut fa));
    }

    #[test]
    fn riff_without_wave_form_type_returns_false() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&8u32.to_le_bytes());
        buf.extend_from_slice(b"AVI ");
        buf.extend_from_slice(&[0; 4]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_wav(&mut fa));
    }

    #[test]
    fn skips_unknown_chunks_between_fmt_and_data() {
        // Build a WAV with a LIST chunk between fmt and data.
        let mut buf = Vec::new();
        let frame_count = 100u32;
        let block_align = 2u16;
        let data_size = frame_count * block_align as u32;
        let list_size = 12u32;

        buf.extend_from_slice(b"RIFF");
        let riff_size = 4 + (8 + 16) + (8 + list_size as u32) + (8 + data_size);
        buf.extend_from_slice(&riff_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        // fmt
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&8000u32.to_le_bytes());
        buf.extend_from_slice(&16000u32.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());

        // LIST chunk (unknown, should be skipped)
        buf.extend_from_slice(b"LIST");
        buf.extend_from_slice(&list_size.to_le_bytes());
        buf.resize(buf.len() + list_size as usize, 0xAA);

        // data
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(buf.len() + data_size as usize, 0);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "SamplingCount")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("100")
        );
    }
}
