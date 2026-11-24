//! RealMedia (RM/RMVB) container parser.
//!
//! RealMedia files begin with the ".RMF" four-byte signature followed by
//! big-endian chunks (PROP, CONT, MDPR, DATA, INDX). Each chunk is a FourCC,
//! a uint32 size that includes the 8-byte header, then the body. The body
//! starts with an object-version uint16.
//!
//! MDPR carries one media stream. Video type-specific data contains a `VIDO`
//! tag, a RealVideo FourCC, and big-endian width and height. Audio
//! type-specific data is a `.ra` header (`version` 4 or 5) whose FourCC
//! names the codec (`dnet` is AC-3).

use revelo_core::{FileAnalyze, StreamKind};

const SIGNATURE: &[u8; 4] = b".RMF";
/// Header walk stops here. MDPR/PROP sit in front of the DATA chunk.
const HEADER_LIMIT: usize = 1024 * 1024;

pub fn parse_rm(fa: &mut FileAnalyze) -> bool {
    let head = match fa.peek_raw(fa.remain().min(4)) {
        Some(b) if b.len() == 4 => b,
        _ => return false,
    };
    if head != SIGNATURE {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "RealMedia");

    // Peek before skipping so the chunk walk still sees the file from
    // offset 0. A short or truncated chunk stops the walk; Format stays set.
    let window = fa.remain().min(HEADER_LIMIT);
    let Some(buf) = fa.peek_raw(window) else {
        return true;
    };
    let info = walk_headers(buf);
    if let Some(duration) = info.duration_ms {
        fa.set_field(StreamKind::General, 0, "Duration", duration.to_string());
    }
    if let Some(title) = info.title.as_deref() {
        fa.set_field(StreamKind::General, 0, "Title", title);
    }
    if let Some(author) = info.author.as_deref() {
        fa.set_field(StreamKind::General, 0, "Performer", author);
    }
    if let Some(copyright) = info.copyright.as_deref() {
        fa.set_field(StreamKind::General, 0, "Copyright", copyright);
    }
    if let Some(comment) = info.comment.as_deref() {
        fa.set_field(StreamKind::General, 0, "Comment", comment);
    }

    let video_count = info.streams.iter().filter(|s| s.kind == StreamKind::Video).count();
    let audio_count = info.streams.iter().filter(|s| s.kind == StreamKind::Audio).count();
    if video_count > 0 {
        fa.set_field(StreamKind::General, 0, "VideoCount", video_count.to_string());
    }
    if audio_count > 0 {
        fa.set_field(StreamKind::General, 0, "AudioCount", audio_count.to_string());
    }
    fa.element_begin("RM");
    fa.skip_b4("Signature");
    fa.element_end();

    for stream in &info.streams {
        let pos = fa.stream_prepare(stream.kind);
        fa.set_field(stream.kind, pos, "Format", stream.format);
        if !stream.codec_id.is_empty() {
            fa.set_field(stream.kind, pos, "CodecID", stream.codec_id.as_str());
        }
        if stream.bit_rate > 0 {
            fa.set_field(stream.kind, pos, "BitRate", stream.bit_rate.to_string());
        }
        if stream.duration_ms > 0 {
            fa.set_field(stream.kind, pos, "Duration", stream.duration_ms.to_string());
        }
        if stream.kind == StreamKind::Video {
            if stream.width > 0 {
                fa.set_field(stream.kind, pos, "Width", stream.width.to_string());
            }
            if stream.height > 0 {
                fa.set_field(stream.kind, pos, "Height", stream.height.to_string());
            }
            if let Some(rate) = stream.frame_rate {
                fa.set_field(stream.kind, pos, "FrameRate", format!("{rate:.3}"));
            }
        }
        if stream.kind == StreamKind::Audio {
            if stream.channels > 0 {
                fa.set_field(stream.kind, pos, "Channels", stream.channels.to_string());
            }
            if stream.sample_rate > 0 {
                fa.set_field(stream.kind, pos, "SamplingRate", stream.sample_rate.to_string());
            }
        }
    }

    true
}

struct RmInfo {
    duration_ms: Option<u32>,
    title: Option<String>,
    author: Option<String>,
    copyright: Option<String>,
    comment: Option<String>,
    streams: Vec<RmStream>,
}

