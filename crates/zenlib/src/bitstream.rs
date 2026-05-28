//! Transliteration of `ZenLib::BitStream` — MSB-first bit reader.
//!
//! Behavior matches the C++ header line-for-line: same field set, same
//! underrun semantics (read past end zeros remaining state and returns 0),
//! same bookmark/peek mechanics, same Byte_Align that consumes the partial
//! byte. The C++ holds a `const Int8u*` that gets advanced; here we keep
//! the slice and track an offset index — equivalent state, no `unsafe`.

use crate::types::{Int8u, Int16u, Int32u, Int64u};

const MASK: [u32; 33] = [
    0x0000_0000,
    0x0000_0001, 0x0000_0003, 0x0000_0007, 0x0000_000f,
    0x0000_001f, 0x0000_003f, 0x0000_007f, 0x0000_00ff,
    0x0000_01ff, 0x0000_03ff, 0x0000_07ff, 0x0000_0fff,
    0x0000_1fff, 0x0000_3fff, 0x0000_7fff, 0x0000_ffff,
    0x0001_ffff, 0x0003_ffff, 0x0007_ffff, 0x000f_ffff,
    0x001f_ffff, 0x003f_ffff, 0x007f_ffff, 0x00ff_ffff,
    0x01ff_ffff, 0x03ff_ffff, 0x07ff_ffff, 0x0fff_ffff,
    0x1fff_ffff, 0x3fff_ffff, 0x7fff_ffff, 0xffff_ffff,
];

#[derive(Clone, Copy)]
struct Snapshot {
    pos: usize,
    buffer_size: usize,
    last_byte: usize,
    last_byte_size: usize,
    buffer_under_run: bool,
}

pub struct BitStream<'a> {
    buffer: &'a [u8],
    pos: usize,
    buffer_size: usize,
    buffer_size_init: usize,
    buffer_size_before_last_call: usize,
    last_byte: usize,
    last_byte_size: usize,
    buffer_under_run: bool,
    bookmark: Option<Snapshot>,
}

