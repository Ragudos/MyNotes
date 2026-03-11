use crate::btree::{MeasuredBTree, MeasuredBTreeData, MeasuredBTreeNode};
use num_traits::SaturatingSub;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub, SubAssign};

pub const MAX_CHUNK_LINES: u8 = 16;

/// Global Measure: 64-bit math guarantees safety for >4GB files on ANY architecture.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Ord, PartialOrd)]
pub struct LineTrackerSummary {
    pub byte_count: u64,
    pub line_count: u64,
}

impl LineTrackerSummary {
    #[inline]
    #[must_use]
    pub fn new(byte_count: u64, line_count: u64) -> Self {
        Self {
            byte_count,
            line_count,
        }
    }
}

impl Add for LineTrackerSummary {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(
            self.byte_count + rhs.byte_count,
            self.line_count + rhs.line_count,
        )
    }
}

impl AddAssign for LineTrackerSummary {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for LineTrackerSummary {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(
            self.byte_count - rhs.byte_count,
            self.line_count - rhs.line_count,
        )
    }
}

impl SubAssign for LineTrackerSummary {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl SaturatingSub for LineTrackerSummary {
    fn saturating_sub(&self, rhs: &Self) -> Self {
        Self::new(
            self.byte_count.saturating_sub(rhs.byte_count),
            self.line_count.saturating_sub(rhs.line_count),
        )
    }
}

impl Sum for LineTrackerSummary {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(LineTrackerSummary::default(), |acc, x| acc + x)
    }
}

/// The actual data stored in the leaves.
/// Uses u32 to support massive single lines (up to 4GB) without wasting 64-bit space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineChunk {
    pub byte_length: u32,
    pub newlines: Vec<u32>,
}

impl MeasuredBTreeData for LineChunk {
    type Measure = LineTrackerSummary;

    fn get_measure(&self) -> Self::Measure {
        LineTrackerSummary::new(self.byte_length as u64, self.newlines.len() as u64)
    }

    fn split_off(&mut self, offset: Self::Measure) -> Self {
        // Safe cast: byte_length is u32, so min() enforces a u32 ceiling
        let split_byte = (offset.byte_count as u32).min(self.byte_length);

        let split_idx = self.newlines.partition_point(|&pos| pos < split_byte);
        let mut right_newlines = self.newlines.split_off(split_idx);

        if !right_newlines.is_empty() {
            for pos in &mut right_newlines {
                // Pure u32 math. Guaranteed not to underflow.
                *pos -= split_byte;
            }
        }

        let right_chunk = LineChunk {
            byte_length: self.byte_length - split_byte,
            newlines: right_newlines,
        };

        self.byte_length = split_byte;

        right_chunk
    }

    fn try_merge(&mut self, other: &Self) -> bool {
        // EDGE CASE 1: Prevent infinite merging to keep Vec shifts fast
        // EDGE CASE 2: Prevent u32 byte overflow (extremely rare with MAX_CHUNK_LINES = 16, but safe)
        if self.newlines.len() + other.newlines.len() > MAX_CHUNK_LINES as usize
            || self.byte_length.checked_add(other.byte_length).is_none()
        {
            return false;
        }

        let byte_offset = self.byte_length;

        self.byte_length += other.byte_length;

        self.newlines.reserve_exact(other.newlines.len());
        // Clean addition without try_from or expect()
        self.newlines
            .extend(other.newlines.iter().map(|&pos| pos + byte_offset));

        true
    }
}

#[derive(Debug)]
pub struct SearchResult<'a> {
    pub chunk: &'a LineChunk,
    pub start_byte: u64, // UPGRADED from usize
    pub start_line: u64, // UPGRADED from usize
}

#[derive(Debug, Default)]
pub struct LineTracker {
    pub tree: MeasuredBTree<LineChunk>,
}

impl LineTracker {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    fn measure_of(&self, node_index: usize) -> LineTrackerSummary {
        match &self.tree.pool[node_index] {
            MeasuredBTreeNode::Internal { measure, .. }
            | MeasuredBTreeNode::Leaf { measure, .. } => *measure,
        }
    }

    pub fn insert(&mut self, target: LineTrackerSummary, data: LineChunk) {
        self.tree.insert(target, data).unwrap();
    }

    pub fn find_by_byte(&self, target_byte: u64) -> Option<SearchResult<'_>> {
        let mut current_node_index = self.tree.root_idx?;
        let (mut accumulated_bytes, mut accumulated_lines) = (0u64, 0u64);

