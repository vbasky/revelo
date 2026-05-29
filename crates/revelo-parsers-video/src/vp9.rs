//! VP9 video parser.
//!
//! Mirrors MediaInfoLib's `File_Vp9.cpp`. Handles the VP9 uncompressed
//! header to extract profile, bit depth, chroma subsampling, width, height,
//! and color info from key frames.

use revelo_core::{FileAnalyze, Reader, StreamKind};

const VP9_COLORSPACE_MAP: [u8; 8] = [2, 5, 1, 6, 7, 9, 2, 0];

const VP9_CHROMA_SUBSAMPLING: [&str; 4] = ["", "4:4:0", "4:2:2", "4:2:0"];

const VP9_CHROMA_SUBSAMPLING_OOB: [u8; 4] = [3, 3, 2, 0];

const VP9_COLOR_RANGE: [&str; 2] = ["Limited", "Full"];

/// Fields decoded from the VP9 uncompressed frame header.
#[derive(Default)]
struct Vp9Header {
    profile: u8,
    bit_depth: u8,
    colorspace: u8,
    yuv_range_flag: u8,
    subsampling: u8,
    width_minus_one: u16,
    height_minus_one: u16,
    has_size: bool,
    has_sync_code_color_refresh: u8,
}

/// Parse VP9 elementary stream.
///
/// Detection: Frame marker + profile bits.
/// Fills: Profile, bit_depth, chroma_subsampling, color_space from vpcC.
pub fn parse_vp9(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < 6 {
        return None;
    }

    r.element_begin("VP9");
    let h = r.bits(|b| {
        let marker = b.read::<u8>(2, "FRAME_MARKER")?;
        if marker != 0x02 {
            return None;
        }
        let version0 = b.read::<u8>(1, "version")?;
        let version1 = b.read::<u8>(1, "high")?;
        let profile = (version1 << 1) | version0;
        if profile >= 3 {
            let version2 = b.read::<u8>(1, "RESERVED_ZERO")?;
            if ((version2 << 2) | profile) > 3 {
                return None;
            }
        }

        let show_existing_frame = b.read::<u8>(1, "show_existing_frame")?;
        if show_existing_frame != 0 {
            b.skip(3); // index_of_frame_to_show
            return None;
        }

        let frame_type = b.read::<u8>(1, "frame_type")?;
        let show_frame = b.read::<u8>(1, "show_frame")?;
        let error_resilient_mode = b.read::<u8>(1, "error_resilient_mode")?;

        let has_sync_code_color_refresh: u8 = if frame_type == 0 {
            3 // I-Frame
        } else if show_frame != 0 {
            let intra_only = b.read::<u8>(1, "intra_only")?;
            let v = if intra_only != 0 { if profile > 0 { 7 } else { 5 } } else { 0 };
            if error_resilient_mode == 0 {
                b.skip(1); // reset_frame_context
            }
            v
        } else {
            0
        };

        let mut h = Vp9Header {
            profile,
            bit_depth: 8,
            subsampling: 3,
            has_sync_code_color_refresh,
            ..Vp9Header::default()
        };

        if has_sync_code_color_refresh != 0 {
            if b.read::<u32>(24, "SYNC_CODE")? != 0x498342 {
                return None;
            }

            if (has_sync_code_color_refresh & 2) != 0 {
                if profile > 1 {
                    let bit_depth_flag = b.read::<u8>(1, "bit_depth_flag")?;
                    h.bit_depth = if bit_depth_flag != 0 { 12 } else { 10 };
                } else {
                    h.bit_depth = 8;
                }

                let cs = b.read::<u8>(3, "colorspace")?;
                h.colorspace = VP9_COLORSPACE_MAP[cs as usize];

                if h.colorspace != 0 {
                    // not sRGB
                    h.yuv_range_flag = b.read::<u8>(1, "yuv_range_flag")?;
                    match profile {
                        1 | 3 => {
                            let subsampling_x = b.read::<u8>(1, "subsampling_x")?;
                            let subsampling_y = b.read::<u8>(1, "subsampling_y")?;
                            h.subsampling = (subsampling_x << 1) + subsampling_y;
                            b.skip(1); // reserved
                        }
                        _ => h.subsampling = 3,
                    }
                } else {
                    b.skip(1); // reserved
                }
            } else {
                b.skip(1); // reserved
            }

            if (has_sync_code_color_refresh & 4) != 0 {
                b.skip(8); // refresh_frame_flags
            }

            h.width_minus_one = b.read::<u16>(16, "width_minus_one")?;
            h.height_minus_one = b.read::<u16>(16, "height_minus_one")?;
            if b.read::<u8>(1, "has_scaling")? != 0 {
                h.width_minus_one = b.read::<u16>(16, "render_width_minus_one")?;
                h.height_minus_one = b.read::<u16>(16, "render_height_minus_one")?;
            }
            h.has_size = true;
        }

        Some(h)
    })?;
    r.element_end();

    r.stream_prepare(StreamKind::Video);
    r.set_field(StreamKind::Video, 0, "Format", "VP9");
    r.set_field(StreamKind::Video, 0, "Format_Profile", h.profile.to_string());
    r.set_field(StreamKind::Video, 0, "BitDepth", h.bit_depth.to_string());

    if h.has_size {
        r.set_field(StreamKind::Video, 0, "Width", (h.width_minus_one as u32 + 1).to_string());
        r.set_field(StreamKind::Video, 0, "Height", (h.height_minus_one as u32 + 1).to_string());
    }

    r.set_field(StreamKind::Video, 0, "ColorSpace", "YUV");

    if h.colorspace > 0 && h.has_sync_code_color_refresh != 0 {
        let chroma_idx = VP9_CHROMA_SUBSAMPLING_OOB[h.subsampling.min(3) as usize];
        r.set_field(
            StreamKind::Video,
            0,
            "ChromaSubsampling",
            VP9_CHROMA_SUBSAMPLING[chroma_idx as usize],
        );
        r.set_field(
            StreamKind::Video,
            0,
            "colour_range",
            VP9_COLOR_RANGE[(h.yuv_range_flag as usize) & 1],
        );
    }

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "VP9");
    Some(())
}