struct RmStream {
    kind: StreamKind,
    format: &'static str,
    codec_id: String,
    width: u32,
    height: u32,
    frame_rate: Option<f64>,
    sample_rate: u32,
    channels: u16,
    bit_rate: u32,
    duration_ms: u32,
}

fn walk_headers(buf: &[u8]) -> RmInfo {
    let mut info = RmInfo {
        duration_ms: None,
        title: None,
        author: None,
        copyright: None,
        comment: None,
        streams: Vec::new(),
    };
    let mut off = 0usize;
    while off + 8 <= buf.len() {
        let id = &buf[off..off + 4];
        let size = u32::from_be_bytes([buf[off + 4], buf[off + 5], buf[off + 6], buf[off + 7]]);
        if size < 8 {
            break;
        }
        if id == b"DATA" || id == b"INDX" {
            break;
        }
        let end = off + size as usize;
        if end > buf.len() {
            break;
        }
        let body = &buf[off + 8..end];
        match id {
            b"PROP" => {
                if let Some(duration) = prop_duration(body) {
                    info.duration_ms = Some(duration);
                }
            }
            b"CONT" => fill_cont(body, &mut info),
            b"MDPR" => {
                if let Some(stream) = parse_mdpr(body) {
                    info.streams.push(stream);
                }
            }
            _ => {}
        }
        off = end;
    }
    info
}

fn prop_duration(body: &[u8]) -> Option<u32> {
    // version, then max/avg bitrate, max/avg packet, packet count, duration.
    let mut r = Be::new(body);
    r.u16()?;
    r.skip(4 * 5)?;
    r.u32()
}

fn fill_cont(body: &[u8], info: &mut RmInfo) {
    let mut r = Be::new(body);
    if r.u16().is_none() {
        return;
    }
    let Ok(title) = read_str16(&mut r) else { return };
    let Ok(author) = read_str16(&mut r) else { return };
    let Ok(copyright) = read_str16(&mut r) else { return };
    let Ok(comment) = read_str16(&mut r) else { return };
    info.title = non_empty(title);
    info.author = non_empty(author);
    info.copyright = non_empty(copyright);
    info.comment = non_empty(comment);
}

fn read_str16(r: &mut Be<'_>) -> Result<String, ()> {
    r.str16().ok_or(())
}

fn non_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

fn parse_mdpr(body: &[u8]) -> Option<RmStream> {
    let mut r = Be::new(body);
    r.u16()?; // object version
    r.u16()?; // stream number
    r.u32()?; // max bit rate
    let avg_bit_rate = r.u32()?;
    r.u32()?; // max packet
    r.u32()?; // avg packet
    r.u32()?; // start
    r.u32()?; // preroll
    let duration_ms = r.u32()?;
    r.str8()?;
    let mime = r.str8()?;
    let type_len = r.u32()? as usize;
    let specific = r.bytes(type_len).unwrap_or(&[]);

    let mut stream = RmStream {
        kind: StreamKind::Audio,
        format: "RealAudio",
        codec_id: String::new(),
        width: 0,
        height: 0,
        frame_rate: None,
        sample_rate: 0,
        channels: 0,
        bit_rate: avg_bit_rate,
        duration_ms,
    };

    if mime.starts_with("video/") || specific.windows(4).any(|w| w == b"VIDO") {
        stream.kind = StreamKind::Video;
        stream.format = "RealVideo";
        if let Some(video) = parse_vido(specific) {
            stream.format = realvideo_format(&video.fourcc);
            stream.codec_id = video.fourcc;
            stream.width = video.width;
            stream.height = video.height;
            stream.frame_rate = video.frame_rate;
        }
    } else if let Some(audio) = parse_ra(specific) {
        stream.format = audio.format;
        stream.codec_id = audio.codec_id;
        stream.sample_rate = audio.sample_rate;
        stream.channels = audio.channels;
        if stream.bit_rate == 0 {
            stream.bit_rate = audio.bit_rate;
        }
    }
    Some(stream)
}

struct VideoSpecific {
    fourcc: String,
    width: u32,
    height: u32,
    frame_rate: Option<f64>,
}

