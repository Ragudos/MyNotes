#[derive(Debug)]
pub struct LineRangeIter<'node> {
    /// Stack tracks: (Node Reference, Index of next child/line to visit)
    pub stack: Vec<(&'node crate::line_index::node::Node, usize)>,
    pub current_line_idx: usize,
    pub end_line_idx: usize,
    pub current_abs_idx: u64,
}

impl<'node> Iterator for LineRangeIter<'node> {
    type Item = (usize, std::ops::Range<u64>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_line_idx >= self.end_line_idx {
            return None;
        }

        todo!();
    }
}
