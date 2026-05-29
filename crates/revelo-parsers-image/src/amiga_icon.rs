//! Amiga Icon (Workbench `.info`) parser.
//!
//! All multi-byte fields are big-endian.
//!
//! DiskObject header (78 bytes):
//!   0..2    Magic       = 0xE310
//!   2..4    Version
//!   --- Gadget (44 bytes) ---
//!   4..8    ga_Next
//!   8..10   ga_LeftEdge
//!   10..12  ga_TopEdge
//!   12..14  ga_Width
//!   14..16  ga_Height
//!   16..18  ga_Flags
//!   18..20  ga_Activation
//!   20..22  ga_GadgetType
//!   22..26  ga_GadgetRender   (non-zero => Classic image follows)
//!   26..30  ga_SelectRender   (non-zero => second Classic image follows)
//!   30..34  ga_GadgetText
//!   34..38  ga_MutualExclude
//!   38..42  ga_SpecialInfo
//!   42..44  ga_GadgetID
//!   44..48  ga_UserData
//!   --- DiskObject continues ---
//!   48      do_Type (1=Disk, 2=Drawer, 3=Tool, 4=Project, 5=Garbage, 6=Kick, 8=AppIcon)
//!   49      do_Pad
//!   50..54  do_DefaultTool
//!   54..58  do_ToolTypes
//!   58..62  do_CurrentX
//!   62..66  do_CurrentY
//!   66..70  do_DrawerData
//!   70..74  do_ToolWindow
//!   74..78  do_StackSize
//!
//! After the header, optional sections follow in order:
//!   DrawerData (56 bytes, if do_DrawerData)
//!   Classic image header (20 bytes) + planar plane data, if ga_GadgetRender
//!     im_LeftEdge(2), im_TopEdge(2), im_Width(2), im_Height(2),
//!     im_Depth(2), im_ImageData(4), im_PlanePick(1), im_PlaneOnOff(1), im_Next(4)
//!   Classic image (selected), if ga_SelectRender
//!   DefaultTool text (u32 length + bytes)
//!   ToolTypes (u32 count + entries) — may contain "IM1=" NewIcon header
//!   ToolWindow text
//!   DrawerData2 (6 bytes)
//!   Optional FORM ICON IFF chunk (GlowIcon/ColorIcon, ARGB)

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_amiga_icon(fa: &mut FileAnalyze) -> bool {
    let avail = fa.remain();
    if avail < 4 {
        return false;
    }
    let head = match fa.peek_raw(avail.min(78)) {
        Some(b) => b,
        None => return false,
    };
    if head[0] != 0xE3 || head[1] != 0x10 {
        return false;
    }
    if head.len() < 78 {
        return false;
    }

    let ga_width = u16::from_be_bytes([head[12], head[13]]);
    let ga_height = u16::from_be_bytes([head[14], head[15]]);
    let gadget_render = u32::from_be_bytes([head[22], head[23], head[24], head[25]]);
    let select_render = u32::from_be_bytes([head[26], head[27], head[28], head[29]]);
    let user_data = u32::from_be_bytes([head[44], head[45], head[46], head[47]]);
    let icon_type = head[48];
    let has_default_tool = u32::from_be_bytes([head[50], head[51], head[52], head[53]]);
    let has_tool_types = u32::from_be_bytes([head[54], head[55], head[56], head[57]]);
    let has_drawer_data = u32::from_be_bytes([head[66], head[67], head[68], head[69]]);
    let has_tool_window = u32::from_be_bytes([head[70], head[71], head[72], head[73]]);

    let full = match fa.peek_raw(avail) {
        Some(b) => b,
        None => return false,
    };
    let mut off: usize = 78;

    if has_drawer_data != 0 && off + 56 <= full.len() {
        off += 56;
    }

    let mut classic: Option<(u16, u16, u16)> = None;
    if gadget_render != 0 && off + 20 <= full.len() {
        let img_width = u16::from_be_bytes([full[off + 4], full[off + 5]]);
        let img_height = u16::from_be_bytes([full[off + 6], full[off + 7]]);
        let img_depth = u16::from_be_bytes([full[off + 8], full[off + 9]]);
        off += 20;
        if img_width > 0 && img_height > 0 && img_depth > 0 && img_depth <= 8 {
            let plane_data_size =
                ((img_width as u64).div_ceil(16) * 2) * img_height as u64 * img_depth as u64;
            if plane_data_size <= (full.len() - off) as u64 {
                off += plane_data_size as usize;
            }
            classic = Some((img_width, img_height, img_depth));
        } else {
            classic = Some((img_width, img_height, img_depth));
        }
    }

    if select_render != 0 && off + 20 <= full.len() {
        let img_width = u16::from_be_bytes([full[off + 4], full[off + 5]]);
        let img_height = u16::from_be_bytes([full[off + 6], full[off + 7]]);
        let img_depth = u16::from_be_bytes([full[off + 8], full[off + 9]]);
        off += 20;
        if img_width > 0 && img_height > 0 && img_depth > 0 && img_depth <= 8 {
            let plane_data_size =
                ((img_width as u64).div_ceil(16) * 2) * img_height as u64 * img_depth as u64;
            if plane_data_size <= (full.len() - off) as u64 {
                off += plane_data_size as usize;
            }
        }
    }

    if has_default_tool != 0 && off + 4 <= full.len() {
        let length =
            u32::from_be_bytes([full[off], full[off + 1], full[off + 2], full[off + 3]]) as usize;
        off += 4;
        let take = length.min(full.len() - off);
        off += take;
    }

    let mut new_icon: Option<(u16, u16)> = None;
    if has_tool_types != 0 && off + 4 <= full.len() {
        let count_field =
            u32::from_be_bytes([full[off], full[off + 1], full[off + 2], full[off + 3]]);
        off += 4;
        if count_field >= 8 {
            let mut num_entries = count_field / 4 - 1;
            let max_entries = ((full.len() - off) / 4) as u32;
            if num_entries > max_entries {
                num_entries = max_entries;
            }
            for _ in 0..num_entries {
                if full.len() - off < 4 {
                    break;
                }
                let length =
                    u32::from_be_bytes([full[off], full[off + 1], full[off + 2], full[off + 3]])
                        as usize;
                off += 4;
                if new_icon.is_none()
                    && length >= 5
                    && full.len() - off >= 4
                    && &full[off..off + 4] == b"IM1="
                {
                    if length >= 9
                        && full.len() - off >= 9
                        && full[off + 5] >= 0x21
                        && full[off + 6] >= 0x21
                    {
                        let w = (full[off + 5] - 0x21) as u16;
                        let h = (full[off + 6] - 0x21) as u16;
                        new_icon = Some((w, h));
                    } else {
                        new_icon = Some((0, 0));
                    }
                }
                if length > full.len() - off {
                    break;
                }
                off += length;
            }
        }
    }

    if has_tool_window != 0 && off + 4 <= full.len() {
        let length =
            u32::from_be_bytes([full[off], full[off + 1], full[off + 2], full[off + 3]]) as usize;
        off += 4;
        let take = length.min(full.len() - off);
        off += take;
    }

    if has_drawer_data != 0 && (user_data & 0xFF) != 0 && off + 6 <= full.len() {
        off += 6;
    }

    let mut glow: Option<(u16, u16, u8, u8)> = None;
    let mut argb: Option<(u16, u16)> = None;
    if off + 12 <= full.len() {
        let mut search_pos = off;
        while search_pos + 12 <= full.len() {
            if &full[search_pos..search_pos + 4] == b"FORM"
                && &full[search_pos + 8..search_pos + 12] == b"ICON"
            {
                let mut p = search_pos + 4;
                let form_size =
                    u32::from_be_bytes([full[p], full[p + 1], full[p + 2], full[p + 3]]);
                p += 4;
                p += 4; // skip "ICON"

                let form_end_raw =
                    p as u64 + if form_size >= 4 { (form_size - 4) as u64 } else { 0 };
                let form_end = (form_end_raw as usize).min(full.len());

                let mut face_w: u16 = 0;
                let mut face_h: u16 = 0;
                let mut has_imag = false;
                let mut has_argb = false;
                let mut imag_depth: u8 = 0;
                let mut imag_format: u8 = 0;

                while p <= form_end && form_end - p >= 8 && full.len() - p >= 8 {
                    let chunk_name = &full[p..p + 4];
                    p += 4;
                    let chunk_size =
                        u32::from_be_bytes([full[p], full[p + 1], full[p + 2], full[p + 3]]);
                    p += 4;

                    if chunk_name == b"FACE" {
                        if chunk_size >= 4 && full.len() - p >= 4 {
                            face_w = full[p] as u16 + 1;
                            face_h = full[p + 1] as u16 + 1;
                        }
                    } else if chunk_name == b"IMAG" {
                        if !has_imag && chunk_size >= 10 && full.len() - p >= 6 {
                            imag_format = full[p + 3];
                            imag_depth = full[p + 5];
                            has_imag = true;
                        }
                    } else if chunk_name == b"ARGB" {
                        has_argb = true;
                    }

                    if chunk_size == 0 {
                        break;
                    }
                    let mut skip_size = chunk_size;
                    if skip_size % 2 != 0 {
                        if skip_size == 0xFFFFFFFF {
                            break;
                        }
                        skip_size += 1;
                    }
                    if (skip_size as usize) <= full.len() - p {
                        p += skip_size as usize;
                    } else {
                        break;
                    }
                }

                if has_imag && face_w > 0 && face_h > 0 {
                    glow = Some((face_w, face_h, imag_depth, imag_format));
                }
                if has_argb && face_w > 0 && face_h > 0 {
                    argb = Some((face_w, face_h));
                }
                break;
            }
            search_pos += 1;
        }
    }

    let _ = (ga_width, ga_height);

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "Amiga Icon");
    let profile = amiga_icon_type(icon_type);
    if !profile.is_empty() {
        fa.set_field(StreamKind::General, 0, "Format_Profile", profile);
    }

    if let Some((w, h, d)) = classic
        && gadget_render != 0
        && w > 0
        && h > 0
    {
        let pos = fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, pos, "Format", "Raw");
        fa.set_field(StreamKind::Image, pos, "Format_Profile", "Classic");
        fa.set_field(StreamKind::Image, pos, "ColorSpace", "RGB");
        fa.set_field(StreamKind::Image, pos, "Width", w.to_string());
        fa.set_field(StreamKind::Image, pos, "Height", h.to_string());
        fa.set_field(StreamKind::Image, pos, "BitDepth", d.to_string());
    }

    if let Some((w, h)) = new_icon
        && w > 0
        && h > 0
    {
        let pos = fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, pos, "Format", "Raw");
        fa.set_field(StreamKind::Image, pos, "Format_Profile", "NewIcon");
        fa.set_field(StreamKind::Image, pos, "ColorSpace", "RGB");
        fa.set_field(StreamKind::Image, pos, "Width", w.to_string());
        fa.set_field(StreamKind::Image, pos, "Height", h.to_string());
    }

    if let Some((w, h, depth, fmt)) = glow {
        let pos = fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, pos, "Format", if fmt == 1 { "RLE" } else { "Raw" });
        fa.set_field(StreamKind::Image, pos, "Format_Profile", "GlowIcon");
        fa.set_field(StreamKind::Image, pos, "ColorSpace", "RGB");
        fa.set_field(StreamKind::Image, pos, "Width", w.to_string());
        fa.set_field(StreamKind::Image, pos, "Height", h.to_string());
        fa.set_field(StreamKind::Image, pos, "BitDepth", depth.to_string());
    }

    if let Some((w, h)) = argb {
        let pos = fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, pos, "Format", "Raw");
        fa.set_field(StreamKind::Image, pos, "Format_Profile", "ARGB");
        fa.set_field(StreamKind::Image, pos, "ColorSpace", "RGBA");
        fa.set_field(StreamKind::Image, pos, "Width", w.to_string());
        fa.set_field(StreamKind::Image, pos, "Height", h.to_string());
        fa.set_field(StreamKind::Image, pos, "BitDepth", "32");
    }

    true
}

