//! VP9 video parser.
//!
//! Mirrors MediaInfoLib's `File_Vp9.cpp`. Handles the VP9 uncompressed
//! header to extract profile, bit depth, chroma subsampling, width, height,
//! and color info from key frames.

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::int32u;

const VP9_COLORSPACE_MAP: [u8; 8] = [2, 5, 1, 6, 7, 9, 2, 0];

const VP9_CHROMA_SUBSAMPLING: [&str; 4] = ["", "4:4:0", "4:2:2", "4:2:0"];

const VP9_CHROMA_SUBSAMPLING_OOB: [u8; 4] = [3, 3, 2, 0];

const VP9_COLOR_RANGE: [&str; 2] = ["Limited", "Full"];

pub fn parse_vp9(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < 6 {
        return false;
    }

    fa.Element_Begin("VP9");

    // Attempt to detect VP9 by checking the frame marker + key frame sync code
    // FFmpeg/WebM convention: the first frame starts with 0x82 (frame_marker=2,
    // version_lsb=0, version_msb=0 -> profile 0).
    let mut first_byte: u8 = 0;
    fa.Peek_B1(&mut first_byte);
    if (first_byte >> 6) != 0x02 {
        // Frame marker must be 0b10 (top 2 bits)
        // Could also be an IVF or other container — let the caller decide
        fa.Element_End();
        return false;
    }

    fa.BS_Begin();
    let mut marker: u8 = 0;
    fa.Get_S1(2, &mut marker, "FRAME_MARKER");

    if marker != 0x02 {
        fa.BS_End();
        fa.Element_End();
        return false;
    }

    let mut version0: u8 = 0;
    let mut version1: u8 = 0;
    fa.Get_S1(1, &mut version0, "version");
    fa.Get_S1(1, &mut version1, "high");

    let profile = (version1 << 1) | version0;

    if profile >= 3 {
        let mut version2: u8 = 0;
        fa.Get_S1(1, &mut version2, "RESERVED_ZERO");
        let profile_ext = (version2 << 2) | profile;
        if profile_ext > 3 {
            fa.BS_End();
            fa.Element_End();
            return false;
        }
    }

    let mut show_existing_frame: u8 = 0;
    fa.Get_S1(1, &mut show_existing_frame, "show_existing_frame");

    if show_existing_frame != 0 {
        fa.Skip_S1(3, "index_of_frame_to_show");
        fa.BS_End();
        fa.Element_End();
        return false;
    }

    let mut frame_type: u8 = 0;
    let mut show_frame: u8 = 0;
    let mut error_resilient_mode: u8 = 0;

    fa.Get_S1(1, &mut frame_type, "frame_type");
    fa.Get_S1(1, &mut show_frame, "show_frame");
    fa.Get_S1(1, &mut error_resilient_mode, "error_resilient_mode");

    let has_sync_code_color_refresh: u8;
    if frame_type == 0 {
        // I-Frame
        has_sync_code_color_refresh = 3;
    } else {
        let mut intra_only: u8 = 0;
        if show_frame != 0 {
            fa.Get_S1(1, &mut intra_only, "intra_only");
            if intra_only != 0 {
                has_sync_code_color_refresh = if profile > 0 { 7 } else { 5 };
            } else {
                has_sync_code_color_refresh = 0;
            }
            if error_resilient_mode == 0 {
                fa.Skip_S1(1, "reset_frame_context");
            }
        } else {
            has_sync_code_color_refresh = 0;
        }
    }

    let mut bit_depth: u8 = 8;
    let mut colorspace: u8 = 0;
    let mut yuv_range_flag: u8 = 0;
    let mut subsampling: u8 = 3;
    let mut width_minus_one: u16 = 0;
    let mut height_minus_one: u16 = 0;
    let mut has_size = false;

    if has_sync_code_color_refresh != 0 {
        let mut sync_code: int32u = 0;
        fa.Get_S3(24, &mut sync_code, "SYNC_CODE");

        if sync_code != 0x498342 {
            fa.BS_End();
            fa.Element_End();
            return false;
        }

        if (has_sync_code_color_refresh & 2) != 0 {
            if profile > 1 {
                let mut bit_depth_flag: u8 = 0;
                fa.Get_S1(1, &mut bit_depth_flag, "bit_depth_flag");
                bit_depth = if bit_depth_flag != 0 { 12 } else { 10 };
            } else {
                bit_depth = 8;
            }

            let mut cs: u8 = 0;
            fa.Get_S1(3, &mut cs, "colorspace");
            colorspace = VP9_COLORSPACE_MAP[cs as usize];

            if colorspace != 0 {
                // not sRGB
                fa.Get_S1(1, &mut yuv_range_flag, "yuv_range_flag");
                match profile {
                    1 | 3 => {
                        let mut subsampling_x: u8 = 0;
                        let mut subsampling_y: u8 = 0;
                        fa.Get_S1(1, &mut subsampling_x, "subsampling_x");
                        fa.Get_S1(1, &mut subsampling_y, "subsampling_y");
                        subsampling = (subsampling_x << 1) + subsampling_y;
                        fa.Skip_S1(1, "reserved");
                    }
                    _ => {
                        subsampling = 3;
                    }
                }
            } else {
                fa.Skip_S1(1, "reserved");
            }
        } else {
            fa.Skip_S1(1, "reserved");
        }

        if (has_sync_code_color_refresh & 4) != 0 {
            fa.Skip_S1(8, "refresh_frame_flags");
        }

        if has_sync_code_color_refresh != 0 {
            fa.Element_Begin("frame_size");
            let mut w: u16 = 0;
            let mut h: u16 = 0;
            fa.Get_S2(16, &mut w, "width_minus_one");
            fa.Get_S2(16, &mut h, "height_minus_one");
            width_minus_one = w;
            height_minus_one = h;
            let mut has_scaling: u8 = 0;
            fa.Get_S1(1, &mut has_scaling, "has_scaling");
            if has_scaling != 0 {
                fa.Get_S2(16, &mut w, "render_width_minus_one");
                fa.Get_S2(16, &mut h, "render_height_minus_one");
                width_minus_one = w;
                height_minus_one = h;
            }
            has_size = true;
            fa.Element_End();
        }
    }

    fa.BS_End();
    fa.Element_End();

    // Fill streams
    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "VP9", false);

    if has_sync_code_color_refresh != 0 && (has_sync_code_color_refresh >> 1) != 0 {
        fa.Fill(StreamKind::Video, 0, "Format_Profile", profile.to_string(), false);
    }

    fa.Fill(StreamKind::Video, 0, "BitDepth", bit_depth.to_string(), false);

    if has_size {
        fa.Fill(StreamKind::Video, 0, "Width", (width_minus_one as u32 + 1).to_string(), false);
        fa.Fill(StreamKind::Video, 0, "Height", (height_minus_one as u32 + 1).to_string(), false);
    }

    fa.Fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);

    if colorspace > 0 && has_sync_code_color_refresh != 0 {
        let chroma_idx = VP9_CHROMA_SUBSAMPLING_OOB[subsampling.min(3) as usize];
        fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", VP9_CHROMA_SUBSAMPLING[chroma_idx as usize], false);
        fa.Fill(StreamKind::Video, 0, "colour_range", VP9_COLOR_RANGE[(yuv_range_flag as usize) & 1], false);
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "VP9", false);

    true
}

