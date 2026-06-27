use std::fmt;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::Path;
use std::sync::Mutex;

/// A checked byte range in a media source.
///
/// Offsets and lengths are modeled as `u64` at the source boundary so large
/// files cannot silently wrap `usize` arithmetic. Conversion to `usize` is
/// delayed until a concrete in-memory window is materialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub offset: u64,
    pub len: u64,
}

impl ByteRange {
    pub fn new(offset: u64, len: u64) -> Result<Self, ReadAtError> {
        offset
            .checked_add(len)
            .ok_or(ReadAtError::RangeOverflow { offset, len })
            .map(|_| Self { offset, len })
    }

    pub fn from_usize(offset: usize, len: usize) -> Result<Self, ReadAtError> {
        Self::new(offset as u64, len as u64)
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn end_exclusive(&self) -> u64 {
        self.offset + self.len
    }

    fn len_usize(&self) -> Result<usize, ReadAtError> {
        usize::try_from(self.len).map_err(|_| ReadAtError::RangeTooLarge { len: self.len })
    }
}

/// Typed failures for random-access reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadAtError {
    RangeOverflow { offset: u64, len: u64 },
    RangeTooLarge { len: u64 },
    OffsetOutOfBounds { offset: u64, source_len: u64 },
    RangeOutOfBounds { offset: u64, len: u64, source_len: u64 },
    DestinationTooSmall { requested: usize, available: usize },
    UnavailableWindow,
    Io { message: String },
}

impl From<std::io::Error> for ReadAtError {
    fn from(value: std::io::Error) -> Self {
        ReadAtError::Io { message: value.to_string() }
    }
}

impl fmt::Display for ReadAtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadAtError::RangeOverflow { offset, len } => {
                write!(f, "byte range overflows: offset={offset}, len={len}")
            }
            ReadAtError::RangeTooLarge { len } => {
                write!(f, "byte range length does not fit in memory: len={len}")
            }
            ReadAtError::OffsetOutOfBounds { offset, source_len } => {
                write!(f, "offset {offset} is outside source length {source_len}")
            }
            ReadAtError::RangeOutOfBounds { offset, len, source_len } => {
                write!(f, "range offset={offset}, len={len} exceeds source length {source_len}")
            }
            ReadAtError::DestinationTooSmall { requested, available } => {
                write!(f, "destination too small: requested={requested}, available={available}")
            }
            ReadAtError::UnavailableWindow => write!(f, "requested byte window is unavailable"),
            ReadAtError::Io { message } => write!(f, "source I/O error: {message}"),
        }
    }
}

impl std::error::Error for ReadAtError {}

/// Optional source-level counters for random-access backends.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SourceStats {
    pub read_at_calls: u64,
    pub window_at_calls: u64,
    pub bytes_requested: u64,
    pub bytes_returned: u64,
    pub max_request_len: u64,
}

/// Canonical random-access/windowed source boundary.
pub trait MediaReadAt: std::fmt::Debug {
    /// Total length of the source in bytes.
    fn len_u64(&self) -> u64;

    fn is_empty(&self) -> bool {
        self.len_u64() == 0
    }

    /// Copy exactly `range.len` bytes into `dst`.
    fn read_at(&self, range: ByteRange, dst: &mut [u8]) -> Result<usize, ReadAtError>;

    /// Borrow exactly `range.len` bytes from the source.
    fn window_at(&self, range: ByteRange) -> Result<&[u8], ReadAtError>;

    /// Borrow up to `range.len` bytes. The offset must still be in bounds.
    fn window_at_partial(&self, range: ByteRange) -> Result<&[u8], ReadAtError> {
        self.window_at(range)
    }

    /// Return the complete contiguous source only for compatibility paths.
    fn as_contiguous(&self) -> Option<&[u8]> {
        None
    }

    fn stats(&self) -> SourceStats {
        SourceStats::default()
    }
}

