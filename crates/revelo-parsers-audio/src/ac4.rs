//! Dolby AC-4 parser — full frame header with IMS/JOC support.
//!
//! Frame layout (big-endian):
//!   0xAC40 / 0xAC41          sync word (2 bytes)
//!   bit_rate_mode(1)          CBR=0 / VBR=1
//!   b_low_delay(1)            low delay mode flag
//!   reserved(2)               always 0
//!   ac4_version(4)            version nibble
//!   frame_length(16)          length of this frame in bytes
//!   ... substream data ...
//!
//! Per-substream fields:
//!   substream_index(4)
//!   frame_rate(3)
//!   sample_rate(4)
//!   channel_mode(4)
//!   ims_flag(1)               Immersive Stereo present
//!   joc_flag(1)               Joint Object Coding present
//!   joc_num_objects(5)        if joc_flag==1
//!   loudness_info(12+)        dialog level, etc.

use revelo_core::{FileAnalyze, StreamKind};

const AC4_SYNC_0: [u8; 2] = [0xAC, 0x40];
const AC4_SYNC_1: [u8; 2] = [0xAC, 0x41];

// Sample rate table keyed by the 4-bit sr_code.
const SAMPLE_RATES: [u32; 16] = [44100, 48000, 96000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

// Channel mode lookup (ac4_channel_mode -> channel count).
const CHANNEL_COUNTS: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0];


/// Read `n` bits big-endian from `data` starting at absolute bit `off`.
fn get_bits(data: &[u8], off: usize, n: usize) -> Option<u32> {
    if n == 0 {
        return Some(0);
    }
    if off + n > data.len() * 8 {
        return None;
    }
    let mut v = 0u32;
    for i in 0..n {
        let bit = off + i;
        let byte = data[bit / 8];
        v = (v << 1) | ((byte >> (7 - (bit % 8))) & 1) as u32;
    }
    Some(v)
}

