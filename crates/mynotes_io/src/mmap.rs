use crate::enums::{MmapFileError, MmapFileResult};
use memmap2::{Mmap, MmapOptions};
use std::cmp::min;
use std::fs::File;
use std::path::{Path, PathBuf};

pub struct MmapFile {
    /// Temporary solution to prevent external modification,
    /// resulting in UB or undefined behavior.
    /// This will hold the lock on the file, preventing
    /// external programs to modify the file.
    _file: Option<File>,
    mmap: Option<Mmap>,
    path: PathBuf,
}

impl MmapFile {
    pub fn new() -> MmapFileResult<Self> {
        Ok(Self {
            _file: None,
            mmap: None,
            path: PathBuf::new(),
        })
    }

    pub fn open_file(file_path: impl AsRef<Path>) -> MmapFileResult<Self> {
        if file_path.as_ref().is_dir() {
            return Err(MmapFileError::FilePathIsDirectory);
        }

        let path_buf = file_path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let mmap = unsafe { MmapOptions::new().map(&file)? };

        Ok(Self {
            _file: Some(file),
            mmap: Some(mmap),
            path: path_buf,
        })
    }

    pub fn get(&self, index: usize, length: usize) -> Option<&[u8]> {
        let mmap = self.mmap.as_ref()?;
        let len = mmap.len();

        if index >= len {
            return None;
        }

        let end = index + length;

        if end >= len {
            return None;
        }

        mmap.get(index..end)
    }

    /// # Safety
    ///
    /// clamps the resulting `end` index if
    /// it exceeds the length of this `mmap`
    pub unsafe fn get_clamped(&self, index: usize, length: usize) -> &[u8] {
        let Some(mmap) = self.mmap.as_ref() else {
            return &[];
        };
        let len = mmap.len();

        if index >= len {
            return &[];
        }

        let end = min(index + length, len);

        unsafe { mmap.get_unchecked(index..end) }
    }

    ///
    #[inline]
    pub fn len(&self) -> usize {
        self.mmap.as_ref().map_or(0, |m| m.len())
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn get_path(&self) -> &Path {
        &self.path
    }
}

impl Drop for MmapFile {
    fn drop(&mut self) {
        self.mmap.take();
        self._file.take();
    }
}