/// A legacy source of bytes addressable at arbitrary offsets.
///
/// # Implementors
///
/// | Type | I/O model | Use when |
/// |---|---|---|
/// | [`SliceBackend`] | Zero-copy `&[u8]` borrow | Bytes already in memory |
/// | [`MmapBackend`] | Zero-copy mmap, OS pages on demand | Local files of any size |
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

/// Seekable file-backed media source.
///
/// This backend can copy byte ranges with [`MediaReadAt::read_at`], but it does
/// not expose borrowed windows. A future sliding-window backend can build on
/// the same typed range/error contract while adding cache-backed `window_at`.
#[derive(Debug)]
pub struct FileBackend {
    file: Mutex<std::fs::File>,
    len: u64,
}

impl FileBackend {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, ReadAtError> {
        let file = std::fs::File::open(path).map_err(ReadAtError::from)?;
        Self::from_file(file)
    }

    pub fn from_file(file: std::fs::File) -> Result<Self, ReadAtError> {
        let len = file.metadata().map_err(ReadAtError::from)?.len();
        Ok(Self { file: Mutex::new(file), len })
    }
}

impl MediaReadAt for FileBackend {
    fn len_u64(&self) -> u64 {
        self.len
    }

    fn read_at(&self, range: ByteRange, dst: &mut [u8]) -> Result<usize, ReadAtError> {
        let requested_len = range.len_usize()?;
        if dst.len() < requested_len {
            return Err(ReadAtError::DestinationTooSmall {
                requested: requested_len,
                available: dst.len(),
            });
        }
        if range.end_exclusive() > self.len {
            return Err(ReadAtError::RangeOutOfBounds {
                offset: range.offset,
                len: range.len,
                source_len: self.len,
            });
        }
        let mut file = self
            .file
            .lock()
            .map_err(|_| ReadAtError::Io { message: "file backend mutex poisoned".to_string() })?;
        file.seek(SeekFrom::Start(range.offset)).map_err(ReadAtError::from)?;
        file.read_exact(&mut dst[..requested_len]).map_err(ReadAtError::from)?;
        Ok(requested_len)
    }

    fn window_at(&self, _range: ByteRange) -> Result<&[u8], ReadAtError> {
        Err(ReadAtError::UnavailableWindow)
    }
}

/// Borrowed in-memory media source.
#[derive(Debug, Clone, Copy)]
pub struct SliceBackend<'a> {
    bytes: &'a [u8],
}

impl<'a> SliceBackend<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    pub fn as_slice(&self) -> &'a [u8] {
        self.bytes
    }
}

impl MediaReadAt for SliceBackend<'_> {
    fn len_u64(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, range: ByteRange, dst: &mut [u8]) -> Result<usize, ReadAtError> {
        let source = ReadBackend::Slice(self.bytes);
        source.read_at(range, dst)
    }

    fn window_at(&self, range: ByteRange) -> Result<&[u8], ReadAtError> {
        let bounds = exact_bounds(range, self.bytes.len())?;
        Ok(&self.bytes[bounds])
    }

    fn window_at_partial(&self, range: ByteRange) -> Result<&[u8], ReadAtError> {
        let bounds = partial_bounds(range, self.bytes.len())?;
        Ok(&self.bytes[bounds])
    }

    fn as_contiguous(&self) -> Option<&[u8]> {
        Some(self.bytes)
    }
}

/// Borrowed memory-mapped media source.
#[cfg(feature = "mmap")]
#[derive(Debug, Clone, Copy)]
pub struct MmapBackend<'a> {
    mmap: &'a memmap2::Mmap,
}

#[cfg(feature = "mmap")]
impl<'a> MmapBackend<'a> {
    pub fn new(mmap: &'a memmap2::Mmap) -> Self {
        Self { mmap }
    }

    pub fn as_slice(&self) -> &'a [u8] {
        self.mmap.as_ref()
    }
}

