//! Stream model — transliteration of MediaInfoLib's per-stream `Fill` /
//! `Retrieve` state.
//!
//! On the C++ side this is `MediaInfo_Internal::Stream` indexed by
//! `(stream_t, size_t)` (kind + position-within-kind) and stores parsed
//! fields as [`Ztring`]. Output formatters (in the `revelo-export` crate)
//! walk this state to produce XML, JSON, and text results, so this is the
//! canonical place every parser writes into under the direction of a
//! [`FileAnalyze`](super::FileAnalyze).

use std::collections::BTreeMap;
use revelo_util::Ztring;

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
    /// "Extra" fields — emitted in their own `<extra>...</extra>` block
    /// at the end of the stream, distinct from standard fields. Order
    /// preserved as inserted. Mirrors MediaInfoLib's
    /// `Stream::Extra` / `Fill_Measure` two-tier output model.
    extras: Vec<(String, Ztring)>,
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

    /// Append (or, if `replace`, overwrite) an `<extra>`-bucket entry.
    /// Extras don't appear in `iter()` — formatters call `extras_iter()`
    /// separately so they end up in their own XML/JSON block.
    pub fn set_extra(&mut self, parameter: &str, value: Ztring, replace: bool) {
        if replace {
            if let Some(slot) = self.extras.iter_mut().find(|(k, _)| k == parameter) {
                slot.1 = value;
                return;
            }
        } else if self.extras.iter().any(|(k, _)| k == parameter) {
            return;
        }
        self.extras.push((parameter.to_owned(), value));
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

    /// Iterate `<extra>`-bucket fields in insertion order.
    pub fn extras_iter(&self) -> impl Iterator<Item = (&str, &Ztring)> {
        self.extras.iter().map(|(k, v)| (k.as_str(), v))
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
    pub fn stream_prepare(&mut self, kind: StreamKind) -> usize {
        let v = self.by_kind.entry(kind).or_default();
        v.push(Stream::new());
        v.len() - 1
    }

    pub fn stream_count(&self, kind: StreamKind) -> usize {
        self.by_kind.get(&kind).map(|v| v.len()).unwrap_or(0)
    }

    /// Set a field on a stream. If the field already exists, it is NOT
    /// overwritten (first-write-wins). Auto-creates the stream if it
    /// doesn't exist yet.
    pub fn set_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.fill(kind, pos, parameter, value, false)
    }

    /// Set a field on a stream, ALWAYS overwriting any existing value.
    /// Auto-creates the stream if it doesn't exist yet.
    pub fn force_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.fill(kind, pos, parameter, value, true)
    }

    /// Inner helper: `Fill(StreamKind, StreamPos, Parameter, Value, Replace)`.
    fn fill(
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

    /// Set a field in the stream's `<extra>` bucket instead of the
    /// standard field list. First-write-wins.
    pub fn set_extra_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.fill_extra(kind, pos, parameter, value, false)
    }

    /// Like [`set_extra_field`], but ALWAYS overwrites.
    pub fn force_extra_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.fill_extra(kind, pos, parameter, value, true)
    }

    /// Inner helper for extra-bucket fields.
    fn fill_extra(
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
        v[pos].set_extra(parameter, value.into(), replace);
    }

    pub fn retrieve(&self, kind: StreamKind, pos: usize, parameter: &str) -> Option<&Ztring> {
        self.by_kind.get(&kind)?.get(pos)?.get(parameter)
    }

    pub fn stream(&self, kind: StreamKind, pos: usize) -> Option<&Stream> {
        self.by_kind.get(&kind)?.get(pos)
    }

    pub fn iter(&self) -> impl Iterator<Item = (StreamKind, usize, &Stream)> {
        self.by_kind.iter().flat_map(|(k, v)| v.iter().enumerate().map(move |(i, s)| (*k, i, s)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_prepare_returns_sequential_indices() {
        let mut c = StreamCollection::new();
        assert_eq!(c.stream_prepare(StreamKind::Audio), 0);
        assert_eq!(c.stream_prepare(StreamKind::Audio), 1);
        assert_eq!(c.stream_prepare(StreamKind::Video), 0);
        assert_eq!(c.stream_count(StreamKind::Audio), 2);
        assert_eq!(c.stream_count(StreamKind::Video), 1);
        assert_eq!(c.stream_count(StreamKind::Text), 0);
    }

    #[test]
    fn fill_and_retrieve_round_trip() {
        let mut c = StreamCollection::new();
        c.stream_prepare(StreamKind::Audio);
        c.set_field(StreamKind::Audio, 0, "Format", "FLAC");
        c.set_field(StreamKind::Audio, 0, "SamplingRate", "48000");
        assert_eq!(c.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str()), Some("FLAC"));
        assert_eq!(
            c.retrieve(StreamKind::Audio, 0, "SamplingRate").map(|z| z.as_str()),
            Some("48000")
        );
        assert_eq!(c.retrieve(StreamKind::Audio, 0, "Missing"), None);
    }

    #[test]
    fn set_field_keeps_first_value() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", "MP4");
        c.set_field(StreamKind::General, 0, "Format", "MOV");
        assert_eq!(c.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str()), Some("MP4"));
    }

    #[test]
    fn force_field_overwrites() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", "MP4");
        c.force_field(StreamKind::General, 0, "Format", "MOV");
        assert_eq!(c.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str()), Some("MOV"));
    }

    #[test]
    fn set_field_auto_creates_stream_if_pos_unset() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::Audio, 2, "Format", "AAC");
        assert_eq!(c.stream_count(StreamKind::Audio), 3);
        assert_eq!(c.retrieve(StreamKind::Audio, 2, "Format").map(|z| z.as_str()), Some("AAC"));
        // The auto-created earlier streams are empty
        assert_eq!(c.retrieve(StreamKind::Audio, 0, "Format"), None);
    }

    #[test]
    fn iter_preserves_insertion_order_within_stream() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::Video, 0, "Format", "AVC");
        c.set_field(StreamKind::Video, 0, "Width", "1920");
        c.set_field(StreamKind::Video, 0, "Height", "1080");
        let s = c.stream(StreamKind::Video, 0).unwrap();
        let order: Vec<&str> = s.iter().map(|(k, _)| k).collect();
        assert_eq!(order, vec!["Format", "Width", "Height"]);
    }

    #[test]
    fn iter_walks_all_streams_grouped_by_kind() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", "MP4");
        c.set_field(StreamKind::Video, 0, "Format", "AVC");
        c.set_field(StreamKind::Audio, 0, "Format", "AAC");
        c.set_field(StreamKind::Audio, 1, "Format", "AC3");
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
