/// A source of bytes addressable at arbitrary offsets.
///
/// # Implementors
///
/// | Type | I/O model | Use when |
/// |---|---|---|
/// | [`ReadBackend::Slice`] | Zero-copy `&[u8]` borrow | Bytes already in memory |
/// | [`ReadBackend::Mapped`] | Zero-copy mmap, OS pages on demand | Local files of any size |
/// | (future) `ReadBackend::Streamed` | Sliding window over `Read + Seek` | HTTP range requests, pipes |
///
/// # Future: `Read + Seek` sources
///
/// A `Streamed` variant wrapping a `Box<dyn ReadAndSeek>` or similar
/// is the natural extension. It would maintain an internal sliding window:
/// reads that fall within the current window return `&[u8]` slices;
/// reads beyond it trigger a window shift via `Seek::seek` + `Read::read_exact`.
/// Callers that need this today can prototype by reading the file on
/// their own terms and using [`ReadBackend::Slice`].
pub trait ByteSource: std::fmt::Debug {
    /// Total length of the source in bytes.
    fn len(&self) -> usize;

    /// Whether the source is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the byte at absolute `offset`, or `None` if out of bounds.
    fn byte_at(&self, offset: usize) -> Option<u8>;

    /// Return a slice of `len` bytes starting at absolute `offset`,
    /// or `None` if the range is not fully available.
    fn slice_at(&self, offset: usize, len: usize) -> Option<&[u8]>;
}

/// The concrete byte source used by [`FileAnalyze`](crate::FileAnalyze).
///
/// An enum with two zero-copy variants today; a `Streamed` variant for
/// `Read + Seek` sources is planned but not yet implemented.
#[derive(Debug, Clone, Copy)]
pub enum ReadBackend<'a> {
    /// Bytes already in memory — the classic `&[u8]` path.
    Slice(&'a [u8]),
    /// Operating-system memory-mapped file. The backing file is opened
    /// read-only and the OS faults in pages on demand.
    ///
    /// Only available when the `mmap` feature is enabled (on by default;
    /// disabled for WASM builds).
    #[cfg(feature = "mmap")]
    Mapped(&'a memmap2::Mmap),
}

impl<'a> ReadBackend<'a> {
    /// View the entire backend as a contiguous `&[u8]` slice.
    ///
    /// Both `Slice` and `Mapped` implement efficient deref to `[u8]`;
    /// this method dispatches to the active variant.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            ReadBackend::Slice(s) => s,
            #[cfg(feature = "mmap")]
            ReadBackend::Mapped(m) => m.as_ref(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ByteSource for ReadBackend<'_> {
    #[inline]
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    #[inline]
    fn byte_at(&self, offset: usize) -> Option<u8> {
        self.as_slice().get(offset).copied()
    }

    #[inline]
    fn slice_at(&self, offset: usize, len: usize) -> Option<&[u8]> {
        self.as_slice().get(offset..offset + len)
    }
}

impl<'a> From<&'a [u8]> for ReadBackend<'a> {
    #[inline]
    fn from(slice: &'a [u8]) -> Self {
        ReadBackend::Slice(slice)
    }
}

impl<'a> From<&'a Vec<u8>> for ReadBackend<'a> {
    #[inline]
    fn from(v: &'a Vec<u8>) -> Self {
        ReadBackend::Slice(v.as_slice())
    }
}

#[cfg(feature = "mmap")]
impl<'a> From<&'a memmap2::Mmap> for ReadBackend<'a> {
    #[inline]
    fn from(mmap: &'a memmap2::Mmap) -> Self {
        ReadBackend::Mapped(mmap)
    }
}