impl<'a> BitStream<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        let bits = buffer.len() * 8;
        BitStream {
            buffer,
            pos: 0,
            buffer_size: bits,
            buffer_size_init: bits,
            buffer_size_before_last_call: bits,
            last_byte: 0,
            last_byte_size: 0,
            buffer_under_run: buffer.is_empty(),
            bookmark: None,
        }
    }

    pub fn Attach(&mut self, buffer: &'a [u8]) {
        if std::ptr::eq(buffer.as_ptr(), self.buffer.as_ptr())
            && buffer.len() == self.buffer.len()
        {
            return;
        }
        let bits = buffer.len() * 8;
        self.buffer = buffer;
        self.pos = 0;
        self.buffer_size = bits;
        self.buffer_size_init = bits;
        self.buffer_size_before_last_call = bits;
        self.last_byte_size = 0;
        self.buffer_under_run = buffer.is_empty();
        self.bookmark = None;
    }

    pub fn Get(&mut self, how_many: usize) -> Int32u {
        let to_return: usize;
        if how_many == 0 || how_many > 32 {
            return 0;
        }
        if how_many > self.buffer_size + self.last_byte_size {
            self.buffer_size = 0;
            self.last_byte_size = 0;
            self.buffer_under_run = true;
            return 0;
        }

        self.buffer_size_before_last_call = self.buffer_size + self.last_byte_size;

        if how_many <= self.last_byte_size {
            self.last_byte_size -= how_many;
            to_return = self.last_byte >> self.last_byte_size;
        } else {
            let mut new_bits = how_many - self.last_byte_size;
            let mut acc: usize = if new_bits == 32 {
                0
            } else {
                self.last_byte << new_bits
            };
            // Mirror the C++ switch/fallthrough: pull whole bytes until <=8
            // bits remain to consume.
            let mut tier = (new_bits - 1) / 8;
            while tier >= 1 {
                new_bits -= 8;
                acc |= (self.buffer[self.pos] as usize) << new_bits;
                self.pos += 1;
                self.buffer_size -= 8;
                tier -= 1;
            }
            // case 0: load the next byte as the new LastByte
            self.last_byte = self.buffer[self.pos] as usize;
            self.pos += 1;
            self.last_byte_size = self.buffer_size.min(8) - new_bits;
            self.buffer_size -= self.buffer_size.min(8);
            acc |= (self.last_byte >> self.last_byte_size) & MASK[new_bits] as usize;
            to_return = acc;
        }
        (to_return as u32) & MASK[how_many]
    }

    pub fn GetB(&mut self) -> bool {
        self.Get(1) != 0
    }

    pub fn Get1(&mut self, how_many: usize) -> Int8u {
        self.Get(how_many) as Int8u
    }

    pub fn Get2(&mut self, how_many: usize) -> Int16u {
        self.Get(how_many) as Int16u
    }

    pub fn Get4(&mut self, how_many: usize) -> Int32u {
        self.Get(how_many)
    }

    pub fn Get8(&mut self, how_many: usize) -> Int64u {
        if how_many > 64 {
            return 0;
        }
        let how_many1 = if how_many > 32 { how_many - 32 } else { 0 };
        let how_many2 = how_many - how_many1;
        let value1 = self.Get(how_many1) as Int64u;
        let value2 = self.Get(how_many2) as Int64u;
        if self.buffer_under_run {
            return 0;
        }
        value1 * 0x1_0000_0000 + value2
    }

    pub fn Skip(&mut self, mut how_many: usize) {
        if how_many == 0 {
            return;
        }
        if how_many > 32 {
            while how_many > 32 {
                self.Skip(32);
                how_many -= 32;
            }
            if how_many > 0 {
                self.Skip(how_many);
            }
            return;
        }
        if how_many > self.buffer_size + self.last_byte_size {
            self.buffer_size = 0;
            self.last_byte_size = 0;
            self.buffer_under_run = true;
            return;
        }
        self.buffer_size_before_last_call = self.buffer_size + self.last_byte_size;

        if how_many <= self.last_byte_size {
            self.last_byte_size -= how_many;
        } else {
            let mut new_bits = how_many - self.last_byte_size;
            let mut tier = (new_bits - 1) / 8;
            while tier >= 1 {
                new_bits -= 8;
                self.pos += 1;
                self.buffer_size -= 8;
                tier -= 1;
            }
            self.last_byte = self.buffer[self.pos] as usize;
            self.pos += 1;
            self.last_byte_size = self.buffer_size.min(8) - new_bits;
            self.buffer_size -= self.buffer_size.min(8);
        }
    }

    pub fn SkipB(&mut self) { self.Skip(1) }
    pub fn Skip1(&mut self, how_many: usize) { self.Skip(how_many) }
    pub fn Skip2(&mut self, how_many: usize) { self.Skip(how_many) }
    pub fn Skip4(&mut self, how_many: usize) { self.Skip(how_many) }

    pub fn Skip8(&mut self, how_many: usize) {
        if how_many > 64 {
            return;
        }
        let how_many1 = if how_many > 32 { how_many - 32 } else { 0 };
        let how_many2 = how_many - how_many1;
        self.Skip(how_many1);
        self.Skip(how_many2);
    }

    pub fn Peek(&mut self, how_many: usize) -> Int32u {
        self.BookMarkPos(true);
        let v = self.Get(how_many);
        self.BookMarkPos(false);
        v
    }

    pub fn PeekB(&mut self) -> bool { self.Peek(1) != 0 }
    pub fn Peek1(&mut self, how_many: usize) -> Int8u { self.Peek(how_many) as Int8u }
    pub fn Peek2(&mut self, how_many: usize) -> Int16u { self.Peek(how_many) as Int16u }
    pub fn Peek3(&mut self, how_many: usize) -> Int32u { self.Peek(how_many) }
    pub fn Peek4(&mut self, how_many: usize) -> Int32u { self.Peek(how_many) }
    pub fn Peek8(&mut self, how_many: usize) -> Int64u { self.Peek(how_many) as Int64u }

    pub fn BookMarkPos(&mut self, set: bool) {
        if set {
            self.bookmark = Some(Snapshot {
                pos: self.pos,
                buffer_size: self.buffer_size,
                last_byte: self.last_byte,
                last_byte_size: self.last_byte_size,
                buffer_under_run: self.buffer_under_run,
            });
        } else if let Some(s) = self.bookmark.take() {
            self.pos = s.pos;
            self.buffer_size = s.buffer_size;
            self.last_byte = s.last_byte;
            self.last_byte_size = s.last_byte_size;
            self.buffer_under_run = s.buffer_under_run;
        }
    }

    pub fn Remain(&self) -> Int32u {
        (self.buffer_size + self.last_byte_size) as Int32u
    }

    pub fn Byte_Align(&mut self) {
        self.Get(self.last_byte_size);
    }

    pub fn Offset_Get(&self) -> usize {
        if self.buffer_under_run {
            return 0;
        }
        (self.buffer_size_init - self.buffer_size) / 8
    }

    pub fn BitOffset_Get(&self) -> usize {
        if self.buffer_under_run {
            return 0;
        }
        self.last_byte_size
    }

    pub fn OffsetBeforeLastCall_Get(&self) -> usize {
        if self.buffer_under_run {
            return 0;
        }
        (self.buffer_size_init - self.buffer_size_before_last_call) / 8
    }

    pub fn BufferUnderRun(&self) -> bool {
        self.buffer_under_run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_single_bits_msb_first() {
        // 0b1010_1100 = 0xAC
        let mut bs = BitStream::new(&[0xAC]);
        assert!(bs.GetB());
        assert!(!bs.GetB());
        assert!(bs.GetB());
        assert!(!bs.GetB());
        assert!(bs.GetB());
        assert!(bs.GetB());
        assert!(!bs.GetB());
        assert!(!bs.GetB());
    }

    #[test]
    fn read_nibbles_across_bytes() {
        let mut bs = BitStream::new(&[0xAB, 0xCD, 0xEF]);
        assert_eq!(bs.Get(4), 0xA);
        assert_eq!(bs.Get(4), 0xB);
        assert_eq!(bs.Get(4), 0xC);
        assert_eq!(bs.Get(4), 0xD);
        assert_eq!(bs.Get(4), 0xE);
        assert_eq!(bs.Get(4), 0xF);
        assert!(!bs.BufferUnderRun());
    }

    #[test]
    fn read_32_bits_aligned() {
        let mut bs = BitStream::new(&[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(bs.Get(32), 0xDEAD_BEEF);
    }

    #[test]
    fn read_64_bits_via_get8() {
        let mut bs = BitStream::new(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]);
        assert_eq!(bs.Get8(64), 0xDEAD_BEEF_CAFE_BABE);
    }

    #[test]
    fn read_unaligned_3_bits_then_5() {
        // 0b1101_1010 = 0xDA — first 3 = 110 (6), next 5 = 11010 (26)
        let mut bs = BitStream::new(&[0xDA]);
        assert_eq!(bs.Get(3), 6);
        assert_eq!(bs.Get(5), 26);
    }

    #[test]
    fn underrun_zeros_result_and_flags() {
        let mut bs = BitStream::new(&[0xFF]);
        assert_eq!(bs.Get(16), 0);
        assert!(bs.BufferUnderRun());
        assert_eq!(bs.remain(), 0);
    }

    #[test]
    fn peek_does_not_advance() {
        let mut bs = BitStream::new(&[0xAB, 0xCD]);
        assert_eq!(bs.Peek(8), 0xAB);
        assert_eq!(bs.Peek(8), 0xAB);
        assert_eq!(bs.Get(8), 0xAB);
        assert_eq!(bs.Get(8), 0xCD);
    }

    #[test]
    fn skip_then_read() {
        let mut bs = BitStream::new(&[0xAB, 0xCD, 0xEF]);
        bs.Skip(12);
        assert_eq!(bs.Get(12), 0xDEF);
    }

    #[test]
    fn skip_more_than_32() {
        let mut bs = BitStream::new(&[0; 8]);
        bs.Skip(40);
        assert_eq!(bs.remain(), 24);
    }

    #[test]
    fn byte_align_consumes_partial_byte() {
        let mut bs = BitStream::new(&[0xAB, 0xCD]);
        assert_eq!(bs.Get(3), 0b101);
        assert_eq!(bs.BitOffset_Get(), 5);
        bs.Byte_Align();
        assert_eq!(bs.BitOffset_Get(), 0);
        assert_eq!(bs.Get(8), 0xCD);
    }

    #[test]
    fn offset_tracking() {
        let mut bs = BitStream::new(&[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(bs.Offset_Get(), 0);
        bs.Get(16);
        assert_eq!(bs.Offset_Get(), 2);
    }
}
