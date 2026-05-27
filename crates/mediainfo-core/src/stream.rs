//! Stream model — transliteration of MediaInfoLib's per-stream `Fill` /
//! `Retrieve` state.
//!
//! On the C++ side this is `MediaInfo_Internal::Stream` indexed by
//! `(stream_t, size_t)` (kind + position-within-kind) and stores parsed
//! fields as `Ztring`. Output formatters (`Inform`, XML, JSON) walk this
//! state to produce their results, so this is the canonical place every
//! parser writes into.

use std::collections::BTreeMap;
use zenlib::Ztring;

/// `stream_t` from `MediaInfo_Const.h`. Same discriminants so external
/// consumers binding through the C ABI later get matching values.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum StreamKind {
    General = 0,
    Video = 1,
    Audio = 2,
    Text = 3,
    Other = 4,
    Image = 5,
    Menu = 6,
}

impl StreamKind {
    pub fn name(self) -> &'static str {
        match self {
            StreamKind::General => "General",
            StreamKind::Video => "Video",
            StreamKind::Audio => "Audio",
            StreamKind::Text => "Text",
            StreamKind::Other => "Other",
            StreamKind::Image => "Image",
            StreamKind::Menu => "Menu",
        }
    }
}

/// One stream's fields. BTreeMap keeps iteration order stable, which the
/// output formatters depend on for deterministic XML/JSON.
#[derive(Clone, Debug, Default)]
pub struct Stream {
    fields: BTreeMap<String, Ztring>,
    /// Insertion order, for formatters that want the order parsers wrote in.
    insertion_order: Vec<String>,
}

impl Stream {
    pub fn new() -> Self {
        Stream::default()
    }

    pub fn set(&mut self, parameter: &str, value: Ztring, replace: bool) {
        let key = parameter.to_owned();
        let existed = self.fields.contains_key(&key);
        if existed && !replace {
            return;
        }
        if !existed {
            self.insertion_order.push(key.clone());
        }
        self.fields.insert(key, value);
    }

    pub fn get(&self, parameter: &str) -> Option<&Ztring> {
        self.fields.get(parameter)
    }

    pub fn contains(&self, parameter: &str) -> bool {
        self.fields.contains_key(parameter)
    }

    pub fn count(&self) -> usize {
        self.fields.len()
    }

    /// Iterate fields in insertion order — matches the C++ behavior of
    /// emitting in the order parsers filled.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Ztring)> {
        self.insertion_order
            .iter()
            .filter_map(|k| self.fields.get_key_value(k.as_str()).map(|(k, v)| (k.as_str(), v)))
    }
}

/// Container of per-kind, indexed-by-position streams.
#[derive(Clone, Debug, Default)]
pub struct StreamCollection {
    by_kind: BTreeMap<StreamKind, Vec<Stream>>,
}

impl StreamCollection {
    pub fn new() -> Self {
        StreamCollection::default()
    }

    /// Allocate a new stream of `kind`, return its `StreamPos`.
    /// Matches `File__Analyze::Stream_Prepare`.
    pub fn Stream_Prepare(&mut self, kind: StreamKind) -> usize {
        let v = self.by_kind.entry(kind).or_default();
        v.push(Stream::new());
        v.len() - 1
    }

    pub fn Count_Get(&self, kind: StreamKind) -> usize {
        self.by_kind.get(&kind).map(|v| v.len()).unwrap_or(0)
    }

    /// `Fill(StreamKind, StreamPos, Parameter, Value, Replace)`. If the
    /// stream doesn't exist yet it is auto-created — matches the C++
    /// behavior where Fill at pos=0 implicitly prepares a stream.
    pub fn Fill(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
        replace: bool,
    ) {
        let v = self.by_kind.entry(kind).or_default();
        while v.len() <= pos {
            v.push(Stream::new());
        }
        v[pos].set(parameter, value.into(), replace);
    }

    pub fn Retrieve(&self, kind: StreamKind, pos: usize, parameter: &str) -> Option<&Ztring> {
        self.by_kind.get(&kind)?.get(pos)?.get(parameter)
    }