fn parse_vido(data: &[u8]) -> Option<VideoSpecific> {
    let pos = data.windows(4).position(|w| w == b"VIDO")?;
    let mut r = Be::new(&data[pos + 4..]);
    let fourcc = String::from_utf8_lossy(r.bytes(4)?).into_owned();
    let width = u32::from(r.u16()?);
    let height = u32::from(r.u16()?);
    r.u16()?; // bits per sample
    r.u32()?; // padding
    let fps_fp = r.u32().unwrap_or(0);
    let frame_rate = if fps_fp > 0 { Some(f64::from(fps_fp) / 65536.0) } else { None };
    Some(VideoSpecific { fourcc, width, height, frame_rate })
}

fn realvideo_format(fourcc: &str) -> &'static str {
    match fourcc {
        "RV10" | "RV13" => "RealVideo 1",
        "RV20" => "RealVideo 2",
        "RV30" => "RealVideo 3",
        "RV40" => "RealVideo 4",
        _ => "RealVideo",
    }
}

struct AudioSpecific {
    format: &'static str,
    codec_id: String,
    sample_rate: u32,
    channels: u16,
    bit_rate: u32,
}

fn parse_ra(data: &[u8]) -> Option<AudioSpecific> {
    let pos = data.windows(4).position(|w| w == b".ra\xfd")?;
    let mut r = Be::new(&data[pos + 4..]);
    let version = r.u16()?;
    if version == 3 {
        return Some(AudioSpecific {
            format: "RealAudio",
            codec_id: "lpcJ".to_string(),
            sample_rate: 8000,
            channels: 1,
            bit_rate: 0,
        });
    }
    r.skip(2)?;
    if r.u32()? != u32::from_be_bytes(*b".ra4") {
        return None;
    }
    r.u32()?; // data size
    r.u16()?; // version2
    r.u32()?; // header size
    r.u16()?; // flavor
    r.u32()?; // coded frame size
    r.u32()?;
    let bytes_per_minute = r.u32()?;
    r.u32()?;
    r.u16()?; // sub_packet_h
    r.u16()?; // block align
    r.u16()?; // sub packet size
    r.u16()?;
    if version == 5 {
        r.skip(6)?;
    }
    let sample_rate = u32::from(r.u16()?);
    r.u32()?;
    let channels = r.u16()?;
    let codec_id = if version == 5 {
        r.skip(4)?; // deinterleave id
        String::from_utf8_lossy(r.bytes(4)?).into_owned()
    } else {
        r.str8()?; // deinterleave name
        r.str8()?
    };
    let bit_rate = if version == 4 && bytes_per_minute > 0 {
        bytes_per_minute.saturating_mul(8) / 60
    } else {
        0
    };
    Some(AudioSpecific {
        format: realaudio_format(&codec_id),
        codec_id,
        sample_rate,
        channels,
        bit_rate,
    })
}

fn realaudio_format(fourcc: &str) -> &'static str {
    match fourcc {
        "dnet" => "AC-3",
        "cook" => "Cook",
        "sipr" => "Sipro",
        "atrc" => "ATRAC3",
        "raac" | "racp" => "AAC",
        _ => "RealAudio",
    }
}

struct Be<'a> {
    data: &'a [u8],
    i: usize,
}

