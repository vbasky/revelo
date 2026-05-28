//! Transliteration of MediaInfoLib's `File__Analyze` byte-reader surface.
//!
//! Big-endian readers first; little-endian / floats / strings to follow.
//! Out-parameter style from the C++ side is preserved as `&mut` arguments
//! so parser code reads identically:
//!
//! ```ignore
//! let mut size: int32u = 0;
//! fa.Get_B4(&mut size, "Size");
//! ```
//!
//! Each `Get_B*` consumes N bytes, sets the out-parameter to the value, and
//! advances the position. If the read would overrun, the position is
//! pinned at the end, the out-parameter is left zeroed, and `truncated()`
//! returns true — matching the C++ flag-and-continue semantics.

use crate::element::ElementTree;
use crate::stream::{StreamCollection, StreamKind};
use zenlib::{Ztring, float32, float64, float80, int128u, int16u, int32u, int64u, int8u};

pub struct FileAnalyze<'a> {
    buffer: &'a [u8],
    element_offset: usize,
    truncated: bool,
    tree: ElementTree,
    streams: StreamCollection,
    /// When non-zero, bitstream mode is active and `Get_S*` reads consume
    /// from `buffer[element_offset..]` starting `bs_bits_consumed` bits in.
    /// `BS_End` byte-aligns by advancing `element_offset` and clearing
    /// `bs_bits_consumed`.
    bs_active: bool,
    bs_bits_consumed: usize,
    /// When false, `Get_*` methods skip recording entries on the trace
    /// tree — mirrors the C++ `Trace_Activated` flag.
    pub trace_activated: bool,
}

