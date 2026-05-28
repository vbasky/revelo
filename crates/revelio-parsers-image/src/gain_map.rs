//! ISO 21496-1 Gain Map Metadata parser (AVIF `tmap` payload variant).
//!
//! Big-endian layout:
//!   0       u8   version (must be 0)
//!   1..3    u16  minimum_version (must be 0)
//!   3..5    u16  writer_version
//!   5       u8   flags: bit6 = use_base_colour_space, bit7 = is_multichannel
//!   6..10   u32  base_hdr_headroom_numerator
//!   10..14  u32  base_hdr_headroom_denominator
//!   14..18  u32  alternate_hdr_headroom_numerator
//!   18..22  u32  alternate_hdr_headroom_denominator
//!   then per channel (1 if !is_multichannel else 3), 40 bytes each:
//!     i32  gain_map_min_numerator
//!     u32  gain_map_min_denominator
//!     i32  gain_map_max_numerator
//!     u32  gain_map_max_denominator
//!     u32  gamma_numerator
//!     u32  gamma_denominator
//!     i32  base_offset_numerator
//!     u32  base_offset_denominator
//!     i32  alternate_offset_numerator
//!     u32  alternate_offset_denominator

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_gain_map(fa: &mut FileAnalyze) -> bool {
    struct Channel {
        gmin: Option<f32>,
        gmax: Option<f32>,
        gamma: Option<f32>,
        base_off: Option<f32>,
        alt_off: Option<f32>,
    }
    let minimum_version;
    let writer_version;
    let have_body;
    let use_base_colour_space;
    let is_multichannel;
    let base_hdr: Option<f32>;
    let alt_hdr: Option<f32>;
    let mut channels: Vec<Channel> = Vec::new();
    {
        let head = fa.peek_raw(fa.Remain().min(258));
        let Some(h) = head else { return false };
        if h.len() < 5 {
            return false;
        }
        if h[0] != 0 {
            return false;
        }
        minimum_version = u16::from_be_bytes([h[1], h[2]]);
        if minimum_version != 0 {
            return false;
        }
        writer_version = u16::from_be_bytes([h[3], h[4]]);

        if h.len() < 22 {
            have_body = false;
            use_base_colour_space = false;
            is_multichannel = false;
            base_hdr = None;
            alt_hdr = None;
        } else {
            have_body = true;
            let flags = h[5];
            use_base_colour_space = (flags & (1 << 6)) != 0;
            is_multichannel = (flags & (1 << 7)) != 0;
            let channel_count = if is_multichannel { 3 } else { 1 };

            let base_num = read_u32(h, 6);
            let base_den = read_u32(h, 10);
            let alt_num = read_u32(h, 14);
            let alt_den = read_u32(h, 18);
            base_hdr = (base_den != 0).then(|| base_num as f32 / base_den as f32);
            alt_hdr = (alt_den != 0).then(|| alt_num as f32 / alt_den as f32);

            let mut off = 22;
            for _ in 0..channel_count {
                if off + 40 > h.len() {
                    break;
                }
                let gmin_n = read_i32(h, off);
                let gmin_d = read_u32(h, off + 4);
                let gmax_n = read_i32(h, off + 8);
                let gmax_d = read_u32(h, off + 12);
                let gamma_n = read_u32(h, off + 16);
                let gamma_d = read_u32(h, off + 20);
                let boff_n = read_i32(h, off + 24);
                let boff_d = read_u32(h, off + 28);
                let aoff_n = read_i32(h, off + 32);
                let aoff_d = read_u32(h, off + 36);
                channels.push(Channel {
                    gmin: (gmin_d != 0).then(|| gmin_n as f32 / gmin_d as f32),
                    gmax: (gmax_d != 0).then(|| gmax_n as f32 / gmax_d as f32),
                    gamma: (gamma_d != 0).then(|| gamma_n as f32 / gamma_d as f32),
                    base_off: (boff_d != 0).then(|| boff_n as f32 / boff_d as f32),
                    alt_off: (aoff_d != 0).then(|| aoff_n as f32 / aoff_d as f32),
                });
                off += 40;
            }
        }
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "Gain Map", false);

    fa.Stream_Prepare(StreamKind::Image);
    fa.Fill(StreamKind::Image, 0, "Format", "Gain Map", false);
    fa.Fill(StreamKind::Image, 0, "Format_Version", minimum_version.to_string(), false);

    if !have_body {
        return true;
    }

    fa.Fill(StreamKind::Image, 0, "WriterVersion", writer_version.to_string(), false);
    fa.Fill(
        StreamKind::Image,
        0,
        "IsMultichannel",
        if is_multichannel { "Yes" } else { "No" },
        false,
    );
    fa.Fill(
        StreamKind::Image,
        0,
        "UseBaseColourSpace",
        if use_base_colour_space { "Yes" } else { "No" },
        false,
    );
    if let Some(v) = base_hdr {
        fa.Fill(StreamKind::Image, 0, "BaseHdrHeadroom", format!("{:.6}", v), false);
    }
    if let Some(v) = alt_hdr {
        fa.Fill(StreamKind::Image, 0, "AlternateHdrHeadroom", format!("{:.6}", v), false);
    }

    for (idx, ch) in channels.iter().enumerate() {
        let suffix = if is_multichannel {
            format!("_Channel{}", idx + 1)
        } else {
            String::new()
        };
        if let Some(v) = ch.gmin {
            fa.Fill(StreamKind::Image, 0, &format!("GainMapMin{}", suffix), format!("{:.6}", v), false);
        }
        if let Some(v) = ch.gmax {
            fa.Fill(StreamKind::Image, 0, &format!("GainMapMax{}", suffix), format!("{:.6}", v), false);
        }
        if let Some(v) = ch.gamma {
            fa.Fill(StreamKind::Image, 0, &format!("Gamma{}", suffix), format!("{:.6}", v), false);
        }
        if let Some(v) = ch.base_off {
            fa.Fill(StreamKind::Image, 0, &format!("BaseOffset{}", suffix), format!("{:.6}", v), false);
        }
        if let Some(v) = ch.alt_off {
            fa.Fill(StreamKind::Image, 0, &format!("AlternateOffset{}", suffix), format!("{:.6}", v), false);
        }
    }
    true
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}
fn read_i32(buf: &[u8], off: usize) -> i32 {
    i32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_header(writer_version: u16, flags: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0u8); // version
        buf.extend_from_slice(&0u16.to_be_bytes()); // minimum_version
        buf.extend_from_slice(&writer_version.to_be_bytes());
        buf.push(flags);
        // base_hdr_headroom 4.0 = 4/1
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(&1u32.to_be_bytes());
        // alternate_hdr_headroom 8.0 = 8/1
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf
    }

    fn add_channel(buf: &mut Vec<u8>, gmin: (i32, u32), gmax: (i32, u32), gamma: (u32, u32)) {
        buf.extend_from_slice(&gmin.0.to_be_bytes());
        buf.extend_from_slice(&gmin.1.to_be_bytes());
        buf.extend_from_slice(&gmax.0.to_be_bytes());
        buf.extend_from_slice(&gmax.1.to_be_bytes());
        buf.extend_from_slice(&gamma.0.to_be_bytes());
        buf.extend_from_slice(&gamma.1.to_be_bytes());
        // base_offset 0/64
        buf.extend_from_slice(&0i32.to_be_bytes());
        buf.extend_from_slice(&64u32.to_be_bytes());
        // alternate_offset 0/64
        buf.extend_from_slice(&0i32.to_be_bytes());
        buf.extend_from_slice(&64u32.to_be_bytes());
    }

    #[test]
    fn rejects_bad_version() {
        let mut buf = vec![1u8, 0, 0, 0, 0];
        buf.extend_from_slice(&[0u8; 64]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_gain_map(&mut fa));
    }

    #[test]
    fn rejects_bad_minimum_version() {
        let buf = vec![0u8, 0, 1, 0, 0];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_gain_map(&mut fa));
    }

    #[test]
    fn parses_single_channel() {
        let mut buf = build_header(1, 0);
        add_channel(&mut buf, (0, 64), (8, 1), (1, 1));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_gain_map(&mut fa));
        let i = |k: &str| fa.Retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("Gain Map"));
        assert_eq!(i("IsMultichannel").as_deref(), Some("No"));
        assert_eq!(i("UseBaseColourSpace").as_deref(), Some("No"));
        assert_eq!(i("BaseHdrHeadroom").as_deref(), Some("4.000000"));
        assert_eq!(i("AlternateHdrHeadroom").as_deref(), Some("8.000000"));
        assert_eq!(i("GainMapMax").as_deref(), Some("8.000000"));
        assert_eq!(i("Gamma").as_deref(), Some("1.000000"));
    }

    #[test]
    fn parses_multichannel() {
        let mut buf = build_header(2, 0b1100_0000); // both flags set
        add_channel(&mut buf, (0, 64), (4, 1), (1, 1));
        add_channel(&mut buf, (0, 64), (6, 1), (1, 1));
        add_channel(&mut buf, (0, 64), (8, 1), (1, 1));
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_gain_map(&mut fa));
        let i = |k: &str| fa.Retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Format").as_deref(), Some("Gain Map"));
        assert_eq!(i("IsMultichannel").as_deref(), Some("Yes"));
        assert_eq!(i("UseBaseColourSpace").as_deref(), Some("Yes"));
        assert_eq!(i("GainMapMax_Channel1").as_deref(), Some("4.000000"));
        assert_eq!(i("GainMapMax_Channel2").as_deref(), Some("6.000000"));
        assert_eq!(i("GainMapMax_Channel3").as_deref(), Some("8.000000"));
    }
}
