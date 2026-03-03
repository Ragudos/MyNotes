use std::io;
use thiserror::Error;

pub type MmapFileResult<T> = Result<T, MmapFileError>;

#[derive(Error, Debug)]
pub enum MmapFileError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error("The provided file path is a directory. It must be a file.")]
    FilePathIsDirectory,
}