impl<'a> FileAnalyze<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        FileAnalyze {
            buffer,
            element_offset: 0,
            truncated: false,
            tree: ElementTree::new(),
            streams: StreamCollection::new(),
            bs_active: false,
            bs_bits_consumed: 0,
            trace_activated: true,
        }
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

    pub fn Stream_Prepare(&mut self, kind: StreamKind) -> usize {
        self.streams.Stream_Prepare(kind)
    }

    pub fn Fill(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
        replace: bool,
    ) {
        self.streams.Fill(kind, pos, parameter, value, replace);
    }

    /// Fill into the stream's `<extra>` bucket instead of the standard
    /// field list. Used for tag-style metadata (ID3v2 comments, EXIF
    /// sub-IFD camera params, Apple QuickTime keys with no oracle-side
    /// canonical name) that oracle groups under `<extra>...</extra>`.
    pub fn Fill_Extra(
        &mut self,
        kind: StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<Ztring>,
        replace: bool,
    ) {
        self.streams.Fill_Extra(kind, pos, parameter, value, replace);
    }

    pub fn Retrieve(&self, kind: StreamKind, pos: usize, parameter: &str) -> Option<&Ztring> {
        self.streams.Retrieve(kind, pos, parameter)
    }

    pub fn Count_Get(&self, kind: StreamKind) -> usize {
        self.streams.Count_Get(kind)
    }

    pub fn Element_Begin(&mut self, name: &str) {
        self.tree.Element_Begin(name);
    }
    pub fn Element_End(&mut self) {
        self.tree.Element_End();
    }
    pub fn Element_Info(&mut self, value: impl Into<String>, measure: Option<&str>) {
        self.tree.Element_Info(value, measure);
    }
    pub fn Element_Name(&mut self, name: &str) {
        self.tree.Element_Name(name);
    }
    pub fn Element_Level(&self) -> usize {
        self.tree.Element_Level()
    }

    fn param<V: ToString>(&mut self, name: &str, value: V) {
        if self.trace_activated && !name.is_empty() {
            self.tree.Param(name, value.to_string());
        }
    }

    pub fn Element_Offset(&self) -> usize {
        self.element_offset
    }

    pub fn Element_Size(&self) -> usize {
        self.buffer.len()
    }

    pub fn Remain(&self) -> usize {
        self.buffer.len().saturating_sub(self.element_offset)
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    fn read_be_u64(&mut self, n: usize) -> Option<u64> {
        if self.Remain() < n {
            self.truncated = true;
            self.element_offset = self.buffer.len();
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v = (v << 8) | self.buffer[self.element_offset + i] as u64;
        }
        self.element_offset += n;
        Some(v)
    }

    fn peek_be_u64(&self, n: usize) -> Option<u64> {
        if self.Remain() < n {
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v = (v << 8) | self.buffer[self.element_offset + i] as u64;
        }
        Some(v)
    }

    // ----------------------------------------------------------------------
    // Big-endian — Get_B*
    // ----------------------------------------------------------------------

    pub fn Get_B1(&mut self, info: &mut int8u, name: &str) {
        *info = self.read_be_u64(1).unwrap_or(0) as int8u;
        self.param(name, *info);
    }
    pub fn Get_B2(&mut self, info: &mut int16u, name: &str) {
        *info = self.read_be_u64(2).unwrap_or(0) as int16u;
        self.param(name, *info);
    }
    pub fn Get_B3(&mut self, info: &mut int32u, name: &str) {
        *info = self.read_be_u64(3).unwrap_or(0) as int32u;
        self.param(name, *info);
    }
    pub fn Get_B4(&mut self, info: &mut int32u, name: &str) {
        *info = self.read_be_u64(4).unwrap_or(0) as int32u;
        self.param(name, *info);
    }
    pub fn Get_B5(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_be_u64(5).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_B6(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_be_u64(6).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_B7(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_be_u64(7).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_B8(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_be_u64(8).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_B16(&mut self, info: &mut int128u, name: &str) {
        if self.Remain() < 16 {
            *info = 0;
            self.truncated = true;
            self.element_offset = self.buffer.len();
            return;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v = (v << 8) | self.buffer[self.element_offset + i] as u128;
        }
        self.element_offset += 16;
        *info = v;
        self.param(name, *info);
    }

    // ----------------------------------------------------------------------
    // Big-endian — Peek_B*
    // ----------------------------------------------------------------------

    pub fn Peek_B1(&self, info: &mut int8u) {
        *info = self.peek_be_u64(1).unwrap_or(0) as int8u;
    }
    pub fn Peek_B2(&self, info: &mut int16u) {
        *info = self.peek_be_u64(2).unwrap_or(0) as int16u;
    }
    pub fn Peek_B3(&self, info: &mut int32u) {
        *info = self.peek_be_u64(3).unwrap_or(0) as int32u;
    }
    pub fn Peek_B4(&self, info: &mut int32u) {
        *info = self.peek_be_u64(4).unwrap_or(0) as int32u;
    }
    pub fn Peek_B5(&self, info: &mut int64u) {
        *info = self.peek_be_u64(5).unwrap_or(0);
    }
    pub fn Peek_B6(&self, info: &mut int64u) {
        *info = self.peek_be_u64(6).unwrap_or(0);
    }
    pub fn Peek_B7(&self, info: &mut int64u) {
        *info = self.peek_be_u64(7).unwrap_or(0);
    }
    pub fn Peek_B8(&self, info: &mut int64u) {
        *info = self.peek_be_u64(8).unwrap_or(0);
    }
    pub fn Peek_B16(&self, info: &mut int128u) {
        if self.Remain() < 16 {
            *info = 0;
            return;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v = (v << 8) | self.buffer[self.element_offset + i] as u128;
        }
        *info = v;
    }

    // ----------------------------------------------------------------------
    // Big-endian — Skip_B*
    // ----------------------------------------------------------------------

    pub fn Skip_B1(&mut self, _name: &str) { self.skip(1) }
    pub fn Skip_B2(&mut self, _name: &str) { self.skip(2) }
    pub fn Skip_B3(&mut self, _name: &str) { self.skip(3) }
    pub fn Skip_B4(&mut self, _name: &str) { self.skip(4) }
    pub fn Skip_B5(&mut self, _name: &str) { self.skip(5) }
    pub fn Skip_B6(&mut self, _name: &str) { self.skip(6) }
    pub fn Skip_B7(&mut self, _name: &str) { self.skip(7) }
    pub fn Skip_B8(&mut self, _name: &str) { self.skip(8) }
    pub fn Skip_B16(&mut self, _name: &str) { self.skip(16) }

    pub fn Skip_Hexa(&mut self, bytes: usize, _name: &str) {
        self.skip(bytes);
    }

    /// Read `n` raw bytes from the current position, advancing the
    /// cursor. Returns an empty slice on underrun (and marks truncated).
    /// Used by parsers reading variable-length payloads like the
    /// VORBIS_COMMENT vendor string.
    pub fn read_raw(&mut self, n: usize) -> &[u8] {
        if self.Remain() < n {
            self.truncated = true;
            self.element_offset = self.buffer.len();
            return &[];
        }
        let start = self.element_offset;
        self.element_offset += n;
        &self.buffer[start..start + n]
    }

    /// Non-advancing variant of `read_raw`. Returns `None` if fewer than
    /// `n` bytes are available.
    pub fn peek_raw(&self, n: usize) -> Option<&[u8]> {
        if self.Remain() < n {
            return None;
        }
        Some(&self.buffer[self.element_offset..self.element_offset + n])
    }

    fn skip(&mut self, n: usize) {
        if self.Remain() < n {
            self.truncated = true;
            self.element_offset = self.buffer.len();
        } else {
            self.element_offset += n;
        }
    }

    // ----------------------------------------------------------------------
    // Little-endian — Get_L*
    // ----------------------------------------------------------------------

    fn read_le_u64(&mut self, n: usize) -> Option<u64> {
        if self.Remain() < n {
            self.truncated = true;
            self.element_offset = self.buffer.len();
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v |= (self.buffer[self.element_offset + i] as u64) << (8 * i);
        }
        self.element_offset += n;
        Some(v)
    }

    fn peek_le_u64(&self, n: usize) -> Option<u64> {
        if self.Remain() < n {
            return None;
        }
        let mut v: u64 = 0;
        for i in 0..n {
            v |= (self.buffer[self.element_offset + i] as u64) << (8 * i);
        }
        Some(v)
    }

    pub fn Get_L1(&mut self, info: &mut int8u, name: &str) {
        *info = self.read_le_u64(1).unwrap_or(0) as int8u;
        self.param(name, *info);
    }
    pub fn Get_L2(&mut self, info: &mut int16u, name: &str) {
        *info = self.read_le_u64(2).unwrap_or(0) as int16u;
        self.param(name, *info);
    }
    pub fn Get_L3(&mut self, info: &mut int32u, name: &str) {
        *info = self.read_le_u64(3).unwrap_or(0) as int32u;
        self.param(name, *info);
    }
    pub fn Get_L4(&mut self, info: &mut int32u, name: &str) {
        *info = self.read_le_u64(4).unwrap_or(0) as int32u;
        self.param(name, *info);
    }
    pub fn Get_L5(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_le_u64(5).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_L6(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_le_u64(6).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_L7(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_le_u64(7).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_L8(&mut self, info: &mut int64u, name: &str) {
        *info = self.read_le_u64(8).unwrap_or(0);
        self.param(name, *info);
    }
    pub fn Get_L16(&mut self, info: &mut int128u, name: &str) {
        if self.Remain() < 16 {
            *info = 0;
            self.truncated = true;
            self.element_offset = self.buffer.len();
            return;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v |= (self.buffer[self.element_offset + i] as u128) << (8 * i);
        }
        self.element_offset += 16;
        *info = v;
        self.param(name, *info);
    }

    pub fn Peek_L1(&self, info: &mut int8u) {
        *info = self.peek_le_u64(1).unwrap_or(0) as int8u;
    }
    pub fn Peek_L2(&self, info: &mut int16u) {
        *info = self.peek_le_u64(2).unwrap_or(0) as int16u;
    }
    pub fn Peek_L3(&self, info: &mut int32u) {
        *info = self.peek_le_u64(3).unwrap_or(0) as int32u;
    }
    pub fn Peek_L4(&self, info: &mut int32u) {
        *info = self.peek_le_u64(4).unwrap_or(0) as int32u;
    }
    pub fn Peek_L5(&self, info: &mut int64u) {
        *info = self.peek_le_u64(5).unwrap_or(0);
    }
    pub fn Peek_L6(&self, info: &mut int64u) {
        *info = self.peek_le_u64(6).unwrap_or(0);
    }
    pub fn Peek_L7(&self, info: &mut int64u) {
        *info = self.peek_le_u64(7).unwrap_or(0);
    }
    pub fn Peek_L8(&self, info: &mut int64u) {
        *info = self.peek_le_u64(8).unwrap_or(0);
    }
    pub fn Peek_L16(&self, info: &mut int128u) {
        if self.Remain() < 16 {
            *info = 0;
            return;
        }
        let mut v: u128 = 0;
        for i in 0..16 {
            v |= (self.buffer[self.element_offset + i] as u128) << (8 * i);
        }
        *info = v;
    }

    pub fn Skip_L1(&mut self, _name: &str) { self.skip(1) }
    pub fn Skip_L2(&mut self, _name: &str) { self.skip(2) }
    pub fn Skip_L3(&mut self, _name: &str) { self.skip(3) }
    pub fn Skip_L4(&mut self, _name: &str) { self.skip(4) }
    pub fn Skip_L5(&mut self, _name: &str) { self.skip(5) }
    pub fn Skip_L6(&mut self, _name: &str) { self.skip(6) }
    pub fn Skip_L7(&mut self, _name: &str) { self.skip(7) }
    pub fn Skip_L8(&mut self, _name: &str) { self.skip(8) }
    pub fn Skip_L16(&mut self, _name: &str) { self.skip(16) }

    // ----------------------------------------------------------------------
    // Floats — BF* (big-endian), LF* (little-endian)
    // ----------------------------------------------------------------------

    pub fn Get_BF4(&mut self, info: &mut float32, name: &str) {
        if let Some(bits) = self.read_be_u64(4) {
            *info = f32::from_bits(bits as u32);
        } else {
            *info = 0.0;
        }
        self.param(name, *info);
    }
    pub fn Get_BF8(&mut self, info: &mut float64, name: &str) {
        if let Some(bits) = self.read_be_u64(8) {
            *info = f64::from_bits(bits);
        } else {
            *info = 0.0;
        }
        self.param(name, *info);
    }
    pub fn Get_BF10(&mut self, info: &mut float80, name: &str) {
        // 80-bit IEEE 754 extended precision, big-endian — used in AIFF.
        // Decode as a finite f64 approximation; matches the C++ side which
        // also narrows to float64 on storage.
        if self.Remain() < 10 {
            *info = 0.0;
            self.truncated = true;
            self.element_offset = self.buffer.len();
            return;
        }
        let bytes = &self.buffer[self.element_offset..self.element_offset + 10];
        self.element_offset += 10;
        *info = decode_f80_be(bytes);
        self.param(name, *info);
    }

    pub fn Get_LF4(&mut self, info: &mut float32, name: &str) {
        if let Some(bits) = self.read_le_u64(4) {
            *info = f32::from_bits(bits as u32);
        } else {
            *info = 0.0;
        }
        self.param(name, *info);
    }
    pub fn Get_LF8(&mut self, info: &mut float64, name: &str) {
        if let Some(bits) = self.read_le_u64(8) {
            *info = f64::from_bits(bits);
        } else {
            *info = 0.0;
        }
        self.param(name, *info);
    }

    pub fn Peek_BF4(&self, info: &mut float32) {
        if let Some(bits) = self.peek_be_u64(4) {
            *info = f32::from_bits(bits as u32);
        } else {
            *info = 0.0;
        }
    }
    pub fn Peek_BF8(&self, info: &mut float64) {
        if let Some(bits) = self.peek_be_u64(8) {
            *info = f64::from_bits(bits);
        } else {
            *info = 0.0;
        }
    }
    pub fn Peek_LF4(&self, info: &mut float32) {
        if let Some(bits) = self.peek_le_u64(4) {
            *info = f32::from_bits(bits as u32);
        } else {
            *info = 0.0;
        }
    }
    pub fn Peek_LF8(&self, info: &mut float64) {
        if let Some(bits) = self.peek_le_u64(8) {
            *info = f64::from_bits(bits);
        } else {
            *info = 0.0;
        }
    }

    pub fn Skip_BF4(&mut self, _name: &str) { self.skip(4) }
    pub fn Skip_BF8(&mut self, _name: &str) { self.skip(8) }
    pub fn Skip_BF10(&mut self, _name: &str) { self.skip(10) }
    pub fn Skip_LF4(&mut self, _name: &str) { self.skip(4) }
    pub fn Skip_LF8(&mut self, _name: &str) { self.skip(8) }

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

    pub fn BS_Begin(&mut self) {
        self.bs_active = true;
        self.bs_bits_consumed = 0;
    }

    pub fn BS_End(&mut self) {
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
        let bytes_after_current = if n <= bits_in_current_byte {
            0
        } else {
            (n - bits_in_current_byte).div_ceil(8)
        };
        let bytes_needed = 1 + bytes_after_current;

        if self.element_offset + bytes_needed > self.buffer.len() {
            self.truncated = true;
            self.element_offset = self.buffer.len();
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
            let chunk = (self.buffer[cursor_byte] >> shift_in_byte) as u64 & ((1u64 << take) - 1);
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

    pub fn Get_S1(&mut self, n: usize, info: &mut int8u, name: &str) {
        *info = self.read_bits_be(n) as int8u;
        self.param(name, *info);
    }
    pub fn Get_S2(&mut self, n: usize, info: &mut int16u, name: &str) {
        *info = self.read_bits_be(n) as int16u;
        self.param(name, *info);
    }
    pub fn Get_S3(&mut self, n: usize, info: &mut int32u, name: &str) {
        *info = self.read_bits_be(n) as int32u;
        self.param(name, *info);
    }
    pub fn Get_S4(&mut self, n: usize, info: &mut int32u, name: &str) {
        *info = self.read_bits_be(n) as int32u;
        self.param(name, *info);
    }
    pub fn Get_S5(&mut self, n: usize, info: &mut int64u, name: &str) {
        *info = self.read_bits_be(n);
        self.param(name, *info);
    }
    pub fn Get_S8(&mut self, n: usize, info: &mut int64u, name: &str) {
        *info = self.read_bits_be(n);
        self.param(name, *info);
    }

    pub fn Skip_S1(&mut self, n: usize, _name: &str) { self.read_bits_be(n); }
    pub fn Skip_S2(&mut self, n: usize, _name: &str) { self.read_bits_be(n); }
    pub fn Skip_S3(&mut self, n: usize, _name: &str) { self.read_bits_be(n); }
    pub fn Skip_S4(&mut self, n: usize, _name: &str) { self.read_bits_be(n); }
    pub fn Skip_S5(&mut self, n: usize, _name: &str) { self.read_bits_be(n); }
    pub fn Skip_S8(&mut self, n: usize, _name: &str) { self.read_bits_be(n); }

    // ----------------------------------------------------------------------
    // 4CC / Character codes (Get_C4 is used everywhere for MP4 atoms, RIFF)
    // ----------------------------------------------------------------------

    pub fn Get_C4(&mut self, info: &mut int32u, name: &str) {
        // 4CCs are read as a big-endian u32 of 4 ASCII bytes. Display happens
        // via Ztring::From_CC4 elsewhere; for trace we render the printable
        // form when all bytes are ASCII printable, else fall back to u32.
        *info = self.read_be_u64(4).unwrap_or(0) as int32u;
        if self.trace_activated && !name.is_empty() {
            let bytes = info.to_be_bytes();
            let printable = bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ');
            let value = if printable {
                String::from_utf8_lossy(&bytes).into_owned()
            } else {
                info.to_string()
            };
            self.tree.Param(name, value);
        }
    }

    pub fn Peek_C4(&self, info: &mut int32u) {
        self.Peek_B4(info)
    }

    pub fn Skip_C4(&mut self, _name: &str) {
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
        let mut a: int8u = 0;
        let mut b: int16u = 0;
        let mut c: int32u = 0;
        fa.Get_B1(&mut a, "a");
        fa.Get_B2(&mut b, "b");
        fa.Get_B4(&mut c, "c");
        assert_eq!(a, 0x12);
        assert_eq!(b, 0x3456);
        assert_eq!(c, 0x789A_BCDE);
        assert_eq!(fa.Element_Offset(), 7);
        assert_eq!(fa.Remain(), 1);
    }

    #[test]
    fn get_b3_5_6_7_widths() {
        let buf = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];
        let mut fa = FileAnalyze::new(&buf);
        let mut a: int32u = 0;
        let mut b: int64u = 0;
        fa.Get_B3(&mut a, "a");
        fa.Get_B5(&mut b, "b");
        assert_eq!(a, 0x00_0102_03);
        assert_eq!(b, 0x00_0405_0607_08);
    }

    #[test]
    fn get_b16_reads_128_bits() {
        let buf: Vec<u8> = (0..16u8).collect();
        let mut fa = FileAnalyze::new(&buf);
        let mut v: int128u = 0;
        fa.Get_B16(&mut v, "guid");
        assert_eq!(v, 0x0001_0203_0405_0607_0809_0A0B_0C0D_0E0F);
    }

    #[test]
    fn truncation_pins_position_and_sets_flag() {
        let buf = [0xAA, 0xBB];
        let mut fa = FileAnalyze::new(&buf);
        let mut v: int32u = 0;
        fa.Get_B4(&mut v, "v");
        assert_eq!(v, 0);
        assert!(fa.truncated());
        assert_eq!(fa.Element_Offset(), 2);
    }

    #[test]
    fn peek_does_not_advance() {
        let buf = [0x11, 0x22, 0x33, 0x44];
        let mut fa = FileAnalyze::new(&buf);
        let mut v: int32u = 0;
        fa.Peek_B4(&mut v);
        assert_eq!(v, 0x1122_3344);
        assert_eq!(fa.Element_Offset(), 0);
        let mut w: int32u = 0;
        fa.Get_B4(&mut w, "w");
        assert_eq!(w, 0x1122_3344);
        assert_eq!(fa.Element_Offset(), 4);
    }

    #[test]
    fn skip_advances() {
        let buf = [0; 16];
        let mut fa = FileAnalyze::new(&buf);
        fa.Skip_B4("padding");
        fa.Skip_Hexa(8, "header");
        assert_eq!(fa.Element_Offset(), 12);
        assert_eq!(fa.Remain(), 4);
    }

    #[test]
    fn cc4_reads_mp4_atom_name_as_u32() {
        // "ftyp" atom — F=0x66 t=0x74 y=0x79 p=0x70
        let buf = [b'f', b't', b'y', b'p'];
        let mut fa = FileAnalyze::new(&buf);
        let mut code: int32u = 0;
        fa.Get_C4(&mut code, "Type");
        assert_eq!(code, 0x6674_7970);
    }

    #[test]
    fn get_l1_through_l8_read_little_endian() {
        // Same bytes as the BE test; expect bytes reversed in numeric value.
        let buf = [0x12, 0x34, 0x56, 0x78];
        let mut fa = FileAnalyze::new(&buf);
        let mut a: int8u = 0;
        let mut b: int16u = 0;
        fa.Get_L1(&mut a, "a");
        fa.Get_L2(&mut b, "b");
        assert_eq!(a, 0x12);
        assert_eq!(b, 0x5634);

        let buf2 = [0x12, 0x34, 0x56, 0x78];
        let mut fa2 = FileAnalyze::new(&buf2);
        let mut c: int32u = 0;
        fa2.Get_L4(&mut c, "c");
        assert_eq!(c, 0x7856_3412);
    }

    #[test]
    fn get_l16_reads_uuid_little_endian() {
        // First 8 bytes form low 64 bits, last 8 bytes form high 64 bits.
        let buf: [u8; 16] = [
            0xEF, 0xCD, 0xAB, 0x90, 0x78, 0x56, 0x34, 0x12,
            0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11,
        ];
        let mut fa = FileAnalyze::new(&buf);
        let mut v: int128u = 0;
        fa.Get_L16(&mut v, "uuid");
        assert_eq!(v, 0x1122_3344_5566_7788_1234_5678_90AB_CDEF);
    }

    #[test]
    fn get_bf4_reads_be_float32() {
        let v = 3.14159_f32;
        let buf = v.to_be_bytes();
        let mut fa = FileAnalyze::new(&buf);
        let mut out: float32 = 0.0;
        fa.Get_BF4(&mut out, "pi");
        assert_eq!(out, v);
    }

    #[test]
    fn get_bf8_reads_be_float64() {
        let v = std::f64::consts::E;
        let buf = v.to_be_bytes();
        let mut fa = FileAnalyze::new(&buf);
        let mut out: float64 = 0.0;
        fa.Get_BF8(&mut out, "e");
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
        let mut a: float32 = 0.0;
        let mut b: float64 = 0.0;
        fa.Get_LF4(&mut a, "a");
        fa.Get_LF8(&mut b, "b");
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
        let mut hz: float80 = 0.0;
        fa.Get_BF10(&mut hz, "SampleRate");
        assert!((hz - 44100.0).abs() < 1e-9, "got {hz}");
    }

    #[test]
    fn get_bf10_zero() {
        let buf = [0u8; 10];
        let mut fa = FileAnalyze::new(&buf);
        let mut v: float80 = 1.0;
        fa.Get_BF10(&mut v, "zero");
        assert_eq!(v, 0.0);
    }

    #[test]
    fn get_b4_records_field_on_current_element() {
        let buf = [0xDE, 0xAD, 0xBE, 0xEF];
        let mut fa = FileAnalyze::new(&buf);
        fa.Element_Begin("header");
        let mut v: int32u = 0;
        fa.Get_B4(&mut v, "Magic");
        fa.Element_End();

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
        fa.Element_Begin("atom");
        let mut code: int32u = 0;
        fa.Get_C4(&mut code, "Type");
        fa.Element_End();

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
        fa.Element_Begin("moov");
            fa.Element_Begin("mvhd");
                let mut ver: int8u = 0;
                fa.Get_B1(&mut ver, "Version");
                let mut flags: int32u = 0;
                fa.Get_B3(&mut flags, "Flags");
            fa.Element_End();
            fa.Element_Begin("trak");
            fa.Element_End();
        fa.Element_End();

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
        fa.Element_Begin("silent");
        let mut v: int32u = 0;
        fa.Get_B4(&mut v, "Value");
        fa.Element_End();
        assert_eq!(v, 0x1234_5678);
        assert!(fa.tree().root().children[0].infos.is_empty());
    }

    #[test]
    fn file_analyze_fills_streams_directly() {
        let buf = [0; 4];
        let mut fa = FileAnalyze::new(&buf);
        let pos = fa.Stream_Prepare(StreamKind::Audio);
        fa.Fill(StreamKind::Audio, pos, "Format", "FLAC", false);
        fa.Fill(StreamKind::Audio, pos, "BitDepth", "24", false);
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, pos, "Format").map(|z| z.as_str()),
            Some("FLAC")
        );
        assert_eq!(fa.Count_Get(StreamKind::Audio), 1);
    }

    #[test]
    fn bs_get_s_reads_bit_aligned_fields() {
        // 0xAB 0xCD = 1010 1011 1100 1101
        // Read: 4 bits (1010 = 0xA), 4 bits (1011 = 0xB), 8 bits (1100 1101 = 0xCD)
        let buf = [0xAB, 0xCD];
        let mut fa = FileAnalyze::new(&buf);
        fa.BS_Begin();
        let mut a: int8u = 0;
        let mut b: int8u = 0;
        let mut c: int8u = 0;
        fa.Get_S1(4, &mut a, "a");
        fa.Get_S1(4, &mut b, "b");
        fa.Get_S1(8, &mut c, "c");
        fa.BS_End();
        assert_eq!(a, 0xA);
        assert_eq!(b, 0xB);
        assert_eq!(c, 0xCD);
        assert_eq!(fa.Element_Offset(), 2);
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
        fa.BS_Begin();
        let mut sr: int32u = 0;
        let mut ch: int8u = 0;
        let mut bps: int8u = 0;
        let mut samp: int64u = 0;
        fa.Get_S3(20, &mut sr, "SampleRate");
        fa.Get_S1(3, &mut ch, "Channels");
        fa.Get_S1(5, &mut bps, "BitsPerSample");
        fa.Get_S5(36, &mut samp, "Samples");
        fa.BS_End();
        assert_eq!(sr, 48000);
        assert_eq!(ch + 1, 2);
        assert_eq!(bps + 1, 16);
        assert_eq!(samp, 71638);
        assert_eq!(fa.Element_Offset(), 8);
    }

    #[test]
    fn bs_end_byte_aligns_when_partially_consumed() {
        let buf = [0xFF, 0x12];
        let mut fa = FileAnalyze::new(&buf);
        fa.BS_Begin();
        let mut a: int8u = 0;
        fa.Get_S1(3, &mut a, "a");
        assert_eq!(a, 0b111);
        fa.BS_End();
        // Aligned: should now be at byte index 1
        assert_eq!(fa.Element_Offset(), 1);
        let mut b: int8u = 0;
        fa.Get_B1(&mut b, "b");
        assert_eq!(b, 0x12);
    }

    #[test]
    fn bs_end_no_op_when_already_aligned() {
        let buf = [0xAA, 0xBB];
        let mut fa = FileAnalyze::new(&buf);
        fa.BS_Begin();
        let mut a: int8u = 0;
        fa.Get_S1(8, &mut a, "a");
        fa.BS_End();
        assert_eq!(a, 0xAA);
        assert_eq!(fa.Element_Offset(), 1);
    }

    #[test]
    fn empty_name_does_not_record_param() {
        let buf = [0xAA, 0xBB];
        let mut fa = FileAnalyze::new(&buf);
        fa.Element_Begin("e");
        let mut v: int16u = 0;
        fa.Get_B2(&mut v, "");
        fa.Element_End();
        assert!(fa.tree().root().children[0].infos.is_empty());
    }
}