/// Parse VP9 codec configuration record from an MP4 container (codecprivate).
pub fn parse_vp9_codec_config(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < 8 {
        return false;
    }

    fa.Element_Begin("VPCodecConfigurationRecord");

    let mut profile: u8 = 0;
    let mut level: u8 = 0;
    let mut bit_depth: u8 = 0;
    let mut chroma_subsampling: u8 = 0;
    let mut video_full_range_flag: u8 = 0;
    let mut colour_primaries: u8 = 0;
    let mut transfer_characteristics: u8 = 0;
    let mut matrix_coefficients: u8 = 0;

    fa.Get_B1(&mut profile, "profile");
    fa.Get_B1(&mut level, "level");

    fa.BS_Begin();
    fa.Get_S1(4, &mut bit_depth, "bitDepth");
    fa.Get_S1(3, &mut chroma_subsampling, "chromaSubsampling");
    fa.Get_S1(1, &mut video_full_range_flag, "videoFullRangeFlag");
    fa.BS_End();

    fa.Get_B1(&mut colour_primaries, "colourPrimaries");
    fa.Get_B1(&mut transfer_characteristics, "transferCharacteristics");
    fa.Get_B1(&mut matrix_coefficients, "matrixCoefficients");

    let mut codec_init_data_size: u16 = 0;
    fa.Get_B2(&mut codec_init_data_size, "codecInitializationDataSize");
    fa.Skip_Hexa(codec_init_data_size as usize, "codecInitializationData");

    fa.Element_End();

    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "VP9", false);
    fa.Fill(StreamKind::Video, 0, "Format_Profile", profile.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Format_Level", format!("{:.1}", level as f64 / 10.0), false);
    fa.Fill(StreamKind::Video, 0, "BitDepth", bit_depth.to_string(), false);

    let oob_idx = VP9_CHROMA_SUBSAMPLING_OOB[(chroma_subsampling.min(3)) as usize];
    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", VP9_CHROMA_SUBSAMPLING[oob_idx as usize], false);
    fa.Fill(StreamKind::Video, 0, "colour_range", VP9_COLOR_RANGE[(video_full_range_flag as usize) & 1], false);

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "VP9", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vp9_iframe(profile: u8, bit_depth: u8, width: u16, height: u16) -> Vec<u8> {
        let mut buf = Vec::new();

        // Build the VP9 uncompressed header as a bitstream (MSB-first)
        let mut bits = BitVec::new();

        // FRAME_MARKER (2 bits) = 0b10
        bits.add(2, 0x02);
        // version0 (1 bit)
        bits.add(1, (profile & 1) as u32);
        // version1 (1 bit)
        bits.add(1, ((profile >> 1) & 1) as u32);
        if profile >= 3 {
            bits.add(1, 0); // version2 (reserved zero)
        }
        // show_existing_frame = 0
        bits.add(1, 0);
        // frame_type = 0 (key frame)
        bits.add(1, 0);
        // show_frame = 1
        bits.add(1, 1);
        // error_resilient_mode = 0
        bits.add(1, 0);
        // I-Frame: Has_SyncCode_Color_Refresh = 3 (sync + color)
        // SYNC_CODE (24 bits) = 0x498342
        bits.add(24, 0x498342);
        // bitdepth_colorspace_sampling: profile > 1 gets bit_depth_flag
        if profile > 1 {
            bits.add(1, if bit_depth >= 12 { 1 } else { 0 });
        }
        // colorspace (3 bits) = 0 (CS_UNKNOWN -> sRGB-like)
        bits.add(3, 0);
        // if colorspace==0, skip reserved
        bits.add(1, 0); // reserved
        // Refresh frame flags (8 bits) — only when Has_SyncCode_Color_Refresh & 4
        // For profile 0: Has_SyncCode_Color_Refresh=3, so 3&4=0, no refresh
        // For profile 2: Has_SyncCode_Color_Refresh=7, so 7&4=4, refresh present
        if profile >= 2 {
            bits.add(8, 0xFF);
        }
        // frame_size: width_minus_one (16 bits)
        bits.add(16, width.saturating_sub(1) as u32);
        // frame_size: height_minus_one (16 bits)
        bits.add(16, height.saturating_sub(1) as u32);
        // has_scaling = 0
        bits.add(1, 0);

        buf.extend_from_slice(&bits.bytes);
        buf
    }

    #[test]
    fn rejects_wrong_frame_marker() {
        let buf = vec![0u8; 4];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_vp9(&mut fa));
    }

    #[test]
    fn parses_vp9_key_frame() {
        let buf = make_vp9_iframe(0, 8, 1920, 1080);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_vp9(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()),
            Some("VP9")
        );
    }

    #[test]
    fn parses_vp9_dimensions() {
        let buf = make_vp9_iframe(0, 8, 3840, 2160);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_vp9(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()),
            Some("3840")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()),
            Some("2160")
        );
    }

    #[test]
    fn parses_vp9_codec_config() {
        // Minimal 9-byte VP9 codec config: profile=0, level=30 (3.0),
        // bit_depth=8, chroma=3 (4:2:0), full_range=0, primaries=2, transfer=1, matrix=6, size=0
        let mut buf = Vec::new();
        buf.push(0); // profile
        buf.push(41); // level = 4.1 (41)
        buf.push(0b0011_1000); // bitDepth=8(1000), chroma=3(011), fullRange=0(0) 
        buf.push(2); // colour primaries = BT.709
        buf.push(1); // transfer = BT.709
        buf.push(6); // matrix = BT.709
        buf.push(0); // codecInitializationDataSize (high byte)
        buf.push(0); // codecInitializationDataSize (low byte)
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_vp9_codec_config(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Format_Profile").map(|z| z.as_str()),
            Some("0")
        );
    }

    // Helper: simple bit vector builder for tests
    struct BitVec {
        bytes: Vec<u8>,
        bit_pos: usize,
    }

    impl BitVec {
        fn new() -> Self {
            BitVec { bytes: Vec::new(), bit_pos: 0 }
        }

        fn add(&mut self, n_bits: usize, value: u32) {
            for i in (0..n_bits).rev() {
                let bit = (value >> i) & 1;
                let byte_idx = self.bit_pos / 8;
                let bit_idx = 7 - (self.bit_pos % 8);
                if byte_idx >= self.bytes.len() {
                    self.bytes.push(0);
                }
                self.bytes[byte_idx] |= (bit as u8) << bit_idx;
                self.bit_pos += 1;
            }
        }
    }
}
