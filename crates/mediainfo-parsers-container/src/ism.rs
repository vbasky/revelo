//! IIS Smooth Streaming manifest (ISM) parser.
//!
//! Server-side ISM files are SMIL documents whose root element is `<smil>`
//! and contain a `<body><switch>...</switch></body>` hierarchy describing
//! the available video/audio/text streams. This mirrors the C++
//! `File_Ism::FileHeader_Begin` logic in MediaInfoLib, which uses tinyxml2
//! to walk `smil → body → switch` and then enumerates stream children.
//!
//! Client-side Smooth Streaming manifests (`.ismc`) use a
//! `<SmoothStreamingMedia ...>` root instead; for parity with the
//! Microsoft ecosystem we also accept that shape (optionally namespaced
//! as `<ism:SmoothStreamingMedia>`) since both extensions surface as
//! `ISM` in MediaInfo.
//!
//! Only the General stream's Format is populated. The C++ parser then
//! walks the SMIL `<switch>` children to enqueue referenced segment
//! files via `ReferenceFiles`; the Rust engine does not (yet) follow
//! external references, so we stop after identifying the container.

use mediainfo_core::{FileAnalyze, StreamKind};

const SCAN_WINDOW: usize = 1024;

pub fn parse_ism(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.Remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    // Only ASCII-compatible XML is supported here; UTF-16 prologs are rare
    // for manifests and the C++ FileHeader_Begin_XML path also expects
    // UTF-8 in practice for ISM.
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or(""),
    };

    let trimmed = text.trim_start();
    let after_prolog = if let Some(rest) = trimmed.strip_prefix("<?xml") {
        match rest.find("?>") {
            Some(end) => rest[end + 2..].trim_start(),
            None => return false,
        }
    } else {
        trimmed
    };

    // Skip an optional XML comment or doctype before the root.
    let after_prolog = skip_xml_noise(after_prolog);

    // Accept either the SMIL server manifest (`<smil>`) — which is what
    // the C++ parser uses — or the client Smooth Streaming manifest
    // (`<SmoothStreamingMedia>` / `<ism:SmoothStreamingMedia>`).
    let candidates: &[&str] = &[
        "<smil",
        "<SmoothStreamingMedia",
        "<ism:SmoothStreamingMedia",
    ];
    let rest = candidates
        .iter()
        .find_map(|tag| after_prolog.strip_prefix(*tag));
    let Some(rest) = rest else {
        return false;
    };

    // Next character must terminate the element name (whitespace, '>', '/').
    let next = rest.chars().next().unwrap_or('\0');
    if !next.is_whitespace() && next != '>' && next != '/' {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "ISM", true);
    true
}

fn skip_xml_noise(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("<!--") {
            match rest.find("-->") {
                Some(end) => s = &rest[end + 3..],
                None => return s,
            }
        } else if let Some(rest) = s.strip_prefix("<!") {
            match rest.find('>') {
                Some(end) => s = &rest[end + 1..],
                None => return s,
            }
        } else {
            return s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smil_server_manifest() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<smil xmlns="http://www.w3.org/2001/SMIL20/Language">
  <body>
    <switch>
      <video src="video.ismv" systemBitrate="1500000"/>
      <audio src="audio.isma" systemBitrate="64000"/>
    </switch>
  </body>
</smil>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_ism(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("ISM")
        );
    }

    #[test]
    fn parses_smooth_streaming_client_manifest() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<SmoothStreamingMedia MajorVersion="2" MinorVersion="0" Duration="6537916167"/>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_ism(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("ISM")
        );
    }

    #[test]
    fn rejects_xml_with_wrong_root_element() {
        let xml = br#"<?xml version="1.0"?><MPD xmlns="urn:mpeg:dash:schema:mpd:2011"></MPD>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_ism(&mut fa));
    }

    #[test]
    fn rejects_non_xml_buffer() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE");
        assert!(!parse_ism(&mut fa));
    }
}
