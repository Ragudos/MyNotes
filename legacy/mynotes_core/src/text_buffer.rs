use std::{borrow::Borrow, io, sync::Arc};

use mynotes_io::{create_next_untitled_file, enums::MmapFileError, mmap::MmapFile};
use thiserror::Error;

use crate::{
    enums::ZeroCopyChunk,
    line_ending::LineEnding,
    line_tracker::LineTracker,
    piece_table::{BufferKind, Piece, PieceTable},
};

#[derive(Error, Debug)]
pub enum TextBufferError {
    #[error("Failed to create memory-mapped file: {0}")]
    MmapFileCreationError(#[from] MmapFileError),
    #[error("Failed to create temporary file: {0}")]
    IoError(#[from] io::Error),
}

#[derive(Debug)]
pub struct TextBuffer {
    mmap_file: Arc<MmapFile>,
    append_buffer: Arc<Vec<u8>>,
    piece_table: PieceTable,
    line_tracker: LineTracker,
    line_ending: LineEnding,
    is_dirty: bool,
}

impl TextBuffer {
    pub fn new() -> Result<Self, TextBufferError> {
        let (_, file_path) = create_next_untitled_file()?;
        let mmap_file = Arc::new(MmapFile::open_file(file_path)?);
        let append_buffer = Arc::new(Vec::new());
        let piece_table = PieceTable::new();
        let line_tracker = LineTracker::new();
        let line_ending = LineEnding::from_current_platform();

        Ok(Self {
            mmap_file,
            append_buffer,
            piece_table,
            line_tracker,
            line_ending,
            is_dirty: false,
        })
    }

    pub fn empty_for_test() -> Self {
        Self {
            mmap_file: Arc::new(MmapFile::new().unwrap()),
            append_buffer: Arc::new(Vec::new()),
            piece_table: PieceTable::new(),
            line_tracker: LineTracker::new(),
            line_ending: LineEnding::from_current_platform(),
            is_dirty: false,
        }
    }

    /// Instantly opens a file of any size using memory mapping.
    pub fn open(file_path: impl AsRef<std::path::Path>) -> Result<Self, TextBufferError> {
        let mmap_file = Arc::new(MmapFile::open_file(file_path)?);
        let file_len = mmap_file.len();

        let mut piece_table = PieceTable::new();

        if file_len > 0 {
            // Insert the entire file as a single piece.
            // Adjust this depending on your PieceTable's exact insert signature!
            // E.g., tree.insert(0, Piece { buffer_kind: BufferKind::Original, start: 0, end: file_len as u64 })
            let initial_piece = Piece {
                buffer_kind: BufferKind::Original,
                start: 0,
                end: file_len as u64,
            };
            // Assuming your tree inserts by doc_offset
            piece_table.tree.insert(0, initial_piece).unwrap();
        }

        Ok(Self {
            mmap_file,
            append_buffer: Arc::new(Vec::new()),
            piece_table,
            line_tracker: LineTracker::new(), // Note: You'll eventually want to parse line breaks here or lazily
            line_ending: LineEnding::from_current_platform(),
            is_dirty: false,
        })
    }

    /// Safely reloads the file from disk after an external modification is detected.
    pub fn reload_from_disk(&mut self) -> Result<(), TextBufferError> {
        // 1. If the user has unsaved changes in the append_buffer, you usually
        // prompt them here ("File changed on disk, overwrite your changes?").
        // If we want to force-reload, we just clear everything.

        let current_path = self.mmap_file.get_path().to_path_buf();

        // 2. CRITICAL: We must drop the old MmapFile to release the OS locks
        // and prevent a SIGBUS crash before we try to read the new state.
        // Replacing it temporarily with an empty one works perfectly.
        self.mmap_file = Arc::new(MmapFile::new().unwrap());

        // 3. Now safely map the newly modified file
        let new_mmap = Arc::new(MmapFile::open_file(&current_path)?);
        let new_file_len = new_mmap.len() as u64;

        self.mmap_file = new_mmap;
        self.append_buffer = Arc::new(Vec::new()); // Wipe typing history
        self.piece_table = PieceTable::new(); // Wipe the old tree
        self.is_dirty = false;

        // 4. Insert the new file as a single clean piece
        if new_file_len > 0 {
            let initial_piece = Piece {
                buffer_kind: BufferKind::Original,
                start: 0,
                end: new_file_len,
            };
            self.piece_table.tree.insert(0, initial_piece).unwrap();
        }

        Ok(())
    }

    pub fn insert(&mut self, index: usize, text: &[u8]) {}

    pub fn delete(&mut self, index: usize, length: usize) {}

    pub fn iter(&self) -> TextBufferChunkIter<impl Iterator<Item = &Piece>> {
        TextBufferChunkIter {
            mmap_file: Arc::clone(&self.mmap_file),
            append_buffer: Arc::clone(&self.append_buffer),
            piece_iter: self.piece_table.iter(),
        }
    }

    /// 2. Snapshot iterator for the Detached Background Save Thread
    pub fn into_save_iter(
        &self,
    ) -> TextBufferChunkIter<impl Iterator<Item = Piece> + Send + 'static> {
        // Instantly copies the 24-byte structs into a Vec
        let pieces_snapshot = self.piece_table.get_all_pieces();

        TextBufferChunkIter {
            mmap_file: Arc::clone(&self.mmap_file),
            append_buffer: Arc::clone(&self.append_buffer),
            // .into_iter() takes ownership, making it 'static and safe for threads
            piece_iter: pieces_snapshot.into_iter(),
        }
    }

    pub fn get_line_ending(&self) -> &LineEnding {
        &self.line_ending
    }

    pub fn get_is_dirty(&self) -> bool {
        self.is_dirty
    }
}

pub struct TextBufferChunkIter<I> {
    mmap_file: Arc<MmapFile>,
    append_buffer: Arc<Vec<u8>>,
    piece_iter: I,
}

impl<I, P> Iterator for TextBufferChunkIter<I>
where
    I: Iterator<Item = P>,
    P: Borrow<Piece>,
{
    type Item = ZeroCopyChunk;

    fn next(&mut self) -> Option<Self::Item> {
        let piece = self.piece_iter.next()?;
        let piece = piece.borrow();

        match piece.buffer_kind {
            BufferKind::Original => Some(ZeroCopyChunk::Mmap {
                mmap: Arc::clone(&self.mmap_file),
                index: piece.start as usize,
                end_index: piece.end as usize,
            }),
            BufferKind::Add => Some(ZeroCopyChunk::AppendBuffer {
                buffer: Arc::clone(&self.append_buffer),
                index: piece.start as usize,
                end_index: piece.end as usize,
            }),
        }
    }
}