#[cfg(feature = "mmap")]
impl MediaReadAt for MmapBackend<'_> {
    fn len_u64(&self) -> u64 {
        self.mmap.len() as u64
    }

    fn read_at(&self, range: ByteRange, dst: &mut [u8]) -> Result<usize, ReadAtError> {
        let source = ReadBackend::Mapped(self.mmap);
        source.read_at(range, dst)
    }

    fn window_at(&self, range: ByteRange) -> Result<&[u8], ReadAtError> {
        let bytes = self.mmap.as_ref();
        let bounds = exact_bounds(range, bytes.len())?;
        Ok(&bytes[bounds])
    }

    fn window_at_partial(&self, range: ByteRange) -> Result<&[u8], ReadAtError> {
        let bytes = self.mmap.as_ref();
        let bounds = partial_bounds(range, bytes.len())?;
        Ok(&bytes[bounds])
    }

    fn as_contiguous(&self) -> Option<&[u8]> {
        Some(self.mmap.as_ref())
    }
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

fn exact_bounds(range: ByteRange, source_len: usize) -> Result<Range<usize>, ReadAtError> {
    bounds(range, source_len, false)
}

fn partial_bounds(range: ByteRange, source_len: usize) -> Result<Range<usize>, ReadAtError> {
    bounds(range, source_len, true)
}

fn bounds(
    range: ByteRange,
    source_len: usize,
    allow_partial: bool,
) -> Result<Range<usize>, ReadAtError> {
    let source_len_u64 = source_len as u64;
    let start = usize::try_from(range.offset).map_err(|_| ReadAtError::OffsetOutOfBounds {
        offset: range.offset,
        source_len: source_len_u64,
    })?;
    if start > source_len || (start == source_len && !range.is_empty()) {
        return Err(ReadAtError::OffsetOutOfBounds {
            offset: range.offset,
            source_len: source_len_u64,
        });
    }
    let requested_len = range.len_usize()?;
    let requested_end = start
        .checked_add(requested_len)
        .ok_or(ReadAtError::RangeOverflow { offset: range.offset, len: range.len })?;
    if requested_end <= source_len {
        Ok(start..requested_end)
    } else if allow_partial {
        Ok(start..source_len)
    } else {
        Err(ReadAtError::RangeOutOfBounds {
            offset: range.offset,
            len: range.len,
            source_len: source_len_u64,
        })
    }
}

impl MediaReadAt for ReadBackend<'_> {
    #[inline]
    fn len_u64(&self) -> u64 {
        self.as_slice().len() as u64
    }

    fn read_at(&self, range: ByteRange, dst: &mut [u8]) -> Result<usize, ReadAtError> {
        let requested_len = range.len_usize()?;
        if dst.len() < requested_len {
            return Err(ReadAtError::DestinationTooSmall {
                requested: requested_len,
                available: dst.len(),
            });
        }
        let window = self.window_at(range)?;
        dst[..requested_len].copy_from_slice(window);
        Ok(requested_len)
    }

    fn window_at(&self, range: ByteRange) -> Result<&[u8], ReadAtError> {
        let bounds = exact_bounds(range, self.as_slice().len())?;
        Ok(&self.as_slice()[bounds])
    }

    fn window_at_partial(&self, range: ByteRange) -> Result<&[u8], ReadAtError> {
        let bounds = partial_bounds(range, self.as_slice().len())?;
        Ok(&self.as_slice()[bounds])
    }

    fn as_contiguous(&self) -> Option<&[u8]> {
        Some(self.as_slice())
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
        let range = ByteRange::from_usize(offset, len).ok()?;
        self.window_at(range).ok()
    }
}

impl<'a> From<&'a [u8]> for ReadBackend<'a> {
    #[inline]
    fn from(slice: &'a [u8]) -> Self {
        ReadBackend::Slice(slice)
    }
}

