//! Ergonomic byte reader — wraps [`FileAnalyze`] with a fluent, Rust-native API.
//!
//! Every read method returns the value directly (no out-parameters) and
//! advances the cursor. Peek methods do not advance. All methods return
//! `Option<T>` — `None` means buffer truncated.
//!
//! ```ignore
//! let r = Reader::wrap(&mut fa);
//! let width  = r.be_u32("Width")?;
//! let depth  = r.be_u8("BitDepth")?;
//! let magic  = r.fourcc("Magic")?;
//!
//! r.bits(|b| {
//!     let bsid  = b.read::<u8>(5, "bsid")?;
//!     Ok(())
//! })?;
//! ```

use crate::file_analyze::FileAnalyze;

type R<T> = Option<T>;

fn ok<T>(fa: &FileAnalyze<'_>, v: T) -> R<T> {
    if fa.truncated() { None } else { Some(v) }
}

/// Fluent byte reader over a [`FileAnalyze`] buffer.
pub struct Reader<'a, 'b> {
    fa: &'a mut FileAnalyze<'b>,
}

impl<'a, 'b> Reader<'a, 'b> {
    pub fn wrap(fa: &'a mut FileAnalyze<'b>) -> Self {
        Self { fa }
    }

    // ── raw ────────────────────────────────────────────────────

    pub fn peek_raw(&self, n: usize) -> Option<&[u8]> {
        self.fa.peek_raw(n)
    }

    pub fn peek_magic<const N: usize>(&self, magic: &[u8; N]) -> bool {
        self.fa.peek_magic(magic)
    }

    pub fn read_raw(&mut self, n: usize) -> Option<&[u8]> {
        let out = self.fa.read_raw(n);
        if out.is_empty() && n > 0 { None } else { Some(out) }
    }

    pub fn skip(&mut self, n: usize) -> R<()> {
        if self.fa.remain() < n {
            None
        } else {
            self.fa.skip_hexa(n, "");
            Some(())
        }
    }

    pub fn remain(&self) -> usize {
        self.fa.remain()
    }

    // ── element tree ───────────────────────────────────────────

    pub fn element_begin(&mut self, name: &str) {
        self.fa.element_begin(name);
    }
    pub fn element_end(&mut self) {
        self.fa.element_end();
    }
    pub fn element_offset(&self) -> usize {
        self.fa.element_offset()
    }

    // ── stream filling ─────────────────────────────────────────

    pub fn stream_prepare(&mut self, kind: crate::stream::StreamKind) -> usize {
        self.fa.stream_prepare(kind)
    }
    pub fn set_field(
        &mut self,
        kind: crate::stream::StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<zenlib::Ztring>,
    ) {
        self.fa.set_field(kind, pos, parameter, value);
    }
    pub fn force_field(
        &mut self,
        kind: crate::stream::StreamKind,
        pos: usize,
        parameter: &str,
        value: impl Into<zenlib::Ztring>,
    ) {
        self.fa.force_field(kind, pos, parameter, value);
    }

    // ── big-endian integers ────────────────────────────────────

    pub fn be_u8(&mut self, name: &str) -> R<u8> {
        let v = self.fa.get_b1(name);
        ok(self.fa, v)
    }
    pub fn be_u16(&mut self, name: &str) -> R<u16> {
        let v = self.fa.get_b2(name);
        ok(self.fa, v)
    }
    pub fn be_u24(&mut self, name: &str) -> R<u32> {
        let v = self.fa.get_b3(name);
        ok(self.fa, v)
    }
    pub fn be_u32(&mut self, name: &str) -> R<u32> {
        let v = self.fa.get_b4(name);
        ok(self.fa, v)
    }
    pub fn be_u40(&mut self, name: &str) -> R<u64> {
        let v = self.fa.get_b5(name);
        ok(self.fa, v)
    }
    pub fn be_u48(&mut self, name: &str) -> R<u64> {
        let v = self.fa.get_b6(name);
        ok(self.fa, v)
    }
    pub fn be_u56(&mut self, name: &str) -> R<u64> {
        let v = self.fa.get_b7(name);
        ok(self.fa, v)
    }
    pub fn be_u64(&mut self, name: &str) -> R<u64> {
        let v = self.fa.get_b8(name);
        ok(self.fa, v)
    }
    pub fn be_u128(&mut self, name: &str) -> R<u128> {
        let v = self.fa.get_b16(name);
        ok(self.fa, v)
    }

    // ── little-endian integers ─────────────────────────────────

