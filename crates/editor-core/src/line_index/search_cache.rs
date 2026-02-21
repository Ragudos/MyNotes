#[derive(Clone, Copy, Debug)]
pub struct SearchCache {
    pub line_idx: usize,
    pub byte_offset: u64,
}
