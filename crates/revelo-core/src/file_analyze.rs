//! Big-endian readers first; little-endian / floats / strings to follow.
//! Return-value style: each `Get_B*` consumes N bytes, returns the value,
//! and advances the position. If the read would overrun, `truncated()` returns
//! true and the returned value is zero.
//!
//! Parsers write results into the [`StreamCollection`] via [`set_field`](FileAnalyze::set_field)
//! or [`force_field`](FileAnalyze::force_field), and optionally record parser trace nodes
//! in the [`ElementTree`]. The [`StreamKind`] enum partitions fields by media type
//! (General, Video, Audio, etc.).

use crate::byte_source::ReadBackend;
use crate::config::MediaConfig;
use crate::element::ElementTree;
use crate::stream::{StreamCollection, StreamKind};
use revelo_util::Ztring;

/// Cursor-based byte reader over a [`ReadBackend`].
///
/// All parsers receive `&mut FileAnalyze` and read fields via the
/// `Get_B*`, `Get_L*`, `Peek_*`, and `Skip_*` methods. The cursor
/// advances automatically; truncated reads set a flag and return
/// zero / empty slices rather than panicking.
///
/// # Backend
///
/// Construct with [`FileAnalyze::new`] (accepts `impl Into<ReadBackend>`)
/// or [`FileAnalyze::from_slice`] (convenience for `&[u8]`).
///
/// | Constructor | Backend | Use |
/// |---|---|---|
/// | `FileAnalyze::from_slice(bytes)` | `ReadBackend::Slice` | In-memory buffer |
/// | `FileAnalyze::new(mmap)` | `ReadBackend::Mapped` | Memory-mapped file |
/// | (future) | `ReadBackend::Streamed` | `Read + Seek` handle |
pub struct FileAnalyze<'a> {
    backend: ReadBackend<'a>,
    element_offset: usize,
    truncated: bool,
    tree: ElementTree,
    streams: StreamCollection,
    /// When non-zero, bitstream mode is active and `Get_S*` reads consume
    /// from `backend[element_offset..]` starting `bs_bits_consumed` bits in.
    /// `BS_End` byte-aligns by advancing `element_offset` and clearing
    /// `bs_bits_consumed`.
    bs_active: bool,
    bs_bits_consumed: usize,
    /// When false, `Get_*` methods skip recording entries on the trace
    /// tree — mirrors the C++ `Trace_Activated` flag.
    pub trace_activated: bool,
    pub config: MediaConfig,
    /// Loaded buffer from multi-file concatenation. When set, multi-file
    /// companion content (BDMV M2TS, companion SRT/SST) was appended.
    pub multi_file_data: Option<Vec<u8>>,
    pub reference_count: usize,
    pub duplicate_indices: Vec<(StreamKind, usize)>,
}

impl<'a> FileAnalyze<'a> {
    /// Create a `FileAnalyze` over an in-memory byte slice.
    ///
    /// Backward-compatible with all existing callers — accepts
    /// `&[u8]`, `&[u8; N]`, and `&Vec<u8>` via auto-deref.
    pub fn new(slice: &'a [u8]) -> Self {
        Self::from_backend(ReadBackend::Slice(slice))
    }

    /// Create a `FileAnalyze` from any [`ReadBackend`] (mmap, future
    /// streaming sources, etc.).
    pub fn from_backend(backend: ReadBackend<'a>) -> Self {
        FileAnalyze {
            backend,
            element_offset: 0,
            truncated: false,
            tree: ElementTree::new(),
            streams: StreamCollection::new(),
            bs_active: false,
            bs_bits_consumed: 0,
            trace_activated: true,
            config: MediaConfig::default(),
            multi_file_data: None,
            reference_count: 0,
            duplicate_indices: Vec::new(),
        }
    }

    /// Convenience: create a `FileAnalyze` from an in-memory byte slice.
    /// Equivalent to `FileAnalyze::new(slice)`.
    pub fn from_slice(slice: &'a [u8]) -> Self {
        Self::new(slice)
    }