    pub fn stream(&self, kind: StreamKind, pos: usize) -> Option<&Stream> {
        self.by_kind.get(&kind)?.get(pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = (StreamKind, usize, &Stream)> {
        self.by_kind
            .iter()
            .flat_map(|(k, v)| v.iter().enumerate().map(move |(i, s)| (*k, i, s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_prepare_returns_sequential_indices() {
        let mut c = StreamCollection::new();
        assert_eq!(c.Stream_Prepare(StreamKind::Audio), 0);
        assert_eq!(c.Stream_Prepare(StreamKind::Audio), 1);
        assert_eq!(c.Stream_Prepare(StreamKind::Video), 0);
        assert_eq!(c.Count_Get(StreamKind::Audio), 2);
        assert_eq!(c.Count_Get(StreamKind::Video), 1);
        assert_eq!(c.Count_Get(StreamKind::Text), 0);
    }

    #[test]
    fn fill_and_retrieve_round_trip() {
        let mut c = StreamCollection::new();
        c.Stream_Prepare(StreamKind::Audio);
        c.Fill(StreamKind::Audio, 0, "Format", "FLAC", false);
        c.Fill(StreamKind::Audio, 0, "SamplingRate", "48000", false);
        assert_eq!(
            c.Retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str()),
            Some("FLAC")
        );
        assert_eq!(
            c.Retrieve(StreamKind::Audio, 0, "SamplingRate").map(|z| z.as_str()),
            Some("48000")
        );
        assert_eq!(c.Retrieve(StreamKind::Audio, 0, "Missing"), None);
    }

    #[test]
    fn fill_without_replace_keeps_first_value() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", "MP4", false);
        c.Fill(StreamKind::General, 0, "Format", "MOV", false);
        assert_eq!(
            c.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str()),
            Some("MP4")
        );
    }

    #[test]
    fn fill_with_replace_overwrites() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", "MP4", false);
        c.Fill(StreamKind::General, 0, "Format", "MOV", true);
        assert_eq!(
            c.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str()),
            Some("MOV")
        );
    }

    #[test]
    fn fill_auto_creates_stream_if_pos_unset() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::Audio, 2, "Format", "AAC", false);
        assert_eq!(c.Count_Get(StreamKind::Audio), 3);
        assert_eq!(
            c.Retrieve(StreamKind::Audio, 2, "Format").map(|z| z.as_str()),
            Some("AAC")
        );
        // The auto-created earlier streams are empty
        assert_eq!(c.Retrieve(StreamKind::Audio, 0, "Format"), None);
    }

    #[test]
    fn iter_preserves_insertion_order_within_stream() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::Video, 0, "Format", "AVC", false);
        c.Fill(StreamKind::Video, 0, "Width", "1920", false);
        c.Fill(StreamKind::Video, 0, "Height", "1080", false);
        let s = c.stream(StreamKind::Video, 0).unwrap();
        let order: Vec<&str> = s.iter().map(|(k, _)| k).collect();
        assert_eq!(order, vec!["Format", "Width", "Height"]);
    }

    #[test]
    fn iter_walks_all_streams_grouped_by_kind() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", "MP4", false);
        c.Fill(StreamKind::Video, 0, "Format", "AVC", false);
        c.Fill(StreamKind::Audio, 0, "Format", "AAC", false);
        c.Fill(StreamKind::Audio, 1, "Format", "AC3", false);
        let pairs: Vec<(StreamKind, usize)> = c.iter().map(|(k, i, _)| (k, i)).collect();
        assert_eq!(
            pairs,
            vec![
                (StreamKind::General, 0),
                (StreamKind::Video, 0),
                (StreamKind::Audio, 0),
                (StreamKind::Audio, 1),
            ]
        );
    }

    #[test]
    fn stream_kind_name_matches_cpp_output_strings() {
        assert_eq!(StreamKind::General.name(), "General");
        assert_eq!(StreamKind::Video.name(), "Video");
        assert_eq!(StreamKind::Audio.name(), "Audio");
        assert_eq!(StreamKind::Text.name(), "Text");
        assert_eq!(StreamKind::Other.name(), "Other");
        assert_eq!(StreamKind::Image.name(), "Image");
        assert_eq!(StreamKind::Menu.name(), "Menu");
    }
}
