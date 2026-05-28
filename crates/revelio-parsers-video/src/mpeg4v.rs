//! MPEG-4 Visual (Part 2) video parser.
//!
//! Parses VOS and VOL start codes from elementary streams.

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_mpeg4v(fa: &mut FileAnalyze) -> bool {
    let mut found = false;

    while fa.remain() >= 4 {
        let mut code: u32 = 0;
        fa.peek_b4(&mut code);
        if (code >> 8) != 0x000001 { break; }

        let start_code = (code & 0xFF) as u8;

        match start_code {
            0xB0 => {
                fa.element_begin("video_object_start_code");
                fa.skip_b4("start code");
                if fa.remain() >= 1 {
                    let mut profile: u8 = 0;
                    fa.get_b1(&mut profile, "profile_and_level_indication");
                    fa.element_end();

                    fa.stream_prepare(StreamKind::Video);
                    fa.fill(StreamKind::Video, 0, "Format", "MPEG-4 Visual", false);
                    let profile_str = match profile {
                        0x01 => "Simple Profile", 0x02 => "Simple Scalable Profile",
                        0x03 => "Core Profile", 0x04 => "Main Profile", 0x05 => "N-bit Profile",
                        0x06 => "Scalable Texture Profile", 0x07 => "Simple Face Animation Profile",
                        0x08 => "Simple FBA Profile", 0x09 => "Basic Animated Texture Profile",
                        0x0A => "Hybrid Profile", 0x0B => "Advanced Real Time Simple Profile",
                        0x0C => "Core Scalable Profile", 0x0D => "Advanced Coding Efficiency Profile",
                        0x0E => "Advanced Core Profile", 0x0F => "Advanced Scalable Texture Profile",
                        _ => "",
                    };
                    fa.fill(StreamKind::Video, 0, "Format_Profile", profile_str, false);
                    found = true;
                }
            }
            0xB3 | 0x20..=0x2F => {
                fa.element_begin("video_object_layer_start_code");
                fa.skip_b4("start code");

                if fa.remain() < 9 {
                    fa.element_end();
                    break;
                }

                let mut random: u8 = 0;
                fa.get_b1(&mut random, "random_accessible_vol");
                let mut obj_type: u8 = 0;
                fa.get_b1(&mut obj_type, "video_object_type_indication");
                let mut is_obj_layer: u8 = 0;
                fa.get_b1(&mut is_obj_layer, "is_object_layer_identifier");

                let mut width: u16 = 0;
                let mut height: u16 = 0;

                if is_obj_layer != 0 && fa.remain() >= 5 {
                    fa.get_b1(&mut random, "video_object_layer_verid");
                    fa.get_b1(&mut random, "video_object_layer_priority");
                    let mut aspect: u8 = 0;
                    fa.get_b1(&mut aspect, "aspect_ratio_info");

                    if fa.remain() >= 1 {
                        let mut vol_ctrl: u8 = 0;
                        fa.get_b1(&mut vol_ctrl, "vol_control_parameters");

                        if vol_ctrl != 0 && fa.remain() >= 1 {
                            fa.get_b1(&mut random, "chroma_format");
                            fa.get_b1(&mut random, "shape");
                        }
                        let mut shape: u8 = 0;
                        fa.get_b1(&mut shape, "video_object_layer_shape");

                        if shape == 0 && fa.remain() >= 8 {
                            fa.get_b1(&mut random, "marker_bit");

                            let mut time_inc: u16 = 0;
                            fa.get_b2(&mut time_inc, "vop_time_increment_resolution");
                            let mut fixed_vop: u16 = 0;
                            fa.get_b2(&mut fixed_vop, "fixed_vop_rate");

                            if fixed_vop != 0 && fa.remain() >= 2 {
                                fa.get_b2(&mut fixed_vop, "fixed_vop_time_increment");
                            }

                            fa.get_b1(&mut random, "marker_bit");
                            if fa.remain() >= 2 {
                                let mut w_hi: u8 = 0;
                                let mut w_lo: u8 = 0;
                                fa.get_b1(&mut w_hi, "width_msb");
                                fa.get_b1(&mut w_lo, "width_lsb");
                                width = (((w_hi as u16) << 5) | ((w_lo as u16) >> 3)) & 0x1FFF;

                                fa.get_b1(&mut random, "marker_bit");
                                if fa.remain() >= 2 {
                                    let mut h_hi: u8 = 0;
                                    let mut h_lo: u8 = 0;
                                    fa.get_b1(&mut h_hi, "height_msb");
                                    fa.get_b1(&mut h_lo, "height_lsb");
                                    height = (((h_hi as u16) << 5) | ((h_lo as u16) >> 3)) & 0x1FFF;
                                }
                            }
                        }
                    }
                }

                fa.element_end();

                fa.stream_prepare(StreamKind::Video);
                fa.fill(StreamKind::Video, 0, "Format", "MPEG-4 Visual", false);
                if width > 0 { fa.fill(StreamKind::Video, 0, "Width", width.to_string(), false); }
                if height > 0 { fa.fill(StreamKind::Video, 0, "Height", height.to_string(), false); }
                found = true;
            }
            _ => {
                if start_code < 0xB0 || (start_code > 0xB7 && start_code != 0xB3 && (start_code < 0x20 || start_code > 0x2F)) {
                    break;
                }
                fa.skip_b4("start code");
            }
        }
    }
    found
}