fn amiga_icon_type(t: u8) -> &'static str {
    match t {
        1 => "Disk",
        2 => "Drawer",
        3 => "Tool",
        4 => "Project",
        5 => "Garbage",
        6 => "Kick",
        8 => "AppIcon",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_header(
        icon_type: u8,
        gadget_render: u32,
        select_render: u32,
        ga_width: u16,
        ga_height: u16,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; 78];
        buf[0] = 0xE3;
        buf[1] = 0x10;
        buf[2] = 0x00; // Version
        buf[3] = 0x01;
        buf[12..14].copy_from_slice(&ga_width.to_be_bytes());
        buf[14..16].copy_from_slice(&ga_height.to_be_bytes());
        buf[22..26].copy_from_slice(&gadget_render.to_be_bytes());
        buf[26..30].copy_from_slice(&select_render.to_be_bytes());
        buf[48] = icon_type;
        buf
    }

    fn append_classic_image(buf: &mut Vec<u8>, w: u16, h: u16, depth: u16) {
        let start = buf.len();
        buf.resize(start + 20, 0);
        buf[start + 4..start + 6].copy_from_slice(&w.to_be_bytes());
        buf[start + 6..start + 8].copy_from_slice(&h.to_be_bytes());
        buf[start + 8..start + 10].copy_from_slice(&depth.to_be_bytes());
        let plane_data_size = ((w as usize).div_ceil(16) * 2) * h as usize * depth as usize;
        buf.resize(buf.len() + plane_data_size, 0);
    }

    #[test]
    fn rejects_non_amiga_icon() {
        let mut fa = FileAnalyze::new(b"NOT AN AMIGA ICON FILE AT ALL XYZ");
        assert!(!parse_amiga_icon(&mut fa));
    }

    #[test]
    fn parses_minimal_classic_icon() {
        let mut buf = build_header(3, 0x1000_0000, 0, 40, 20);
        append_classic_image(&mut buf, 40, 20, 2);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_amiga_icon(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("Amiga Icon"));
        assert_eq!(g("Format_Profile").as_deref(), Some("Tool"));
        assert_eq!(i("Format").as_deref(), Some("Raw"));
        assert_eq!(i("Format_Profile").as_deref(), Some("Classic"));
        assert_eq!(i("ColorSpace").as_deref(), Some("RGB"));
        assert_eq!(i("Width").as_deref(), Some("40"));
        assert_eq!(i("Height").as_deref(), Some("20"));
        assert_eq!(i("BitDepth").as_deref(), Some("2"));
    }

    #[test]
    fn parses_drawer_with_no_image_layers() {
        let buf = build_header(2, 0, 0, 0, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_amiga_icon(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("Amiga Icon"));
        assert_eq!(g("Format_Profile").as_deref(), Some("Drawer"));
        assert!(fa.retrieve(StreamKind::Image, 0, "Width").is_none());
    }

    #[test]
    fn rejects_truncated_header() {
        let mut fa = FileAnalyze::new(&[0xE3, 0x10, 0x00, 0x01, 0x00]);
        assert!(!parse_amiga_icon(&mut fa));
    }
}