    pub fn le_u8(&mut self, name: &str) -> R<u8> {
        let v = self.fa.get_l1(name);
        ok(self.fa, v)
    }
    pub fn le_u16(&mut self, name: &str) -> R<u16> {
        let v = self.fa.get_l2(name);
        ok(self.fa, v)
    }
    pub fn le_u24(&mut self, name: &str) -> R<u32> {
        let v = self.fa.get_l3(name);
        ok(self.fa, v)
    }
    pub fn le_u32(&mut self, name: &str) -> R<u32> {
        let v = self.fa.get_l4(name);
        ok(self.fa, v)
    }
    pub fn le_u64(&mut self, name: &str) -> R<u64> {
        let v = self.fa.get_l8(name);
        ok(self.fa, v)
    }

    // ── peeks (non-advancing) ──────────────────────────────────

    pub fn peek_be_u16(&self) -> R<u16> {
        let v = self.fa.peek_b2();
        ok(self.fa, v)
    }
    pub fn peek_be_u32(&self) -> R<u32> {
        let v = self.fa.peek_b4();
        ok(self.fa, v)
    }
    pub fn peek_be_u64(&self) -> R<u64> {
        let v = self.fa.peek_b8();
        ok(self.fa, v)
    }
    pub fn peek_le_u16(&self) -> R<u16> {
        let v = self.fa.peek_l2();
        ok(self.fa, v)
    }
    pub fn peek_le_u32(&self) -> R<u32> {
        let v = self.fa.peek_l4();
        ok(self.fa, v)
    }

    // ── floats ─────────────────────────────────────────────────

    pub fn be_f32(&mut self, name: &str) -> R<f32> {
        let v = self.fa.get_bf4(name);
        ok(self.fa, v)
    }
    pub fn be_f64(&mut self, name: &str) -> R<f64> {
        let v = self.fa.get_bf8(name);
        ok(self.fa, v)
    }
    pub fn le_f32(&mut self, name: &str) -> R<f32> {
        let v = self.fa.get_lf4(name);
        ok(self.fa, v)
    }
    pub fn le_f64(&mut self, name: &str) -> R<f64> {
        let v = self.fa.get_lf8(name);
        ok(self.fa, v)
    }
    pub fn be_f80(&mut self, name: &str) -> R<f64> {
        let v = self.fa.get_bf10(name);
        ok(self.fa, v)
    }

    // ── 4CC ────────────────────────────────────────────────────

    pub fn fourcc(&mut self, name: &str) -> R<u32> {
        let v = self.fa.get_c4(name);
        ok(self.fa, v)
    }

    // ── bitstream ──────────────────────────────────────────────

    pub fn bits<F, T>(&mut self, f: F) -> R<T>
    where
        F: FnOnce(&mut BitReader<'_, 'b>) -> R<T>,
    {
        self.fa.bs_begin();
        let mut br = BitReader { fa: &mut *self.fa };
        let result = f(&mut br);
        if !br.fa.truncated() {
            br.fa.bs_end();
        }
        result
    }
}

// ── bit reader ─────────────────────────────────────────────────

pub struct BitReader<'a, 'b> {
    fa: &'a mut FileAnalyze<'b>,
}

impl BitReader<'_, '_> {
    pub fn read<T: FromBits>(&mut self, n: usize, name: &str) -> R<T> {
        T::read_bits_be(self.fa, n, name)
    }
    pub fn skip(&mut self, n: usize) {
        self.fa.skip_s1(n, "");
    }
}

pub trait FromBits: Sized {
    fn read_bits_be(fa: &mut FileAnalyze<'_>, n: usize, name: &str) -> R<Self>;
}

impl FromBits for u8 {
    fn read_bits_be(fa: &mut FileAnalyze<'_>, n: usize, name: &str) -> R<Self> {
        let v = fa.get_s1(n, name);
        ok(fa, v)
    }
}
impl FromBits for u16 {
    fn read_bits_be(fa: &mut FileAnalyze<'_>, n: usize, name: &str) -> R<Self> {
        let v = fa.get_s2(n, name);
        ok(fa, v)
    }
}
impl FromBits for u32 {
    fn read_bits_be(fa: &mut FileAnalyze<'_>, n: usize, name: &str) -> R<Self> {
        let v = fa.get_s3(n, name);
        ok(fa, v)
    }
}
impl FromBits for u64 {
    fn read_bits_be(fa: &mut FileAnalyze<'_>, n: usize, name: &str) -> R<Self> {
        let v = fa.get_s5(n, name);
        ok(fa, v)
    }
}
