#[derive(Debug)]
pub struct MmapFile {
    // Wrap the internal OS resources in Option
    _file: Option<std::fs::File>,
    mmap: Option<memmap2::Mmap>,
    path: std::path::PathBuf,
}

impl MmapFile {
    pub fn open(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = std::fs::File::open(&path_buf)?;

        // SAFETY: Same as before
        let mmap = unsafe { memmap2::Mmap::map(&file)? };

        Ok(Self {
            _file: Some(file),
            mmap: Some(mmap),
            path: path_buf,
        })
    }

    /// NEW: Explicitly drops the memory map and file handle, releasing OS locks.
    pub fn close(&mut self) {
        // Taking the values out of the Option drops them instantly.
        self.mmap = None;
        self._file = None;
    }

    #[inline]
    #[must_use]
    pub fn get_bytes_exact(&self, start: usize, length: usize) -> Option<&[u8]> {
        // If the mmap is None (closed), the `?` operator cleanly returns None early.
        let mmap = self.mmap.as_ref()?;
        let end = start.checked_add(length)?;

        mmap.get(start..end)
    }

    #[inline]
    #[must_use]
    pub fn get_bytes_clamped(&self, start: usize, length: usize) -> &[u8] {
        // If closed, safely return an empty slice
        let Some(mmap) = &self.mmap else {
            return &[];
        };

        let len = mmap.len();
        if start >= len {
            return &[];
        }

        let end = std::cmp::min(start.saturating_add(length), len);
        &mmap[start..end]
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        // Deref the Option, or default to an empty slice
        self.mmap.as_deref().unwrap_or(&[])
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.mmap.as_ref().map_or(0, |m| m.len())
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}
