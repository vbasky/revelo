//! AIFF (Audio Interchange File Format) parser.
//!
//! Same `FORM`-style chunked container as WAV but big-endian throughout
//! and with a different chunk vocabulary (`COMM` instead of `fmt `,
//! `SSND` instead of `data`). Sample rate is encoded as an 80-bit IEEE
//! 754 extended-precision float.
//!
//! Layout:
//!   "FORM" <u32 BE total-size-minus-8> "AIFF"
//!     chunks:
//!       "COMM" <u32 BE size> numChannels<2> numSampleFrames<4> sampleSize<2> sampleRate<10 80-bit BE>
//!       "SSND" <u32 BE size> offset<4> blockSize<4> samples<size-8>
//!       (other chunks ignored)
//!
//! BitRate formula matches the C++ side (see File_Riff_Elements.cpp's
//! WAVE_data_Continue equivalent): the stored Audio_Duration is the
//! integer-millisecond truncation of `numSampleFrames/sampleRate*1000`,
//! then BitRate = StreamSize * 8 * 1000 / Duration_ms, formatted with
//! 10 decimal digits.

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::{float80, int16u, int32u, int8u};

const FOURCC_FORM: int32u = u32::from_be_bytes(*b"FORM");
const FOURCC_AIFF: int32u = u32::from_be_bytes(*b"AIFF");
const FOURCC_COMM: int32u = u32::from_be_bytes(*b"COMM");
const FOURCC_SSND: int32u = u32::from_be_bytes(*b"SSND");

#[derive(Debug, Default)]
struct CommChunk {
    num_channels: int16u,
    num_sample_frames: int32u,
    sample_size: int16u,
    sample_rate: float80,
}

pub fn parse_aiff(fa: &mut FileAnalyze) -> bool {
    let mut magic: int32u = 0;
    fa.Peek_B4(&mut magic);
    if magic != FOURCC_FORM {
        return false;
    }

    fa.Element_Begin("FORM");
    let mut form_id: int32u = 0;
    fa.Get_C4(&mut form_id, "ID");
    let mut form_size: int32u = 0;
    fa.Get_B4(&mut form_size, "Size");
    let mut form_type: int32u = 0;
    fa.Get_C4(&mut form_type, "Type");

    if form_type != FOURCC_AIFF {
        fa.Element_End();
        return false;
    }

    let mut comm: Option<CommChunk> = None;
    let mut audio_stream_size: u64 = 0;

    while fa.Remain() >= 8 {
        let mut chunk_id: int32u = 0;
        fa.Get_C4(&mut chunk_id, "ChunkID");
        let mut chunk_size: int32u = 0;
        fa.Get_B4(&mut chunk_size, "ChunkSize");

        let chunk_size_usize = chunk_size as usize;
        if fa.Remain() < chunk_size_usize {
            break;
        }

        match chunk_id {
            FOURCC_COMM => {
                fa.Element_Begin("Common");
                let mut num_channels: int16u = 0;
                fa.Get_B2(&mut num_channels, "numChannels");
                let mut num_sample_frames: int32u = 0;
                fa.Get_B4(&mut num_sample_frames, "numSampleFrames");
                let mut sample_size: int16u = 0;
                fa.Get_B2(&mut sample_size, "sampleSize");
                let mut sample_rate: float80 = 0.0;
                fa.Get_BF10(&mut sample_rate, "sampleRate");

                let consumed: usize = 18;
                if chunk_size_usize > consumed {
                    fa.Skip_Hexa(chunk_size_usize - consumed, "Extension");
                }
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: int8u = 0;
                    fa.Get_B1(&mut _pad, "Padding");
                }
                fa.Element_End();

                comm = Some(CommChunk {
                    num_channels,
                    num_sample_frames,
                    sample_size,
                    sample_rate,
                });
            }
            FOURCC_SSND => {
                fa.Element_Begin("SoundData");
                let mut offset: int32u = 0;
                fa.Get_B4(&mut offset, "offset");
                let mut block_size: int32u = 0;
                fa.Get_B4(&mut block_size, "blockSize");
                // Actual audio data is the chunk body minus the 8-byte
                // offset+blockSize prefix.
                let samples_size = chunk_size_usize.saturating_sub(8);
                audio_stream_size = samples_size as u64;
                fa.Skip_Hexa(samples_size, "Samples");
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: int8u = 0;
                    fa.Get_B1(&mut _pad, "Padding");
                }
                fa.Element_End();
            }
            _ => {
                fa.Skip_Hexa(chunk_size_usize, "Unknown");
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: int8u = 0;
                    fa.Get_B1(&mut _pad, "Padding");
                }
            }
        }
    }

    fa.Element_End();

    if let Some(comm) = comm {
        fill_streams(fa, &comm, audio_stream_size);
        true
    } else {
        false
    }
}

