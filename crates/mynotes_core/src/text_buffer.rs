// Assuming these exist in your crate
use mynotes_io::enums::MmapFileError;
use std::fs::{File, OpenOptions, rename};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::{io, thread};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TextBufferError {
    #[error("IO Error: {0}")]
    IOError(#[from] io::Error),
    #[error("Memory Map Error: {0}")]
    MmapFileError(#[from] MmapFileError),
    #[error("Serialization Error: {0}")]
    SerializationError(String),
    #[error("Buffer has no associated file path. Use save_as().")]
    NoFilePath,
    #[error("Swap file corruption: {0}")]
    SwapCorruption(String),
}

pub type TextBufferResult<T> = Result<T, TextBufferError>;

/// Messages sent from the background saving thread to the main/UI thread.
#[derive(Debug)]
pub enum SaveProgress {
    /// Save operation has started
    Started { total_bytes: usize },
    /// A chunk of bytes has been successfully written
    Written { bytes: usize },
    /// Save completed successfully. Contains the path to the newly saved file.
    Finished { path: PathBuf },
    /// An error occurred during the background save
    Error(TextBufferError),
}

pub type ProgressSender = Sender<SaveProgress>;

/// Represents a single change in the document for the Write-Ahead Log.
#[derive(Debug)]
pub enum JournalOp {
    Insert {
        offset: usize,
        add_buf_offset: usize,
        len: usize,
    },
    Delete {
        offset: usize,
        len: usize,
    },
}

impl JournalOp {
    /// Serializes the operation into a compact byte array for fast disk logging.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Implementation note: In a real app, use `bincode` or manually pack into [u8; 25]
        match self {
            JournalOp::Insert {
                offset,
                add_buf_offset,
                len,
            } => {
                let mut buf = vec![0u8]; // 0 = Insert
                buf.extend_from_slice(&offset.to_le_bytes());
                buf.extend_from_slice(&add_buf_offset.to_le_bytes());
                buf.extend_from_slice(&len.to_le_bytes());
                buf
            }
            JournalOp::Delete { offset, len } => {
                let mut buf = vec![1u8]; // 1 = Delete
                buf.extend_from_slice(&offset.to_le_bytes());
                buf.extend_from_slice(&len.to_le_bytes());
                buf
            }
        }
    }
}

#[derive(Debug)]
pub struct SwapManager {
    add_file: Option<File>,
    log_file: Option<File>,
    pub base_path: PathBuf,
}

impl SwapManager {
    /// # Purpose
    /// Initializes a new SwapManager, creating append-only `.swp.add` and `.swp.log` files
    /// at the specified base path.
    // # Parameters
    /// - **`base_path`**: The path prefix for the swap files (e.g., `~/.local/share/myapp/sessions/uuid` or `./main.rs`).
    // # Panics
    /// - **`None`**: This function does not intentionally panic.
    /// # Errors
    /// - **`TextBufferError::IOError`**: Happens when the OS denies file creation or appending rights.
    /// # Returns
    /// - **`TextBufferResult<Self>`**: A new instance of `SwapManager`.
    pub fn new<P: AsRef<Path>>(base_path: P) -> TextBufferResult<Self> {
        let path = base_path.as_ref();

        let add_path = path.with_extension("swp.add");
        let log_path = path.with_extension("swp.log");

        let add_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(add_path)?;
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;

        Ok(Self {
            add_file: Some(add_file),
            log_file: Some(log_file),
            base_path: path.to_path_buf(),
        })
    }

    /// # Purpose
    /// Appends text to the add buffer file and logs the insert operation to the journal.
    /// # Parameters
    /// - **`offset`**: The position in the document where text is inserted.
    /// - **`add_buf_offset`**: The position in the add_buf where this text begins.
    /// - **`text`**: The actual bytes being inserted.
    /// # Panics
    /// - **`None`**: This function does not intentionally panic.
    /// # Errors
    /// - **`TextBufferError::IOError`**: Happens if writing to the disk fails (e.g., disk full).
    /// # Returns
    /// - **`TextBufferResult<()>`**: Indicates success.
    pub fn log_insert(
        &mut self,
        offset: usize,
        add_buf_offset: usize,
        text: &[u8],
    ) -> TextBufferResult<()> {
        if let Some(f) = &mut self.add_file {
            f.write_all(text)?;
        }

        let op = JournalOp::Insert {
            offset,
            add_buf_offset,
            len: text.len(),
        };
        if let Some(f) = &mut self.log_file {
            f.write_all(&op.to_bytes())?;
        }
        Ok(())
    }

    /// # Purpose
    /// Cleans up the swap files from the disk. Used after a successful save or clean exit.
    /// # Parameters
    /// - **`None`**
    /// # Panics
    /// - **`None`**: This function does not intentionally panic.
    /// # Errors
    /// - **`TextBufferError::IOError`**: Happens if file deletion fails.
    /// # Returns
    /// - **`TextBufferResult<()>`**: Indicates success.
    pub fn clear_swaps(&mut self) -> TextBufferResult<()> {
        self.add_file = None;
        self.log_file = None;
        let add_path = self.base_path.with_extension("swp.add");
        let log_path = self.base_path.with_extension("swp.log");

        let _ = std::fs::remove_file(add_path);
        let _ = std::fs::remove_file(log_path);
        Ok(())
    }
}

// Assuming you have a way to clone the tree/state or extract an iterator of chunks from PieceTable
// For this example, we assume we can get an Iterator of `&[u8]` chunks representing the document.

pub struct BackgroundSaver;

impl BackgroundSaver {
    /// # Purpose
    /// Spawns a background thread to safely write document contents to a temporary file
    /// and atomically rename it, reporting progress via a channel.
    /// # Parameters
    /// - **`target_path`**: The final destination path for the saved file.
    /// - **`chunks`**: A vector of byte arrays representing the full document text. (Extracted from PieceTable).
    /// - **`progress_tx`**: The channel sender to report progress to the UI.
    /// # Panics
    /// - **`None`**: Thread panics are isolated; main application will not crash.
    /// # Errors
    /// - **`None`**: Errors are sent through the `progress_tx` channel instead of returned.
    /// # Returns
    /// - **`()`**: Spawns a thread and returns immediately.
    pub fn save_async(
        target_path: PathBuf,
        chunks: Vec<Vec<u8>>, // In a real app, pass a cloned snapshot of the PieceTable and read from Mmap/AddBuf directly
        progress_tx: ProgressSender,
    ) {
        thread::spawn(move || {
            let total_bytes: usize = chunks.iter().map(|c| c.len()).sum();

            if progress_tx
                .send(SaveProgress::Started { total_bytes })
                .is_err()
            {
                return; // Receiver dropped, abort reporting
            }

            // 1. Create adjacent temporary file
            let file_name = target_path.file_name().unwrap_or_default();
            let tmp_file_name = format!(".{}.tmp", file_name.to_string_lossy());
            let tmp_path = target_path.with_file_name(tmp_file_name);

            let mut tmp_file = match File::create(&tmp_path) {
                Ok(f) => f,
                Err(e) => {
                    let _ = progress_tx.send(SaveProgress::Error(TextBufferError::IOError(e)));
                    return;
                }
            };

            // 2. Write chunks and report progress
            for chunk in chunks {
                if let Err(e) = tmp_file.write_all(&chunk) {
                    let _ = progress_tx.send(SaveProgress::Error(TextBufferError::IOError(e)));
                    return;
                }
                let _ = progress_tx.send(SaveProgress::Written { bytes: chunk.len() });
            }

            // 3. Sync to physical disk
            if let Err(e) = tmp_file.sync_all() {
                let _ = progress_tx.send(SaveProgress::Error(TextBufferError::IOError(e)));
                return;
            }

            // 4. Atomic Rename
            if let Err(e) = rename(&tmp_path, &target_path) {
                let _ = progress_tx.send(SaveProgress::Error(TextBufferError::IOError(e)));
                return;
            }

            // 5. Sync Directory (Unix)
            #[cfg(target_family = "unix")]
            if let Some(parent) = target_path.parent()
                && let Ok(dir) = File::open(parent)
            {
                let _ = dir.sync_all();
            }

            let _ = progress_tx.send(SaveProgress::Finished { path: target_path });
        });
    }
}