impl<'a> Be<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, i: 0 }
    }

    fn u16(&mut self) -> Option<u16> {
        let b = self.bytes(2)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let b = self.bytes(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.i + n > self.data.len() {
            return None;
        }
        let out = &self.data[self.i..self.i + n];
        self.i += n;
        Some(out)
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        self.bytes(n).map(|_| ())
    }

    fn str8(&mut self) -> Option<String> {
        let n = *self.bytes(1)?.first()? as usize;
        Some(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }

    fn str16(&mut self) -> Option<String> {
        let n = self.u16()? as usize;
        Some(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_rm_header() {
        let mut buf = Vec::from(*SIGNATURE);
        // Append a plausible trailing chunk header; the parser ignores it.
        buf.extend_from_slice(&[0u8; 16]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_rm(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("RealMedia"),
        );
    }

    #[test]
    fn rejects_non_rm_buffer() {
        let mut fa = FileAnalyze::new(b"NOT A REALMEDIA FILE");
        assert!(!parse_rm(&mut fa));
    }

    #[test]
    fn rejects_truncated_buffer() {
        // Fewer than 4 bytes — peek_raw can't read a full signature.
        let mut fa = FileAnalyze::new(b".RM");
        assert!(!parse_rm(&mut fa));
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(8 + body.len() as u32).to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    fn ra4_dnet(sample_rate: u16, channels: u16) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b".ra\xfd");
        b.extend_from_slice(&4u16.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(b".ra4");
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&4u16.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&sample_rate.to_be_bytes());
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&channels.to_be_bytes());
        b.push(4);
        b.extend_from_slice(b"Int0");
        b.push(4);
        b.extend_from_slice(b"dnet");
        b
    }

    #[test]
    fn parses_realvideo_and_ac3_streams() {
        let mut prop = Vec::new();
        prop.extend_from_slice(&0u16.to_be_bytes());
        for value in [0u32, 0, 0, 0, 1, 417, 0, 0, 0] {
            prop.extend_from_slice(&value.to_be_bytes());
        }
        prop.extend_from_slice(&2u16.to_be_bytes());
        prop.extend_from_slice(&0u16.to_be_bytes());

        let mut video = Vec::new();
        video.extend_from_slice(&0u16.to_be_bytes()); // version
        video.extend_from_slice(&0u16.to_be_bytes()); // stream
        video.extend_from_slice(&200_000u32.to_be_bytes());
        video.extend_from_slice(&200_000u32.to_be_bytes());
        video.extend_from_slice(&0u32.to_be_bytes());
        video.extend_from_slice(&0u32.to_be_bytes());
        video.extend_from_slice(&0u32.to_be_bytes());
        video.extend_from_slice(&0u32.to_be_bytes());
        video.extend_from_slice(&417u32.to_be_bytes());
        video.push(0); // name
        let mime = b"video/x-pn-realvideo";
        video.push(mime.len() as u8);
        video.extend_from_slice(mime);
        let mut specific = Vec::new();
        specific.extend_from_slice(&0u32.to_be_bytes());
        specific.extend_from_slice(b"VIDORV20");
        specific.extend_from_slice(&160u16.to_be_bytes());
        specific.extend_from_slice(&120u16.to_be_bytes());
        specific.extend_from_slice(&0u16.to_be_bytes());
        specific.extend_from_slice(&0u32.to_be_bytes());
        specific.extend_from_slice(&(23u32 * 65536).to_be_bytes());
        video.extend_from_slice(&(specific.len() as u32).to_be_bytes());
        video.extend_from_slice(&specific);

        let mut audio = Vec::new();
        audio.extend_from_slice(&0u16.to_be_bytes());
        audio.extend_from_slice(&1u16.to_be_bytes());
        audio.extend_from_slice(&96_000u32.to_be_bytes());
        audio.extend_from_slice(&96_000u32.to_be_bytes());
        audio.extend_from_slice(&0u32.to_be_bytes());
        audio.extend_from_slice(&0u32.to_be_bytes());
        audio.extend_from_slice(&0u32.to_be_bytes());
        audio.extend_from_slice(&0u32.to_be_bytes());
        audio.extend_from_slice(&417u32.to_be_bytes());
        audio.push(0);
        let amime = b"audio/x-pn-realaudio";
        audio.push(amime.len() as u8);
        audio.extend_from_slice(amime);
        let ra = ra4_dnet(48000, 1);
        audio.extend_from_slice(&(ra.len() as u32).to_be_bytes());
        audio.extend_from_slice(&ra);

        let mut buf = chunk(b".RMF", &[0u8; 10]);
        buf.extend(chunk(b"PROP", &prop));
        buf.extend(chunk(b"MDPR", &video));
        buf.extend(chunk(b"MDPR", &audio));

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_rm(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("RealMedia"));
        assert_eq!(g("Duration").as_deref(), Some("417"));
        assert_eq!(g("VideoCount").as_deref(), Some("1"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(v("Format").as_deref(), Some("RealVideo 2"));
        assert_eq!(v("CodecID").as_deref(), Some("RV20"));
        assert_eq!(v("Width").as_deref(), Some("160"));
        assert_eq!(v("Height").as_deref(), Some("120"));
        assert_eq!(v("FrameRate").as_deref(), Some("23.000"));
        assert_eq!(a("Format").as_deref(), Some("AC-3"));
        assert_eq!(a("CodecID").as_deref(), Some("dnet"));
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("BitRate").as_deref(), Some("96000"));
    }
}