    /// Returns a reference to the underlying [`ReadBackend`].
    pub fn backend(&self) -> &ReadBackend<'a> {
        &self.backend
    }

    /// View the complete backend as a `&[u8]` slice (zero-copy for
    /// both `Slice` and `Mapped` variants).
    #[inline]
    fn buf(&self) -> &[u8] {
        self.backend.as_slice()
    }

    pub fn tree(&self) -> &ElementTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut ElementTree {
        &mut self.tree
    }

    pub fn streams(&self) -> &StreamCollection {
        &self.streams
    }

    pub fn streams_mut(&mut self) -> &mut StreamCollection {
        &mut self.streams
    }

    pub fn stream_prepare(&mut self, kind: StreamKind) -> usize {
        self.streams.stream_prepare(kind)
    }

    /// Set a field on a stream. If the field already exists, it is NOT
    /// overwritten (first-write-wins). Use [`force_field`] to overwrite.
    pub fn set_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.streams.set_field(kind, pos, parameter, value);
    }

    /// Set a field on a stream, ALWAYS overwriting any existing value.
    pub fn force_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.streams.force_field(kind, pos, parameter, value);
    }

    /// Fill into the stream's `<extra>` bucket instead of the standard
    /// field list. Used for tag-style metadata (ID3v2 comments, EXIF
    /// sub-IFD camera params, Apple QuickTime keys with no oracle-side
    /// canonical name) that oracle groups under `<extra>...</extra>`.
    /// First-write-wins; use [`force_extra_field`] to overwrite.
    pub fn set_extra_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.streams.set_extra_field(kind, pos, parameter, value);
    }

    /// Like [`set_extra_field`], but ALWAYS overwrites.
    pub fn force_extra_field(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
    ) {
        self.streams.force_extra_field(kind, pos, parameter, value);
    }

    pub fn retrieve(&self, kind: StreamKind, pos: usize, parameter: &str) -> Option<&Ztring> {
        self.streams.retrieve(kind, pos, parameter)
    }

    pub fn stream_count(&self, kind: StreamKind) -> usize {
        self.streams.stream_count(kind)
    }

    pub fn element_begin(&mut self, name: &str) {
        self.tree.element_begin(name);
    }
    pub fn element_end(&mut self) {
        self.tree.element_end();
    }
    pub fn element_info(&mut self, value: impl Into<String>, measure: Option<&str>) {
        self.tree.element_info(value, measure);
    }
    pub fn element_name(&mut self, name: &str) {
        self.tree.element_name(name);
    }
    pub fn element_level(&self) -> usize {
        self.tree.element_level()
    }

    fn param<V: ToString>(&mut self, name: &str, value: V) {
        if self.trace_activated && !name.is_empty() {
            self.tree.param(name, value.to_string());
        }
    }

    pub fn element_offset(&self) -> usize {
        self.element_offset
    }

    pub fn element_size(&self) -> usize {
        self.buf().len()
    }

    /// Apply MediaConfig options to the parser instance.
    pub fn set_config(&mut self, config: MediaConfig) {
        self.trace_activated = config.trace_activated;
        self.config = config;
    }

    /// Configure a parser option at runtime — mirrors MediaInfo::Option().
    /// Returns true if the option was recognized and applied.
    pub fn set_option(&mut self, key: &str, value: &str) -> bool {
        self.config.set_option(key, value)
    }

    pub fn remain(&self) -> usize {
        self.buf().len().saturating_sub(self.element_offset)
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    fn read_be_u64(&mut self, n: usize) -> Option<u64> {
        if self.remain() < n {
            self.truncated = true;
            self.element_offset = self.buf().len();
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v = (v << 8) | self.buf()[self.element_offset + i] as u64;
        }
        self.element_offset += n;
        Some(v)
    }

    fn peek_be_u64(&self, n: usize) -> Option<u64> {
        if self.remain() < n {
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v = (v << 8) | self.buf()[self.element_offset + i] as u64;
        }
        Some(v)
    }

    // ----------------------------------------------------------------------
    // Big-endian — Get_B*
    // ----------------------------------------------------------------------

    pub fn get_b1(&mut self, name: &str) -> u8 {
        let v = self.read_be_u64(1).unwrap_or(0) as u8;
        self.param(name, v);
        v
    }
    pub fn get_b2(&mut self, name: &str) -> u16 {
        let v = self.read_be_u64(2).unwrap_or(0) as u16;
        self.param(name, v);
        v
    }
    pub fn get_b3(&mut self, name: &str) -> u32 {
        let v = self.read_be_u64(3).unwrap_or(0) as u32;
        self.param(name, v);
        v
    }
    pub fn get_b4(&mut self, name: &str) -> u32 {
        let v = self.read_be_u64(4).unwrap_or(0) as u32;
        self.param(name, v);
        v
    }
    pub fn get_b5(&mut self, name: &str) -> u64 {
        let v = self.read_be_u64(5).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_b6(&mut self, name: &str) -> u64 {
        let v = self.read_be_u64(6).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_b7(&mut self, name: &str) -> u64 {
        let v = self.read_be_u64(7).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_b8(&mut self, name: &str) -> u64 {
        let v = self.read_be_u64(8).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_b16(&mut self, name: &str) -> u128 {
        if self.remain() < 16 {
            self.truncated = true;
            self.element_offset = self.buf().len();
            self.param(name, 0u128);
            return 0;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v = (v << 8) | self.buf()[self.element_offset + i] as u128;
        }
        self.element_offset += 16;
        self.param(name, v);
        v
    }

    // ----------------------------------------------------------------------
    // Big-endian — Peek_B*
    // ----------------------------------------------------------------------

    pub fn peek_b1(&self) -> u8 {
        self.peek_be_u64(1).unwrap_or(0) as u8
    }
    pub fn peek_b2(&self) -> u16 {
        self.peek_be_u64(2).unwrap_or(0) as u16
    }
    pub fn peek_b3(&self) -> u32 {
        self.peek_be_u64(3).unwrap_or(0) as u32
    }
    pub fn peek_b4(&self) -> u32 {
        self.peek_be_u64(4).unwrap_or(0) as u32
    }
    pub fn peek_b5(&self) -> u64 {
        self.peek_be_u64(5).unwrap_or(0)
    }
    pub fn peek_b6(&self) -> u64 {
        self.peek_be_u64(6).unwrap_or(0)
    }
    pub fn peek_b7(&self) -> u64 {
        self.peek_be_u64(7).unwrap_or(0)
    }
    pub fn peek_b8(&self) -> u64 {
        self.peek_be_u64(8).unwrap_or(0)
    }
    pub fn peek_b16(&self) -> u128 {
        if self.remain() < 16 {
            return 0;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v = (v << 8) | self.buf()[self.element_offset + i] as u128;
        }
        v
    }

    // ----------------------------------------------------------------------
    // Big-endian — Skip_B*
    // ----------------------------------------------------------------------

    pub fn skip_b1(&mut self, _name: &str) {
        self.skip(1)
    }
    pub fn skip_b2(&mut self, _name: &str) {
        self.skip(2)
    }
    pub fn skip_b3(&mut self, _name: &str) {
        self.skip(3)
    }
    pub fn skip_b4(&mut self, _name: &str) {
        self.skip(4)
    }
    pub fn skip_b5(&mut self, _name: &str) {
        self.skip(5)
    }
    pub fn skip_b6(&mut self, _name: &str) {
        self.skip(6)
    }
    pub fn skip_b7(&mut self, _name: &str) {
        self.skip(7)
    }
    pub fn skip_b8(&mut self, _name: &str) {
        self.skip(8)
    }
    pub fn skip_b16(&mut self, _name: &str) {
        self.skip(16)
    }

    pub fn skip_hexa(&mut self, bytes: usize, _name: &str) {
        self.skip(bytes);
    }

    /// Read `n` raw bytes from the current position, advancing the
    /// cursor. Returns an empty slice on underrun (and marks truncated).
    /// Used by parsers reading variable-length payloads like the
    /// VORBIS_COMMENT vendor string.
    pub fn read_raw(&mut self, n: usize) -> &[u8] {
        if self.remain() < n {
            self.truncated = true;
            self.element_offset = self.buf().len();
            return &[];
        }
        let start = self.element_offset;
        self.element_offset += n;
        &self.buf()[start..start + n]
    }

    /// Non-advancing magic check — peek N bytes and compare against `expected`.
    /// Returns `true` if the next `expected.len()` bytes match exactly,
    /// `false` if insufficient bytes remain or they don't match.
    pub fn peek_magic<const N: usize>(&self, expected: &[u8; N]) -> bool {
        if self.remain() < N {
            return false;
        }
        &self.buf()[self.element_offset..self.element_offset + N] == expected.as_slice()
    }

    /// Non-advancing variant of `read_raw`. Returns `None` if fewer than
    /// `n` bytes are available.
    pub fn peek_raw(&self, n: usize) -> Option<&[u8]> {
        if self.remain() < n {
            return None;
        }
        Some(&self.buf()[self.element_offset..self.element_offset + n])
    }

    /// Read up to `n` bytes starting at absolute file offset `at`, ignoring
    /// the cursor. Returns the available slice (possibly shorter than `n`
    /// if it runs past EOF), or `None` if `at` is out of bounds. Used to
    /// reach into mdat sample data at offsets recorded from stco/co64.
    pub fn peek_raw_at(&self, at: usize, n: usize) -> Option<&[u8]> {
        if at >= self.buf().len() {
            return None;
        }
        let end = (at + n).min(self.buf().len());
        Some(&self.buf()[at..end])
    }

    fn skip(&mut self, n: usize) {
        if self.remain() < n {
            self.truncated = true;
            self.element_offset = self.buf().len();
        } else {
            self.element_offset += n;
        }
    }

    // ----------------------------------------------------------------------
    // Little-endian — Get_L*
    // ----------------------------------------------------------------------

    fn read_le_u64(&mut self, n: usize) -> Option<u64> {
        if self.remain() < n {
            self.truncated = true;
            self.element_offset = self.buf().len();
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v |= (self.buf()[self.element_offset + i] as u64) << (8 * i);
        }
        self.element_offset += n;
        Some(v)
    }

    fn peek_le_u64(&self, n: usize) -> Option<u64> {
        if self.remain() < n {
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v |= (self.buf()[self.element_offset + i] as u64) << (8 * i);
        }
        Some(v)
    }

    pub fn get_l1(&mut self, name: &str) -> u8 {
        let v = self.read_le_u64(1).unwrap_or(0) as u8;
        self.param(name, v);
        v
    }
    pub fn get_l2(&mut self, name: &str) -> u16 {
        let v = self.read_le_u64(2).unwrap_or(0) as u16;
        self.param(name, v);
        v
    }
    pub fn get_l3(&mut self, name: &str) -> u32 {
        let v = self.read_le_u64(3).unwrap_or(0) as u32;
        self.param(name, v);
        v
    }
    pub fn get_l4(&mut self, name: &str) -> u32 {
        let v = self.read_le_u64(4).unwrap_or(0) as u32;
        self.param(name, v);
        v
    }
    pub fn get_l5(&mut self, name: &str) -> u64 {
        let v = self.read_le_u64(5).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_l6(&mut self, name: &str) -> u64 {
        let v = self.read_le_u64(6).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_l7(&mut self, name: &str) -> u64 {
        let v = self.read_le_u64(7).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_l8(&mut self, name: &str) -> u64 {
        let v = self.read_le_u64(8).unwrap_or(0);
        self.param(name, v);
        v
    }
    pub fn get_l16(&mut self, name: &str) -> u128 {
        if self.remain() < 16 {
            self.truncated = true;
            self.element_offset = self.buf().len();
            self.param(name, 0u128);
            return 0;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v |= (self.buf()[self.element_offset + i] as u128) << (8 * i);
        }
        self.element_offset += 16;
        self.param(name, v);
        v
    }

    pub fn peek_l1(&self) -> u8 {
        self.peek_le_u64(1).unwrap_or(0) as u8
    }
    pub fn peek_l2(&self) -> u16 {
        self.peek_le_u64(2).unwrap_or(0) as u16
    }
    pub fn peek_l3(&self) -> u32 {
        self.peek_le_u64(3).unwrap_or(0) as u32
    }
    pub fn peek_l4(&self) -> u32 {
        self.peek_le_u64(4).unwrap_or(0) as u32
    }
    pub fn peek_l5(&self) -> u64 {
        self.peek_le_u64(5).unwrap_or(0)
    }
    pub fn peek_l6(&self) -> u64 {
        self.peek_le_u64(6).unwrap_or(0)
    }
    pub fn peek_l7(&self) -> u64 {
        self.peek_le_u64(7).unwrap_or(0)
    }
    pub fn peek_l8(&self) -> u64 {
        self.peek_le_u64(8).unwrap_or(0)
    }
    pub fn peek_l16(&self) -> u128 {
        if self.remain() < 16 {
            return 0;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v |= (self.buf()[self.element_offset + i] as u128) << (8 * i);
        }
        v
    }

    pub fn skip_l1(&mut self, _name: &str) {
        self.skip(1)
    }
    pub fn skip_l2(&mut self, _name: &str) {
        self.skip(2)
    }
    pub fn skip_l3(&mut self, _name: &str) {
        self.skip(3)
    }
    pub fn skip_l4(&mut self, _name: &str) {
        self.skip(4)
    }
    pub fn skip_l5(&mut self, _name: &str) {
        self.skip(5)
    }
    pub fn skip_l6(&mut self, _name: &str) {
        self.skip(6)
    }
    pub fn skip_l7(&mut self, _name: &str) {
        self.skip(7)
    }
    pub fn skip_l8(&mut self, _name: &str) {
        self.skip(8)
    }
    pub fn skip_l16(&mut self, _name: &str) {
        self.skip(16)
    }

    // ----------------------------------------------------------------------
    // Floats — BF* (big-endian), LF* (little-endian)
    // ----------------------------------------------------------------------

    pub fn get_bf4(&mut self, name: &str) -> f32 {
        let v = self.read_be_u64(4).map(|bits| f32::from_bits(bits as u32)).unwrap_or(0.0);
        self.param(name, v);
        v
    }
    pub fn get_bf8(&mut self, name: &str) -> f64 {
        let v = self.read_be_u64(8).map(f64::from_bits).unwrap_or(0.0);
        self.param(name, v);
        v
    }
    pub fn get_bf10(&mut self, name: &str) -> f64 {
        // 80-bit IEEE 754 extended precision, big-endian — used in AIFF.
        // Decode as a finite f64 approximation; matches the C++ side which
        // also narrows to f64 on storage.
        if self.remain() < 10 {
            self.truncated = true;
            self.element_offset = self.buf().len();
            self.param(name, 0.0);
            return 0.0;
        }
        let offset = self.element_offset;
        let mut raw = [0u8; 10];
        raw.copy_from_slice(&self.buf()[offset..offset + 10]);
        self.element_offset += 10;
        let v = decode_f80_be(&raw);
        self.param(name, v);
        v
    }

    pub fn get_lf4(&mut self, name: &str) -> f32 {
        let v = self.read_le_u64(4).map(|bits| f32::from_bits(bits as u32)).unwrap_or(0.0);
        self.param(name, v);
        v
    }
    pub fn get_lf8(&mut self, name: &str) -> f64 {
        let v = self.read_le_u64(8).map(f64::from_bits).unwrap_or(0.0);
        self.param(name, v);
        v
    }

    pub fn peek_bf4(&self) -> f32 {
        self.peek_be_u64(4).map(|bits| f32::from_bits(bits as u32)).unwrap_or(0.0)
    }
    pub fn peek_bf8(&self) -> f64 {
        self.peek_be_u64(8).map(f64::from_bits).unwrap_or(0.0)
    }
    pub fn peek_lf4(&self) -> f32 {
        self.peek_le_u64(4).map(|bits| f32::from_bits(bits as u32)).unwrap_or(0.0)
    }
    pub fn peek_lf8(&self) -> f64 {
        self.peek_le_u64(8).map(f64::from_bits).unwrap_or(0.0)
    }

    pub fn skip_bf4(&mut self, _name: &str) {
        self.skip(4)
    }
    pub fn skip_bf8(&mut self, _name: &str) {
        self.skip(8)
    }
    pub fn skip_bf10(&mut self, _name: &str) {
        self.skip(10)
    }
    pub fn skip_lf4(&mut self, _name: &str) {
        self.skip(4)
    }
    pub fn skip_lf8(&mut self, _name: &str) {
        self.skip(8)
    }

    // ----------------------------------------------------------------------
    // Bitstream mode — BS_Begin / Get_S* / BS_End
    //
    // Mirrors the C++ pattern: callers issue `BS_Begin()`, then read
    // bits MSB-first via `Get_S*(N, &mut info, "Name")`, then
    // `BS_End()` to byte-align. Bit reads consume from
    // `buffer[element_offset..]` starting at `bs_bits_consumed` bits
    // past the byte boundary. `BS_End` advances `element_offset` to
    // the next byte boundary and clears the bit cursor.
    // ----------------------------------------------------------------------

    pub fn bs_begin(&mut self) {
        self.bs_active = true;
        self.bs_bits_consumed = 0;
    }

    pub fn bs_end(&mut self) {
        if self.bs_bits_consumed > 0 {
            self.element_offset += 1;
            self.bs_bits_consumed = 0;
        }
        self.bs_active = false;
    }

    /// Read `n` bits MSB-first from the bitstream cursor. Returns 0 on
    /// underrun and marks the buffer truncated. `n` must be <= 64.
    fn read_bits_be(&mut self, n: usize) -> u64 {
        debug_assert!(self.bs_active, "Get_S* called outside BS_Begin/BS_End");
        if n == 0 {
            return 0;
        }
        debug_assert!(n <= 64);

        // Bytes required from current byte to satisfy `n` bits.
        let bits_in_current_byte = 8 - self.bs_bits_consumed;
        let bytes_after_current =
            if n <= bits_in_current_byte { 0 } else { (n - bits_in_current_byte).div_ceil(8) };
        let bytes_needed = 1 + bytes_after_current;

        if self.element_offset + bytes_needed > self.buf().len() {
            self.truncated = true;
            self.element_offset = self.buf().len();
            self.bs_bits_consumed = 0;
            return 0;
        }

        let mut value: u64 = 0;
        let mut bits_left = n;
        let mut cursor_byte = self.element_offset;
        let mut bit_in_byte = self.bs_bits_consumed;

        while bits_left > 0 {
            let avail = 8 - bit_in_byte;
            let take = bits_left.min(avail);
            let shift_in_byte = avail - take;
            let chunk = (self.buf()[cursor_byte] >> shift_in_byte) as u64 & ((1u64 << take) - 1);
            value = (value << take) | chunk;
            bits_left -= take;
            bit_in_byte += take;
            if bit_in_byte == 8 {
                cursor_byte += 1;
                bit_in_byte = 0;
            }
        }

        self.element_offset = cursor_byte;
        self.bs_bits_consumed = bit_in_byte;
        value
    }

    pub fn get_s1(&mut self, n: usize, name: &str) -> u8 {
        let v = self.read_bits_be(n) as u8;
        self.param(name, v);
        v
    }
    pub fn get_s2(&mut self, n: usize, name: &str) -> u16 {
        let v = self.read_bits_be(n) as u16;
        self.param(name, v);
        v
    }
    pub fn get_s3(&mut self, n: usize, name: &str) -> u32 {
        let v = self.read_bits_be(n) as u32;
        self.param(name, v);
        v
    }
    pub fn get_s4(&mut self, n: usize, name: &str) -> u32 {
        let v = self.read_bits_be(n) as u32;
        self.param(name, v);
        v
    }
    pub fn get_s5(&mut self, n: usize, name: &str) -> u64 {
        let v = self.read_bits_be(n);
        self.param(name, v);
        v
    }
    pub fn get_s8(&mut self, n: usize, name: &str) -> u64 {
        let v = self.read_bits_be(n);
        self.param(name, v);
        v
    }

    pub fn skip_s1(&mut self, n: usize, _name: &str) {
        self.read_bits_be(n);
    }
    pub fn skip_s2(&mut self, n: usize, _name: &str) {
        self.read_bits_be(n);
    }
    pub fn skip_s3(&mut self, n: usize, _name: &str) {
        self.read_bits_be(n);
    }
    pub fn skip_s4(&mut self, n: usize, _name: &str) {
        self.read_bits_be(n);
    }
    pub fn skip_s5(&mut self, n: usize, _name: &str) {
        self.read_bits_be(n);
    }
    pub fn skip_s8(&mut self, n: usize, _name: &str) {
        self.read_bits_be(n);
    }

    // ----------------------------------------------------------------------
    // 4CC / Character codes (Get_C4 is used everywhere for MP4 atoms, RIFF)
    // ----------------------------------------------------------------------

    pub fn get_c4(&mut self, name: &str) -> u32 {
        // 4CCs are read as a big-endian u32 of 4 ASCII bytes. Display happens
        // via Ztring::From_CC4 elsewhere; for trace we render the printable
        // form when all bytes are ASCII printable, else fall back to u32.
        let v = self.read_be_u64(4).unwrap_or(0) as u32;
        if self.trace_activated && !name.is_empty() {
            let bytes = v.to_be_bytes();
            let printable = bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ');
            let value = if printable {
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                v.to_string()
            };
            self.tree.param(name, value);
        }
        v
    }

    pub fn peek_c4(&self) -> u32 {
        self.peek_b4()
    }

    pub fn skip_c4(&mut self, _name: &str) {
        self.skip(4)
    }
}

/// Decode a 10-byte big-endian IEEE 754 extended precision (80-bit)
/// floating point value into f64.
///
/// Format (Apple SANE / AIFF):
///   sign (1 bit) | exponent (15 bits) | integer-bit (1) | fraction (63 bits)
/// Bias is 16383; the integer bit is explicit (unlike IEEE 754 binary32/64).
fn decode_f80_be(bytes: &[u8]) -> f64 {
    debug_assert_eq!(bytes.len(), 10);
    let sign = (bytes[0] >> 7) & 1;
    let exp = (((bytes[0] & 0x7F) as u16) << 8) | bytes[1] as u16;
    let mut mant: u64 = 0;
    for i in 0..8 {
        mant = (mant << 8) | bytes[2 + i] as u64;
    }
    if exp == 0 && mant == 0 {
        return if sign == 1 { -0.0 } else { 0.0 };
    }
    if exp == 0x7FFF {
        return if mant == 0 {
            if sign == 1 { f64::NEG_INFINITY } else { f64::INFINITY }
        } else {
            f64::NAN
        };
    }
    // Reconstruct value from explicit integer bit + 63 fraction bits.
    // value = mant / 2^63 * 2^(exp - 16383)
    let scaled = (mant as f64) / (1u64 << 63) as f64;
    let result = scaled * 2f64.powi(exp as i32 - 16383);
    if sign == 1 { -result } else { result }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_b1_through_b8_read_big_endian() {
        let buf = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        let mut fa = FileAnalyze::new(&buf);
        let a = fa.get_b1("a");
        let b = fa.get_b2("b");
        let c = fa.get_b4("c");
        assert_eq!(a, 0x12);
        assert_eq!(b, 0x3456);
        assert_eq!(c, 0x789A_BCDE);
        assert_eq!(fa.element_offset(), 7);
        assert_eq!(fa.remain(), 1);
    }

    #[test]
    fn get_b3_5_6_7_widths() {
        let buf = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];
        let mut fa = FileAnalyze::new(&buf);
        let a = fa.get_b3("a");
        let b = fa.get_b5("b");
        assert_eq!(a, 0x00_0102_03);
        assert_eq!(b, 0x00_0405_0607_08);
    }

    #[test]
    fn get_b16_reads_128_bits() {
        let buf: Vec<u8> = (0..16u8).collect();
        let mut fa = FileAnalyze::new(&buf);
        let v = fa.get_b16("guid");
        assert_eq!(v, 0x0001_0203_0405_0607_0809_0A0B_0C0D_0E0F);
    }

    #[test]
    fn truncation_pins_position_and_sets_flag() {
        let buf = [0xAA, 0xBB];
        let mut fa = FileAnalyze::new(&buf);
        let v = fa.get_b4("v");
        assert_eq!(v, 0);
        assert!(fa.truncated());
        assert_eq!(fa.element_offset(), 2);
    }

    #[test]
    fn peek_does_not_advance() {
        let buf = [0x11, 0x22, 0x33, 0x44];
        let mut fa = FileAnalyze::new(&buf);
        assert_eq!(fa.peek_b4(), 0x1122_3344);
        assert_eq!(fa.element_offset(), 0);
        let w = fa.get_b4("w");
        assert_eq!(w, 0x1122_3344);
        assert_eq!(fa.element_offset(), 4);
    }

    #[test]
    fn skip_advances() {
        let buf = [0; 16];
        let mut fa = FileAnalyze::new(&buf);
        fa.skip_b4("padding");
        fa.skip_hexa(8, "header");
        assert_eq!(fa.element_offset(), 12);
        assert_eq!(fa.remain(), 4);
    }

    #[test]
    fn cc4_reads_mp4_atom_name_as_u32() {
        // "ftyp" atom — F=0x66 t=0x74 y=0x79 p=0x70
        let buf = [b'f', b't', b'y', b'p'];
        let mut fa = FileAnalyze::new(&buf);
        let code = fa.get_c4("Type");
        assert_eq!(code, 0x6674_7970);
    }

    #[test]
    fn get_l1_through_l8_read_little_endian() {
        // Same bytes as the BE test; expect bytes reversed in numeric value.
        let buf = [0x12, 0x34, 0x56, 0x78];
        let mut fa = FileAnalyze::new(&buf);
        let a = fa.get_l1("a");
        let b = fa.get_l2("b");
        assert_eq!(a, 0x12);
        assert_eq!(b, 0x5634);

        let buf2 = [0x12, 0x34, 0x56, 0x78];
        let mut fa2 = FileAnalyze::new(&buf2);
        let c = fa2.get_l4("c");
        assert_eq!(c, 0x7856_3412);
    }

    #[test]
    fn get_l16_reads_uuid_little_endian() {
        // First 8 bytes form low 64 bits, last 8 bytes form high 64 bits.
        let buf: [u8; 16] = [
            0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33,
            0x22, 0x11,
        ];
        let mut fa = FileAnalyze::new(&buf);
        let v = fa.get_l16("uuid");
        assert_eq!(v, 0x1122_3344_5566_7788_1234_5678_90AB_CDEF);
    }

    #[test]
    fn get_bf4_reads_be_float32() {
        let v = std::f32::consts::PI;
        let buf = v.to_be_bytes();
        let mut fa = FileAnalyze::new(&buf);
        let out = fa.get_bf4("pi");
        assert_eq!(out, v);
    }

    #[test]
    fn get_bf8_reads_be_float64() {
        let v = std::f64::consts::E;
        let buf = v.to_be_bytes();
        let mut fa = FileAnalyze::new(&buf);
        let out = fa.get_bf8("e");
        assert_eq!(out, v);
    }

    #[test]
    fn get_lf4_lf8_read_le_floats() {
        let f4 = 1.5_f32;
        let f8 = std::f64::consts::PI;
        let mut buf = Vec::new();
        buf.extend_from_slice(&f4.to_le_bytes());
        buf.extend_from_slice(&f8.to_le_bytes());
        let mut fa = FileAnalyze::new(&buf);
        let a = fa.get_lf4("a");
        let b = fa.get_lf8("b");
        assert_eq!(a, f4);
        assert_eq!(b, f8);
    }

    #[test]
    fn get_bf10_aiff_sample_rate_44100() {
        // 44100.0 Hz encoded as IEEE 754 extended precision, big-endian.
        // Sign=0, exp=16383+15=16398=0x400E, integer-bit=1, frac=44100<<32 == 0xAC44_0000_0000_0000
        // First two bytes: 0x40 0x0E; then 0xAC, 0x44, six zero bytes.
        let buf: [u8; 10] = [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0];
        let mut fa = FileAnalyze::new(&buf);
        let hz = fa.get_bf10("SampleRate");
        assert!((hz - 44100.0).abs() < 1e-9, "got {hz}");
    }

    #[test]
    fn get_bf10_zero() {
        let buf = [0u8; 10];
        let mut fa = FileAnalyze::new(&buf);
        let v = fa.get_bf10("zero");
        assert_eq!(v, 0.0);
    }

    #[test]
    fn get_b4_records_field_on_current_element() {
        let buf = [0xDE, 0xAD, 0xBE, 0xEF];
        let mut fa = FileAnalyze::new(&buf);
        fa.element_begin("header");
        let v = fa.get_b4("Magic");
        assert_eq!(v, 0xDEAD_BEEF);
        fa.element_end();

        let header = &fa.tree().root().children[0];
        assert_eq!(header.name, "header");
        assert_eq!(header.infos.len(), 1);
        assert_eq!(header.infos[0].name.as_deref(), Some("Magic"));
        // Decimal rendering of 0xDEADBEEF
        assert_eq!(header.infos[0].value, "3735928559");
    }

    #[test]
    fn get_c4_records_atom_name_as_printable_string() {
        let buf = [b'f', b't', b'y', b'p'];
        let mut fa = FileAnalyze::new(&buf);
        fa.element_begin("atom");
        let code = fa.get_c4("Type");
        assert_eq!(code, 0x6674_7970);
        fa.element_end();

        let atom = &fa.tree().root().children[0];
        assert_eq!(atom.infos.len(), 1);
        assert_eq!(atom.infos[0].name.as_deref(), Some("Type"));
        assert_eq!(atom.infos[0].value, "ftyp");
    }

    #[test]
    fn nested_atoms_build_correct_trace_tree() {
        // Simulate a tiny MP4-like structure:
        //   moov (size=24)
        //     mvhd (size=8) { Version=0, Flags=0x000001 }
        //     trak (size=0) { }
        let buf = [
            // moov children:
            // mvhd: version=0 (1 byte), flags=0x000001 (3 bytes)
            0x00, 0x00, 0x00, 0x01,
            // (rest unused for test purposes, just need bytes available)
            0, 0, 0, 0,
        ];
        let mut fa = FileAnalyze::new(&buf);
        fa.element_begin("moov");
        fa.element_begin("mvhd");
        let ver = fa.get_b1("Version");
        assert_eq!(ver, 0);
        let flags = fa.get_b3("Flags");
        assert_eq!(flags, 1);
        fa.element_end();
        fa.element_begin("trak");
        fa.element_end();
        fa.element_end();

        let moov = &fa.tree().root().children[0];
        assert_eq!(moov.name, "moov");
        assert_eq!(moov.children.len(), 2);

        let mvhd = &moov.children[0];
        assert_eq!(mvhd.name, "mvhd");
        assert_eq!(mvhd.infos.len(), 2);
        assert_eq!(mvhd.infos[0].name.as_deref(), Some("Version"));
        assert_eq!(mvhd.infos[0].value, "0");
        assert_eq!(mvhd.infos[1].name.as_deref(), Some("Flags"));
        assert_eq!(mvhd.infos[1].value, "1");

        let trak = &moov.children[1];
        assert_eq!(trak.name, "trak");
        assert!(trak.infos.is_empty());
    }

    #[test]
    fn trace_activated_false_suppresses_param_recording() {
        let buf = [0x12, 0x34, 0x56, 0x78];
        let mut fa = FileAnalyze::new(&buf);
        fa.trace_activated = false;
        fa.element_begin("silent");
        let v = fa.get_b4("Value");
        assert_eq!(v, 0x1234_5678);
        fa.element_end();
        assert!(fa.tree().root().children[0].infos.is_empty());
    }

    #[test]
    fn file_analyze_fills_streams_directly() {
        let buf = [0; 4];
        let mut fa = FileAnalyze::new(&buf);
        let pos = fa.stream_prepare(StreamKind::Audio);
        fa.set_field(StreamKind::Audio, pos, "Format", "FLAC");
        fa.set_field(StreamKind::Audio, pos, "BitDepth", "24");
        assert_eq!(fa.retrieve(StreamKind::Audio, pos, "Format").map(|z| z.as_str()), Some("FLAC"));
        assert_eq!(fa.stream_count(StreamKind::Audio), 1);
    }

    #[test]
    fn bs_get_s_reads_bit_aligned_fields() {
        // 0xAB 0xCD = 1010 1011 1100 1101
        // Read: 4 bits (1010 = 0xA), 4 bits (1011 = 0xB), 8 bits (1100 1101 = 0xCD)
        let buf = [0xAB, 0xCD];
        let mut fa = FileAnalyze::new(&buf);
        fa.bs_begin();
        let a = fa.get_s1(4, "a");
        let b = fa.get_s1(4, "b");
        let c = fa.get_s1(8, "c");
        fa.bs_end();
        assert_eq!(a, 0xA);
        assert_eq!(b, 0xB);
        assert_eq!(c, 0xCD);
        assert_eq!(fa.element_offset(), 2);
    }

    #[test]
    fn bs_streaminfo_layout_decoded_correctly() {
        // FLAC STREAMINFO packed field: 20 bits sample rate, 3 bits
        // channels-1, 5 bits bits_per_sample-1, 36 bits samples.
        // Encode: sample_rate=48000 (0x0BB80), channels-1=1, bits-1=15, samples=71638.
        //
        // bits: 00000000101110111000  001  01111  000000000000000000010001011110010110
        //                 0x0BB80      1   0x0F    0x0000_117_96 (71638)
        // Pack into 8 bytes (64 bits).
        let mut packed: u64 = 0;
        let sample_rate: u64 = 48000;
        let channels_m1: u64 = 1; // 2 channels
        let bps_m1: u64 = 15; // 16 bits
        let samples: u64 = 71638;
        packed |= sample_rate << (3 + 5 + 36);
        packed |= channels_m1 << (5 + 36);
        packed |= bps_m1 << 36;
        packed |= samples;
        let buf = packed.to_be_bytes();

        let mut fa = FileAnalyze::new(&buf);
        fa.bs_begin();
        let sr = fa.get_s3(20, "SampleRate");
        let ch = fa.get_s1(3, "Channels");
        let bps = fa.get_s1(5, "BitsPerSample");
        let samp = fa.get_s5(36, "Samples");
        fa.bs_end();
        assert_eq!(sr, 48000);
        assert_eq!(ch + 1, 2);
        assert_eq!(bps + 1, 16);
        assert_eq!(samp, 71638);
        assert_eq!(fa.element_offset(), 8);
    }

    #[test]
    fn bs_end_byte_aligns_when_partially_consumed() {
        let buf = [0xFF, 0x12];
        let mut fa = FileAnalyze::new(&buf);
        fa.bs_begin();
        let a = fa.get_s1(3, "a");
        assert_eq!(a, 0b111);
        fa.bs_end();
        // Aligned: should now be at byte index 1
        assert_eq!(fa.element_offset(), 1);
        let b = fa.get_b1("b");
        assert_eq!(b, 0x12);
    }

    #[test]
    fn bs_end_no_op_when_already_aligned() {
        let buf = [0xAA, 0xBB];
        let mut fa = FileAnalyze::new(&buf);
        fa.bs_begin();
        let a = fa.get_s1(8, "a");
        fa.bs_end();
        assert_eq!(a, 0xAA);
        assert_eq!(fa.element_offset(), 1);
    }

    #[test]
    fn empty_name_does_not_record_param() {
        let buf = [0xAA, 0xBB];
        let mut fa = FileAnalyze::new(&buf);
        fa.element_begin("e");
        let v = fa.get_b2("");
        assert_eq!(v, 0xAABB);
        fa.element_end();
        assert!(fa.tree().root().children[0].infos.is_empty());
    }
}
