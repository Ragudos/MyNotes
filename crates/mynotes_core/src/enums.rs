use std::sync::Arc;

use mynotes_io::mmap::MmapFile;

pub enum ZeroCopyChunk {
    Mmap {
        mmap: Arc<MmapFile>,
        index: usize,
        end_index: usize,
    },
    AppendBuffer {
        buffer: Arc<Vec<u8>>,
        index: usize,
        end_index: usize,
    },
}

impl AsRef<[u8]> for ZeroCopyChunk {
    fn as_ref(&self) -> &[u8] {
        match self {
            ZeroCopyChunk::Mmap {
                mmap,
                index,
                end_index,
            } => unsafe { &mmap.get_clamped(*index, *end_index) },
            ZeroCopyChunk::AppendBuffer {
                buffer,
                index,
                end_index,
            } => &buffer[*index..*end_index],
        }
    }
}