impl<'a> From<SliceBackend<'a>> for ReadBackend<'a> {
    #[inline]
    fn from(slice: SliceBackend<'a>) -> Self {
        ReadBackend::Slice(slice.as_slice())
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

#[cfg(feature = "mmap")]
impl<'a> From<MmapBackend<'a>> for ReadBackend<'a> {
    #[inline]
    fn from(mmap: MmapBackend<'a>) -> Self {
        ReadBackend::Mapped(mmap.mmap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_range_rejects_offset_overflow() {
        assert_eq!(
            ByteRange::new(u64::MAX, 1),
            Err(ReadAtError::RangeOverflow { offset: u64::MAX, len: 1 })
        );
    }

    #[test]
    fn read_backend_windows_exact_ranges() {
        let bytes = [0, 1, 2, 3, 4, 5];
        let source = ReadBackend::Slice(&bytes);

        assert_eq!(source.len_u64(), 6);
        assert_eq!(source.window_at(ByteRange::new(2, 3).unwrap()).unwrap(), &[2, 3, 4]);
        assert_eq!(
            source.window_at(ByteRange::new(4, 4).unwrap()),
            Err(ReadAtError::RangeOutOfBounds { offset: 4, len: 4, source_len: 6 })
        );
    }

    #[test]
    fn read_backend_windows_partial_ranges() {
        let bytes = [0, 1, 2, 3, 4, 5];
        let source = ReadBackend::Slice(&bytes);

        assert_eq!(source.window_at_partial(ByteRange::new(4, 4).unwrap()).unwrap(), &[4, 5]);
        assert_eq!(
            source.window_at_partial(ByteRange::new(6, 1).unwrap()),
            Err(ReadAtError::OffsetOutOfBounds { offset: 6, source_len: 6 })
        );
    }

    #[test]
    fn read_backend_copies_exact_ranges() {
        let bytes = [10, 11, 12, 13];
        let source = ReadBackend::Slice(&bytes);
        let mut out = [0u8; 2];

        assert_eq!(source.read_at(ByteRange::new(1, 2).unwrap(), &mut out), Ok(2));
        assert_eq!(out, [11, 12]);
        assert_eq!(
            source.read_at(ByteRange::new(1, 3).unwrap(), &mut out),
            Err(ReadAtError::DestinationTooSmall { requested: 3, available: 2 })
        );
    }

    #[test]
    fn slice_backend_matches_read_backend_semantics() {
        let bytes = [20, 21, 22, 23, 24];
        let source = SliceBackend::new(&bytes);

        assert_eq!(source.len_u64(), 5);
        assert_eq!(source.window_at(ByteRange::new(1, 3).unwrap()).unwrap(), &[21, 22, 23]);

        let mut copied = [0; 2];
        assert_eq!(source.read_at(ByteRange::new(3, 2).unwrap(), &mut copied), Ok(2));
        assert_eq!(copied, [23, 24]);
    }

    #[test]
    fn file_backend_copies_exact_ranges_without_windows() {
        use std::io::Write;

        let path = std::env::temp_dir().join(format!(
            "revelo-core-file-backend-{}-{}.bin",
            std::process::id(),
            "range"
        ));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(&[50, 51, 52, 53, 54, 55]).unwrap();
        file.sync_all().unwrap();
        drop(file);

        let source = FileBackend::open(&path).unwrap();
        let mut copied = [0; 3];

        assert_eq!(source.len_u64(), 6);
        assert_eq!(source.read_at(ByteRange::new(2, 3).unwrap(), &mut copied), Ok(3));
        assert_eq!(copied, [52, 53, 54]);
        assert_eq!(
            source.window_at(ByteRange::new(0, 1).unwrap()),
            Err(ReadAtError::UnavailableWindow)
        );
        assert_eq!(
            source.read_at(ByteRange::new(5, 2).unwrap(), &mut copied),
            Err(ReadAtError::RangeOutOfBounds { offset: 5, len: 2, source_len: 6 })
        );

        std::fs::remove_file(path).unwrap();
    }
}