        loop {
            match &self.tree.pool[current_node_index] {
                MeasuredBTreeNode::Internal { children, .. } => {
                    current_node_index = children.iter().find_map(|&child_index| {
                        let child_measure = self.measure_of(child_index);
                        let measure_bytes = child_measure.byte_count;

                        (target_byte < accumulated_bytes + measure_bytes)
                            .then_some(child_index)
                            .or_else(|| {
                                accumulated_bytes += measure_bytes;
                                accumulated_lines += child_measure.line_count;
                                None
                            })
                    })?;
                }
                MeasuredBTreeNode::Leaf { data, .. } => {
                    return data.iter().find_map(|chunk| {
                        let chunk_bytes = chunk.byte_length as u64;

                        (target_byte < accumulated_bytes + chunk_bytes)
                            .then_some(SearchResult {
                                chunk,
                                start_byte: accumulated_bytes,
                                start_line: accumulated_lines,
                            })
                            .or_else(|| {
                                accumulated_bytes += chunk_bytes;
                                accumulated_lines += chunk.newlines.len() as u64;
                                None
                            })
                    });
                }
            }
        }
    }

    pub fn find_by_line(&self, target_line: u64) -> Option<SearchResult<'_>> {
        let mut current_node_index = self.tree.root_idx?;
        let (mut accumulated_bytes, mut accumulated_lines) = (0u64, 0u64);

        loop {
            match &self.tree.pool[current_node_index] {
                MeasuredBTreeNode::Internal { children, .. } => {
                    current_node_index = children.iter().find_map(|&child_index| {
                        let child_measure = self.measure_of(child_index);
                        let measure_lines = child_measure.line_count;

                        (target_line < accumulated_lines + measure_lines)
                            .then_some(child_index)
                            .or_else(|| {
                                accumulated_bytes += child_measure.byte_count;
                                accumulated_lines += measure_lines;
                                None
                            })
                    })?;
                }
                MeasuredBTreeNode::Leaf { data, .. } => {
                    return data.iter().find_map(|chunk| {
                        let chunk_lines = chunk.newlines.len() as u64;

                        (target_line < accumulated_lines + chunk_lines)
                            .then_some(SearchResult {
                                chunk,
                                start_byte: accumulated_bytes,
                                start_line: accumulated_lines,
                            })
                            .or_else(|| {
                                accumulated_bytes += chunk.byte_length as u64;
                                accumulated_lines += chunk_lines;
                                None
                            })
                    });
                }
            }
        }
    }

    pub fn byte_offset_of_line(&self, target_line: u64) -> Option<u64> {
        if target_line == 0 {
            return Some(0);
        }

        let target_newline_idx = target_line - 1;
        let search_result = self.find_by_line(target_newline_idx)?;

        // This cast to usize is safe because the local difference will never exceed MAX_CHUNK_LINES (16)
        let local_newline_idx = (target_newline_idx - search_result.start_line) as usize;

        let relative_byte_offset = (search_result.chunk.newlines[local_newline_idx] as u64) + 1;

        Some(search_result.start_byte + relative_byte_offset)
    }

    pub fn delete_range(&mut self, start_byte: u64, end_byte: u64) {
        if start_byte >= end_byte {
            return;
        }

        let start_measure = if start_byte == 0 {
            LineTrackerSummary::default()
        } else {
            let res = self
                .find_by_byte(start_byte)
                .expect("Start byte out of bounds");

            // Safe cast: a chunk's length is u32, so the difference fits in u32
            let local_byte_offset = (start_byte - res.start_byte) as u32;
            let local_lines = res
                .chunk
                .newlines
                .partition_point(|&p| p < local_byte_offset) as u64;

            LineTrackerSummary::new(start_byte, res.start_line + local_lines)
        };

        let end_measure = match self.find_by_byte(end_byte) {
            Some(res) => {
                let local_byte_offset = (end_byte - res.start_byte) as u32;
                let local_lines =
                    res.chunk
                        .newlines
                        .partition_point(|&p| p < local_byte_offset) as u64;

                LineTrackerSummary::new(end_byte, res.start_line + local_lines)
            }
            None => self
                .tree
                .root_idx
                .map(|idx| self.measure_of(idx))
                .unwrap_or_default(),
        };

        self.tree.delete(start_measure..end_measure).unwrap();
    }
}
