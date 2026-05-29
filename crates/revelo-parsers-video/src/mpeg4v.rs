//! MPEG-4 Visual (Part 2) video parser.
//!
//! Parses VOS and VOL start codes from elementary streams.

use revelo_core::{FileAnalyze, Reader, StreamKind};

/// Parse MPEG-4 Visual (DivX/Xvid) elementary stream.
///
/// Detection: Visual Object Sequence start 0x000001B0.
/// Fills: Profile, dimensions, VOL header.
pub fn parse_mpeg4v(fa: &mut FileAnalyze) -> bool {
    let r = &mut Reader::wrap(fa);
    let mut found = false;

    while r.remain() >= 4 {
        let Some(code) = r.peek_be_u32() else { break };
        if (code >> 8) != 0x000001 {
            break;
        }

        let start_code = (code & 0xFF) as u8;

        match start_code {
            0xB0 => {
                r.element_begin("video_object_start_code");
                r.be_u32("start code");
                if r.remain() >= 1 {
                    let profile = r.be_u8("profile_and_level_indication").unwrap_or(0);
                    r.element_end();

                    r.stream_prepare(StreamKind::Video);
                    r.set_field(StreamKind::Video, 0, "Format", "MPEG-4 Visual");
                    let profile_str = match profile {
                        0x01 => "Simple Profile",
                        0x02 => "Simple Scalable Profile",
                        0x03 => "Core Profile",
                        0x04 => "Main Profile",
                        0x05 => "N-bit Profile",
                        0x06 => "Scalable Texture Profile",
                        0x07 => "Simple Face Animation Profile",
                        0x08 => "Simple FBA Profile",
                        0x09 => "Basic Animated Texture Profile",
                        0x0A => "Hybrid Profile",
                        0x0B => "Advanced Real Time Simple Profile",
                        0x0C => "Core Scalable Profile",
                        0x0D => "Advanced Coding Efficiency Profile",
                        0x0E => "Advanced Core Profile",
                        0x0F => "Advanced Scalable Texture Profile",
                        _ => "",
                    };
                    r.set_field(StreamKind::Video, 0, "Format_Profile", profile_str);
                    found = true;
                }
            }
            0xB3 | 0x20..=0x2F => {
                r.element_begin("video_object_layer_start_code");
                r.be_u32("start code");

                if r.remain() < 9 {
                    r.element_end();
                    break;
                }

                r.be_u8("random_accessible_vol");
                r.be_u8("video_object_type_indication");
                let is_obj_layer = r.be_u8("is_object_layer_identifier").unwrap_or(0);

                let mut width: u16 = 0;
                let mut height: u16 = 0;

                if is_obj_layer != 0 && r.remain() >= 5 {
                    r.be_u8("video_object_layer_verid");
                    r.be_u8("video_object_layer_priority");
                    r.be_u8("aspect_ratio_info");

                    if r.remain() >= 1 {
                        let vol_ctrl = r.be_u8("vol_control_parameters").unwrap_or(0);

                        if vol_ctrl != 0 && r.remain() >= 1 {
                            r.be_u8("chroma_format");
                            r.be_u8("shape");
                        }
                        let shape = r.be_u8("video_object_layer_shape").unwrap_or(0);

                        if shape == 0 && r.remain() >= 8 {
                            r.be_u8("marker_bit");
                            r.be_u16("vop_time_increment_resolution");
                            let fixed_vop = r.be_u16("fixed_vop_rate").unwrap_or(0);

                            if fixed_vop != 0 && r.remain() >= 2 {
                                r.be_u16("fixed_vop_time_increment");
                            }

                            r.be_u8("marker_bit");
                            if r.remain() >= 2 {
                                let w_hi = r.be_u8("width_msb").unwrap_or(0);
                                let w_lo = r.be_u8("width_lsb").unwrap_or(0);
                                width = (((w_hi as u16) << 5) | ((w_lo as u16) >> 3)) & 0x1FFF;

                                r.be_u8("marker_bit");
                                if r.remain() >= 2 {
                                    let h_hi = r.be_u8("height_msb").unwrap_or(0);
                                    let h_lo = r.be_u8("height_lsb").unwrap_or(0);
                                    height = (((h_hi as u16) << 5) | ((h_lo as u16) >> 3)) & 0x1FFF;
                                }
                            }
                        }
                    }
                }

                r.element_end();

                r.stream_prepare(StreamKind::Video);
                r.set_field(StreamKind::Video, 0, "Format", "MPEG-4 Visual");
                if width > 0 {
                    r.set_field(StreamKind::Video, 0, "Width", width.to_string());
                }
                if height > 0 {
                    r.set_field(StreamKind::Video, 0, "Height", height.to_string());
                }
                found = true;
            }
            _ => {
                if start_code < 0xB0
                    || (start_code > 0xB7
                        && start_code != 0xB3
                        && !(0x20..=0x2F).contains(&start_code))
                {
                    break;
                }
                r.be_u32("start code");
            }
        }
    }
    found
}