/// Parse VP9 codec configuration record from an MP4 container (codecprivate).
pub fn parse_vp9_codec_config(fa: &mut FileAnalyze) -> bool {
    parse_codec_config(fa).is_some()
}

fn parse_codec_config(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < 8 {
        return None;
    }

    r.element_begin("VPCodecConfigurationRecord");

    let profile = r.be_u8("profile")?;
    let level = r.be_u8("level")?;

    let (bit_depth, chroma_subsampling, video_full_range_flag) = r.bits(|b| {
        let bit_depth = b.read::<u8>(4, "bitDepth")?;
        let chroma_subsampling = b.read::<u8>(3, "chromaSubsampling")?;
        let video_full_range_flag = b.read::<u8>(1, "videoFullRangeFlag")?;
        Some((bit_depth, chroma_subsampling, video_full_range_flag))
    })?;

    r.be_u8("colourPrimaries")?;
    r.be_u8("transferCharacteristics")?;
    r.be_u8("matrixCoefficients")?;

    let codec_init_data_size = r.be_u16("codecInitializationDataSize")?;
    r.skip(codec_init_data_size as usize)?;

    r.element_end();

    r.stream_prepare(StreamKind::Video);
    r.set_field(StreamKind::Video, 0, "Format", "VP9");
    r.set_field(StreamKind::Video, 0, "Format_Profile", profile.to_string());
    r.set_field(StreamKind::Video, 0, "Format_Level", format!("{:.1}", level as f64 / 10.0));
    r.set_field(StreamKind::Video, 0, "BitDepth", bit_depth.to_string());

    let oob_idx = VP9_CHROMA_SUBSAMPLING_OOB[(chroma_subsampling.min(3)) as usize];
    r.set_field(
        StreamKind::Video,
        0,
        "ChromaSubsampling",
        VP9_CHROMA_SUBSAMPLING[oob_idx as usize],
    );
    r.set_field(
        StreamKind::Video,
        0,
        "colour_range",
        VP9_COLOR_RANGE[(video_full_range_flag as usize) & 1],
    );

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "VP9");
    Some(())
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
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()), Some("VP9"));
    }

    #[test]
    fn parses_vp9_dimensions() {
        let buf = make_vp9_iframe(0, 8, 3840, 2160);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_vp9(&mut fa));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()), Some("3840"));
        assert_eq!(fa.retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()), Some("2160"));
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
            fa.retrieve(StreamKind::Video, 0, "Format_Profile").map(|z| z.as_str()),
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