pub fn parse_ac4(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain()).map(|b| b.to_vec()) {
        Some(b) => b,
        None => return false,
    };

    // Sync word check
    if buf.len() < 2 {
        return false;
    }
    let sync = [buf[0], buf[1]];
    let has_crc = sync == AC4_SYNC_1;
    if sync != AC4_SYNC_0 && sync != AC4_SYNC_1 {
        return false;
    }

    // Minimum frame: sync(2) + frame_header(4) = 6 bytes
    if buf.len() < 6 {
        return false;
    }

    let file_size = fa.remain();
    let mut bit_off = 16; // skip sync word

    let bit_rate_mode = get_bits(&buf, bit_off, 1).unwrap_or(1);
    bit_off += 1;
    let _b_low_delay = get_bits(&buf, bit_off, 1).unwrap_or(0);
    bit_off += 1;
    // reserved (2 bits)
    bit_off += 2;
    let _ac4_version = get_bits(&buf, bit_off, 4).unwrap_or(0);
    bit_off += 4;
    let frame_length = get_bits(&buf, bit_off, 16).unwrap_or(0);
    bit_off += 16;

    // ── Substream parsing ──────────────────────────────────────
    let num_substreams = get_bits(&buf, bit_off, 4).unwrap_or(1);
    bit_off += 4;

    let mut total_channels: u8 = 0;
    let mut has_ims = false;
    let mut has_joc = false;
    let mut joc_objects: u8 = 0;
    let mut sample_rate: u32 = 0;
    let mut dialnorm: u8 = 255; // not found

    for _ss in 0..num_substreams {
        let _ss_idx = get_bits(&buf, bit_off, 4).unwrap_or(0);
        bit_off += 4;

        let _frame_rate = get_bits(&buf, bit_off, 3).unwrap_or(0);
        bit_off += 3;

        let sr_code = get_bits(&buf, bit_off, 4).unwrap_or(0);
        bit_off += 4;
        if sr_code < 16 {
            let sr = SAMPLE_RATES[sr_code as usize];
            if sr > 0 {
                sample_rate = sr;
            }
        }

        let ch_mode = get_bits(&buf, bit_off, 4).unwrap_or(0);
        bit_off += 4;
        if (ch_mode as usize) < CHANNEL_COUNTS.len() {
            let cc = CHANNEL_COUNTS[ch_mode as usize];
            if cc > total_channels {
                total_channels = cc;
            }
        }

        // IMS flag
        if let Some(ims) = get_bits(&buf, bit_off, 1) {
            has_ims = ims != 0;
        }
        bit_off += 1;

        // JOC flag
        if let Some(joc) = get_bits(&buf, bit_off, 1) {
            has_joc = joc != 0;
        }
        bit_off += 1;

        // JOC number of objects
        if has_joc {
            if let Some(n_objs) = get_bits(&buf, bit_off, 5) {
                joc_objects = n_objs as u8;
            }
            bit_off += 5;
        }

        // Loudness info (skip for now, but note dialog level position)
        let loudness_type = get_bits(&buf, bit_off, 4).unwrap_or(0);
        bit_off += 4;
        if loudness_type == 3 {
            // dialog_level present
            if let Some(dl) = get_bits(&buf, bit_off, 5) {
                dialnorm = dl as u8;
            }
            bit_off += 5;
        }

        // Skip remaining loudness bits for this substream
        // (measurement_count, measurements, etc.)
    }

    // ── Fill metadata fields ───────────────────────────────────
    let commercial_name = if has_ims || has_joc { "Dolby AC-4 Immersive" } else { "Dolby AC-4" };

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "AC-4");
    fa.set_field(StreamKind::General, 0, "Format_Commercial_IfAny", commercial_name);
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "AC-4");
    fa.set_field(StreamKind::Audio, 0, "Format_Commercial_IfAny", commercial_name);

    if bit_rate_mode == 0 {
        fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");
    } else {
        fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "VBR");
    }
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");

    if sample_rate > 0 {
        fa.set_field(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string());
    }
    if total_channels > 0 {
        fa.set_field(StreamKind::Audio, 0, "Channels", total_channels.to_string());
    }
    if has_crc {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings_CRC", "Yes");
    }

    // IMS / JOC features
    if has_ims {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings", "IMS");
        fa.set_field(StreamKind::Audio, 0, "Format_AdditionalFeatures", "Immersive Stereo");
    }
    if has_joc {
        let existing = if has_ims { "IMS / JOC" } else { "JOC" };
        fa.set_field(StreamKind::Audio, 0, "Format_Settings", existing);
        let joc_str = format!("Joint Object Coding ({} objects)", joc_objects);
        fa.set_field(StreamKind::Audio, 0, "Format_AdditionalFeatures", joc_str.as_str());
    }

    // Duration / frame count from frame_length
    if frame_length > 0 {
        let approx_frames = file_size / frame_length as usize;
        if approx_frames > 0 {
            fa.set_field(StreamKind::Audio, 0, "FrameCount", approx_frames.to_string());
        }
    }

    // Dialnorm from loudness metadata
    if dialnorm != 255 {
        let dialnorm_display = if dialnorm == 0 { -31i32 } else { -(dialnorm as i32) };
        fa.set_field(StreamKind::Audio, 0, "dialnorm", dialnorm_display.to_string());
        fa.set_field(StreamKind::Audio, 0, "dialnorm_Average", dialnorm_display.to_string());
    }

    fa.set_field(StreamKind::Audio, 0, "StreamSize", file_size.to_string());

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ac4() {
        let mut fa = FileAnalyze::new(b"NOT AC4");
        assert!(!parse_ac4(&mut fa));
    }

    #[test]
    fn accepts_sync_ac40() {
        let mut buf = vec![0xAC, 0x40];
        buf.extend(std::iter::repeat_n(0u8, 64));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac4(&mut fa));
    }

    #[test]
    fn accepts_sync_ac41() {
        let mut buf = vec![0xAC, 0x41];
        buf.extend(std::iter::repeat_n(0u8, 64));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ac4(&mut fa));
    }
}
