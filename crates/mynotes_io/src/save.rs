use std::{
    fs::{File, rename},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::mpsc::Sender,
    thread::spawn,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SaveError {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
}

#[derive(Debug)]
pub enum SaveProgress {
    Started { total_bytes: usize },
    Written { bytes: usize },
    Saving,
    CleaningUp,
    Finished { path: PathBuf },
    Error(SaveError),
}

pub type ProgressSender = Sender<SaveProgress>;

pub fn save_async<I, C>(target_path: PathBuf, chunks: I, progress_tx: ProgressSender)
where
    I: Iterator<Item = C> + Send + 'static,
    C: AsRef<[u8]> + Send,
{
    spawn(move || {
        let file_name = target_path.file_name().unwrap_or_default();
        let tmp_file_name = format!("{}.tmp", file_name.to_string_lossy());
        let tmp_path = target_path.with_file_name(&tmp_file_name);

        let tmp_file = match File::create(&tmp_path) {
            Ok(f) => f,
            Err(e) => {
                let _ = progress_tx.send(SaveProgress::Error(SaveError::IoError(e)));
                return;
            }
        };

        // BufWriter handles the chunk batching for high performance
        let mut writer = BufWriter::new(tmp_file);

        for chunk in chunks {
            let bytes = chunk.as_ref();
            if let Err(err) = writer.write_all(bytes) {
                let _ = progress_tx.send(SaveProgress::Error(SaveError::IoError(err)));
                return;
            }

            let _ = progress_tx.send(SaveProgress::Written { bytes: bytes.len() });
        }

        let _ = progress_tx.send(SaveProgress::Saving);

        if let Err(err) = writer.flush() {
            let _ = progress_tx.send(SaveProgress::Error(SaveError::IoError(err)));
            return;
        }

        // Extract the file and force the physical disk write
        let tmp_file = match writer.into_inner() {
            Ok(f) => f,
            Err(e) => {
                let _ = progress_tx.send(SaveProgress::Error(SaveError::IoError(e.into_error())));
                return;
            }
        };

        let _ = progress_tx.send(SaveProgress::CleaningUp);

        if let Err(err) = tmp_file.sync_all() {
            let _ = progress_tx.send(SaveProgress::Error(SaveError::IoError(err)));
            return;
        }

        // Atomic swap
        if let Err(err) = rename(&tmp_path, &target_path) {
            let _ = progress_tx.send(SaveProgress::Error(SaveError::IoError(err)));
            return;
        }

        #[cfg(target_family = "unix")]
        if let Some(parent) = target_path.parent() {
            if let Ok(dir) = File::open(parent) {
                let _ = dir.sync_all();
            }
        }

        let _ = progress_tx.send(SaveProgress::Finished { path: target_path });
    });
}
