use thiserror::Error;

use crate::{
    cursor::Cursor,
    text_buffer::{TextBuffer, TextBufferError},
};

#[derive(Error, Debug)]
pub enum DocumentError {
    #[error("Failed to initialize text buffer: {0}")]
    TextBufferError(#[from] TextBufferError),
}

/// TODO: Cursor Manager
#[derive(Debug)]
pub struct Document {
    text_buffer: TextBuffer,
    cursor: Cursor,
}

impl Document {
    pub fn new() -> Result<Self, DocumentError> {
        Ok(Self {
            text_buffer: TextBuffer::new()?,
            cursor: Cursor::new(0, 0),
        })
    }
}