fn fill_streams(fa: &mut FileAnalyze, comm: &CommChunk, audio_stream_size: u64) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "AIFF", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "PCM", false);
    fa.Fill(
        StreamKind::Audio,
        0,
        "Format_Settings_Endianness",
        "Big",
        false,
    );

    // Duration as integer milliseconds, matching the C++ AfterComma=0 fill
    // of `numSampleFrames/sampleRate*1000`.
    let duration_ms_int: i64 = if comm.sample_rate > 0.0 {
        ((comm.num_sample_frames as f64) / comm.sample_rate * 1000.0).round() as i64
    } else {
        0
    };

    fa.Fill(
        StreamKind::Audio,
        0,
        "Duration",
        duration_ms_int.to_string(),
        false,
    );
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);

    // BitRate = StreamSize * 8 * 1000 / Duration_ms_int, with the integer
    // millisecond truncation that the C++ retrieves via To_float64() of
    // the stored Audio_Duration. 10-decimal display precision to match
    // oracle's %.10f formatting.
    if duration_ms_int > 0 {
        let bitrate_f64 = (audio_stream_size as f64) * 8.0 * 1000.0 / (duration_ms_int as f64);
        fa.Fill(
            StreamKind::Audio,
            0,
            "BitRate",
            format!("{:.10}", bitrate_f64),
            false,
        );
    }

    fa.Fill(
        StreamKind::Audio,
        0,
        "Channels",
        comm.num_channels.to_string(),
        false,
    );
    // Sample rate stored as integer if it's whole; matches oracle's "48000"
    // not "48000.000" for typical AIFF.
    let sr_int: i64 = comm.sample_rate.round() as i64;
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sr_int.to_string(), false);
    fa.Fill(
        StreamKind::Audio,
        0,
        "SamplingCount",
        comm.num_sample_frames.to_string(),
        false,
    );
    fa.Fill(
        StreamKind::Audio,
        0,
        "BitDepth",
        comm.sample_size.to_string(),
        false,
    );
    fa.Fill(
        StreamKind::Audio,
        0,
        "StreamSize",
        audio_stream_size.to_string(),
        false,
    );

    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid AIFF: stereo 24-bit, with the given sample
    /// rate and frame count. SSND prefix offset/block_size are zero.
    fn make_aiff(channels: u16, sample_rate_hz: u32, bits: u16, frame_count: u32) -> Vec<u8> {
        let block_align = channels * (bits / 8);
        let data_size = frame_count * block_align as u32;
        let ssnd_chunk_size = 8 + data_size;
        let comm_chunk_size = 18u32;

        let mut buf = Vec::with_capacity(12 + 8 + comm_chunk_size as usize + 8 + ssnd_chunk_size as usize);
        buf.extend_from_slice(b"FORM");
        let form_size = 4 + (8 + comm_chunk_size) + (8 + ssnd_chunk_size);
        buf.extend_from_slice(&form_size.to_be_bytes());
        buf.extend_from_slice(b"AIFF");

        buf.extend_from_slice(b"COMM");
        buf.extend_from_slice(&comm_chunk_size.to_be_bytes());
        buf.extend_from_slice(&channels.to_be_bytes());
        buf.extend_from_slice(&frame_count.to_be_bytes());
        buf.extend_from_slice(&bits.to_be_bytes());
        buf.extend_from_slice(&encode_f80_be(sample_rate_hz as f64));

        buf.extend_from_slice(b"SSND");
        buf.extend_from_slice(&ssnd_chunk_size.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // offset
        buf.extend_from_slice(&0u32.to_be_bytes()); // blockSize
        buf.resize(buf.len() + data_size as usize, 0);
        buf
    }

    /// Encode an integer sample rate as a 10-byte 80-bit big-endian
    /// extended precision float. Only valid for positive whole numbers
    /// (the only case the AIFF spec uses in practice).
    fn encode_f80_be(value: f64) -> [u8; 10] {
        debug_assert!(value > 0.0 && value < 2f64.powi(63));
        // value = mantissa_as_u64 / 2^63 * 2^(exp - 16383)
        // Equivalently, find E such that 2^E <= value < 2^(E+1).
        let int_part = value.trunc() as u64;
        let e = 63 - int_part.leading_zeros() as i32; // floor(log2)
        // mantissa with explicit integer bit (= 1) and 63 fraction bits
        let scaled = value * 2f64.powi(63 - e);
        let mantissa = scaled.round() as u64;
        let biased_exp = (16383 + e) as u16;
        let mut out = [0u8; 10];
        out[0] = ((biased_exp >> 8) & 0x7F) as u8; // sign=0
        out[1] = (biased_exp & 0xFF) as u8;
        out[2..10].copy_from_slice(&mantissa.to_be_bytes());
        out
    }

    #[test]
    fn parse_minimal_aiff_24bit_stereo() {
        let buf = make_aiff(2, 48000, 24, 71638);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aiff(&mut fa));

        let g = |key: &str| fa.Retrieve(StreamKind::General, 0, key).map(|z| z.as_str().to_owned());
        let a = |key: &str| fa.Retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("AIFF"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Big"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("SamplingCount").as_deref(), Some("71638"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("StreamSize").as_deref(), Some("429828"));
        assert_eq!(a("Duration").as_deref(), Some("1492"));
        // BitRate = 429828 * 8 * 1000 / 1492 = 2304707.7747989...
        // Formatted with %.10f
        let br = a("BitRate").unwrap();
        assert!(br.starts_with("2304707.77479"), "got {br}");
    }

    #[test]
    fn rejects_non_form_buffer() {
        let mut fa = FileAnalyze::new(b"NOTAFORMfile");
        assert!(!parse_aiff(&mut fa));
    }

    #[test]
    fn rejects_form_with_non_aiff_type() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"FORM");
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(b"AIFC");
        buf.extend_from_slice(&[0; 4]);
        let mut fa = FileAnalyze::new(&buf);
        // AIFC is technically also FORM-style; we only handle plain AIFF
        // in this commit. AIFC support comes later.
        assert!(!parse_aiff(&mut fa));
    }

    #[test]
    fn encodes_44100_sample_rate_round_trip() {
        // Verifies our test encoder works against the canonical AIFF
        // 44100 Hz encoding (0x400E AC44 0000 0000 0000).
        let bytes = encode_f80_be(44100.0);
        assert_eq!(bytes, [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
    }
}
