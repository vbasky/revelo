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

use zenlib::{int8u, int16u, int32u, int64u, int128u};

pub struct FileAnalyze<'a> {
    buffer: &'a [u8],
    element_offset: usize,
    truncated: bool,
}

impl<'a> FileAnalyze<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        FileAnalyze {
            buffer,
            element_offset: 0,
            truncated: false,
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

    pub fn Get_B1(&mut self, info: &mut int8u, _name: &str) {
        *info = self.read_be_u64(1).unwrap_or(0) as int8u;
    }
    pub fn Get_B2(&mut self, info: &mut int16u, _name: &str) {
        *info = self.read_be_u64(2).unwrap_or(0) as int16u;
    }
    pub fn Get_B3(&mut self, info: &mut int32u, _name: &str) {
        *info = self.read_be_u64(3).unwrap_or(0) as int32u;
    }
    pub fn Get_B4(&mut self, info: &mut int32u, _name: &str) {
        *info = self.read_be_u64(4).unwrap_or(0) as int32u;
    }
    pub fn Get_B5(&mut self, info: &mut int64u, _name: &str) {
        *info = self.read_be_u64(5).unwrap_or(0);
    }
    pub fn Get_B6(&mut self, info: &mut int64u, _name: &str) {
        *info = self.read_be_u64(6).unwrap_or(0);
    }
    pub fn Get_B7(&mut self, info: &mut int64u, _name: &str) {
        *info = self.read_be_u64(7).unwrap_or(0);
    }
    pub fn Get_B8(&mut self, info: &mut int64u, _name: &str) {
        *info = self.read_be_u64(8).unwrap_or(0);
    }
    pub fn Get_B16(&mut self, info: &mut int128u, _name: &str) {
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

    fn skip(&mut self, n: usize) {
        if self.Remain() < n {
            self.truncated = true;
            self.element_offset = self.buffer.len();
        } else {
            self.element_offset += n;
        }
    }

    // ----------------------------------------------------------------------
    // 4CC / Character codes (Get_C4 is used everywhere for MP4 atoms, RIFF)
    // ----------------------------------------------------------------------

    pub fn Get_C4(&mut self, info: &mut int32u, _name: &str) {
        // 4CCs are read as a big-endian u32 of 4 ASCII bytes. Display happens
        // via Ztring::From_CC4 elsewhere.
        self.Get_B4(info, _name)
    }

    pub fn Peek_C4(&self, info: &mut int32u) {
        self.Peek_B4(info)
    }

    pub fn Skip_C4(&mut self, _name: &str) {
        self.skip(4)
    }
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
}
