use crate::btree::{MeasuredBTree, MeasuredBTreeData, MeasuredBTreeNode};
use num_traits::SaturatingSub;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub, SubAssign};

pub const MAX_CHUNK_LINES: usize = 64;

/// The 2D Measure cached by internal B-Tree nodes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Ord, PartialOrd)]
pub struct LineTrackerSummary {
    pub byte_count: usize,
    pub line_count: usize,
}

impl Add for LineTrackerSummary {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            line_count: self.line_count + rhs.line_count,
            byte_count: self.byte_count + rhs.byte_count,
        }
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
        Self {
            line_count: self.line_count.saturating_sub(rhs.line_count),
            byte_count: self.byte_count.saturating_sub(rhs.byte_count),
        }
    }
}

impl SubAssign for LineTrackerSummary {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl SaturatingSub for LineTrackerSummary {
    fn saturating_sub(&self, v: &Self) -> Self {
        Self {
            line_count: self.line_count.saturating_sub(v.line_count),
            byte_count: self.byte_count.saturating_sub(v.byte_count),
        }
    }
}

impl Sum for LineTrackerSummary {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(LineTrackerSummary::default(), |acc, x| acc + x)
    }
}

/// The actual data stored in the leaves.
/// Knows its exact length and where every newline lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineChunk {
    pub byte_length: usize,
    pub newlines: Vec<usize>,
}

impl MeasuredBTreeData for LineChunk {
    type Measure = LineTrackerSummary;

    fn measure(&self) -> Self::Measure {
        LineTrackerSummary {
            line_count: self.newlines.len(),
            byte_count: self.byte_length,
        }
    }

    fn split_at(&mut self, offset: Self::Measure) -> Self {
        let split_byte = offset.byte_count;
        let split_idx = self.newlines.partition_point(|&pos| pos < split_byte);
        let right_chunk = LineChunk {
            byte_length: self.byte_length - split_byte,
            newlines: self
                .newlines
                .drain(split_idx..)
                .map(|pos| pos - split_byte)
                .collect(),
        };
        // 4. Update the left chunk (self)
        self.byte_length = split_byte;

        right_chunk
    }

    fn try_merge(&mut self, other: &Self) -> bool {
        // EDGE CASE 1: Prevent infinite merging to keep Vec shifts fast
        if self.newlines.len() + other.newlines.len() > MAX_CHUNK_LINES {
            return false;
        }

        // LineChunks can always be merged natively.
        let byte_offset = self.byte_length;

        self.byte_length += other.byte_length;
        self.newlines
            .extend(other.newlines.iter().map(|&pos| pos + byte_offset));

        true
    }
}

#[derive(Debug)]
pub struct SearchResult<'a> {
    pub chunk: &'a LineChunk,
    pub start_byte: usize,
    pub start_line: usize,
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
    /// # Purpose
    ///
    /// Traverses the measured B-Tree to locate the specific text chunk containing a given byte offset.
    /// It routes down the tree by accumulating the byte counts of each node, effectively acting as a fast
    /// spatial search through the document.
    ///
    /// # Parameters
    ///
    /// - **`target_byte`**: The absolute byte offset (0-indexed) to search for. Used to determine which
    ///   child node or leaf chunk contains the target index. It is necessary because the tree only stores
    ///   relative weights (lengths), so the absolute position must be calculated dynamically during traversal.
    ///
    /// # Returns
    ///
    /// Returns an `Option<SearchResult<'_>>`. On success, it yields a `SearchResult` containing a
    /// reference to the `LineChunk` where the byte resides, along with the absolute `start_byte` and
    /// `start_line` of that chunk.
    ///
    /// # Errors
    ///
    /// Returns `None` if the `target_byte` is greater than or equal to the total byte length of the
    /// tracked text, or if the B-Tree is empty (i.e., `root_idx` is `None`).
    ///
    /// # Panics
    ///
    /// This function does not panic under normal operation. It safely returns `None` for out-of-bounds indices.
    ///
    /// # Examples
    ///
    /// ```
    /// use mynotes_core::line_tracker::{LineTracker, LineTrackerSummary, LineChunk, SearchResult};
    ///
    /// fn create_chunk(text: &str) -> LineChunk {
    ///     LineChunk {
    ///         byte_length: text.len(),
    ///         newlines: text.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i).collect(),
    ///     }
    /// }
    ///
    /// let mut tracker = LineTracker::new();
    ///
    /// // Populate the tracker so the example works
    /// tracker.insert(
    ///     LineTrackerSummary { byte_count: 0, line_count: 0 },
    ///     create_chunk("Hello\nWorld")
    /// );
    ///
    /// // Find the chunk containing the 'W' (byte index 6)
    /// let result = tracker.find_by_byte(6).expect("Should find chunk");
    /// assert_eq!(result.start_byte, 0);
    /// assert_eq!(result.chunk.byte_length, 11);
    /// ```
    pub fn find_by_byte(&self, target_byte: usize) -> Option<SearchResult<'_>> {
        let mut current_node_index = self.tree.root_idx?;
        let (mut accumulated_bytes, mut accumulated_lines) = (0, 0);

        loop {
            match &self.tree.pool[current_node_index] {
                MeasuredBTreeNode::Internal { children, .. } => {
                    current_node_index = children.iter().find_map(|&child_index| {
                        let child_measure = self.measure_of(child_index);
                        (target_byte < accumulated_bytes + child_measure.byte_count)
                            .then_some(child_index)
                            .or_else(|| {
                                accumulated_bytes += child_measure.byte_count;
                                accumulated_lines += child_measure.line_count;
                                None
                            })
                    })?;
                }
                MeasuredBTreeNode::Leaf { data, .. } => {
                    return data.iter().find_map(|chunk| {
                        (target_byte < accumulated_bytes + chunk.byte_length)
                            .then_some(SearchResult {
                                chunk,
                                start_byte: accumulated_bytes,
                                start_line: accumulated_lines,
                            })
                            .or_else(|| {
                                accumulated_bytes += chunk.byte_length;
                                accumulated_lines += chunk.newlines.len();
                                None
                            })
                    });
                }
            }
        }
    }

    /// # Purpose
    ///
    /// Traverses the measured B-Tree to locate the specific text chunk containing a given newline index.
    /// It routes down the tree by accumulating the line counts (newlines) of each node, allowing for fast
    /// $O(\log n)$ line lookups.
    ///
    /// # Parameters
    ///
    /// - **`target_line`**: The absolute line/newline index (0-indexed) to search for. Used to determine
    ///   which child node or leaf chunk contains the specific newline. It is required to map a global line
    ///   number down to the local relative chunk that stores its metadata.
    ///
    /// # Returns
    ///
    /// Returns an `Option<SearchResult<'_>>`. On success, it yields a `SearchResult` containing a
    /// reference to the `LineChunk` where the target newline resides, along with the absolute `start_byte`
    /// and `start_line` of that chunk.
    ///
    /// # Errors
    ///
    /// Returns `None` if the `target_line` exceeds the total number of tracked newlines in the document,
    /// or if the B-Tree is empty.
    ///
    /// # Panics
    ///
    /// This function does not panic. Out-of-bounds line queries safely resolve to `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mynotes_core::line_tracker::{LineTracker, LineTrackerSummary, LineChunk, SearchResult};
    ///
    /// fn create_chunk(text: &str) -> LineChunk {
    ///     LineChunk {
    ///         byte_length: text.len(),
    ///         newlines: text.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i).collect(),
    ///     }
    /// }
    ///
    /// let mut tracker = LineTracker::new();
    ///
    /// // Populate the tracker
    /// tracker.insert(
    ///     LineTrackerSummary { byte_count: 0, line_count: 0 },
    ///     create_chunk("Line 0\nLine 1\nLine 2")
    /// );
    ///
    /// // Find which chunk contains the end of Line 1 (the second newline, index 1)
    /// let result = tracker.find_by_line(1).expect("Should find chunk");
    /// assert_eq!(result.start_line, 0);
    /// assert_eq!(result.chunk.newlines.len(), 2);
    /// ```
    pub fn find_by_line(&self, target_line: usize) -> Option<SearchResult<'_>> {
        let mut current_node_index = self.tree.root_idx?;
        let (mut accumulated_bytes, mut accumulated_lines) = (0, 0);

        loop {
            match &self.tree.pool[current_node_index] {
                MeasuredBTreeNode::Internal { children, .. } => {
                    current_node_index = children.iter().find_map(|&child_index| {
                        let child_measure = self.measure_of(child_index);
                        (target_line < accumulated_lines + child_measure.line_count)
                            .then_some(child_index)
                            .or_else(|| {
                                accumulated_bytes += child_measure.byte_count;
                                accumulated_lines += child_measure.line_count;
                                None
                            })
                    })?;
                }
                MeasuredBTreeNode::Leaf { data, .. } => {
                    return data.iter().find_map(|chunk| {
                        (target_line < accumulated_lines + chunk.newlines.len())
                            .then_some(SearchResult {
                                chunk,
                                start_byte: accumulated_bytes,
                                start_line: accumulated_lines,
                            })
                            .or_else(|| {
                                accumulated_bytes += chunk.byte_length;
                                accumulated_lines += chunk.newlines.len();
                                None
                            })
                    });
                }
            }
        }
    }

    /// # Purpose
    ///
    /// Calculates the exact starting byte index of a given line. Since a line implicitly starts exactly
    /// one byte after the newline character that precedes it, this method locates that preceding newline
    /// and offsets its absolute byte position by 1.
    ///
    /// # Parameters
    ///
    /// - **`target_line`**: The 0-based line number whose starting byte offset you want to find. Used to
    ///   identify which preceding newline to search for (e.g., to find the start of Line 2, it searches for
    ///   Newline 1). This is essential for converting 2D line coordinates into 1D physical string slices.
    ///
    /// # Returns
    ///
    /// Returns an `Option<usize>` containing the absolute byte index where the requested line begins.
    ///
    /// # Errors
    ///
    /// Returns `None` if the requested `target_line` does not exist in the document (i.e., requesting
    /// line 5 in a 3-line document).
    ///
    /// # Panics
    ///
    /// This function does not panic. It handles the edge case for Line 0 internally and safely propagates
    /// `None` if the underlying `find_by_line` query fails or goes out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use mynotes_core::line_tracker::{LineTracker, LineTrackerSummary, LineChunk};
    ///
    /// fn create_chunk(text: &str) -> LineChunk {
    ///     LineChunk {
    ///         byte_length: text.len(),
    ///         newlines: text.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i).collect(),
    ///     }
    /// }
    ///
    /// let mut tracker = LineTracker::new();
    ///
    /// // Populate the tracker
    /// tracker.insert(
    ///     LineTrackerSummary { byte_count: 0, line_count: 0 },
    ///     create_chunk("Line 0\nLine 1\nLine 2")
    /// );
    ///
    /// // Line 0 always starts at 0
    /// assert_eq!(tracker.byte_offset_of_line(0), Some(0));
    ///
    /// // Find where "Line 1" begins
    /// assert_eq!(tracker.byte_offset_of_line(1), Some(7)); // Starts right after "Line 0\n"
    /// ```
    pub fn byte_offset_of_line(&self, target_line: usize) -> Option<usize> {
        // Line 0 always starts at the very beginning of the file
        if target_line == 0 {
            return Some(0);
        }

        // To find where a line starts, find the newline character right before it.
        let target_newline_idx = target_line - 1;

        // Find the chunk containing that specific newline
        let search_result = self.find_by_line(target_newline_idx)?;

        // Get the exact local index of that newline inside the chunk
        let local_newline_idx = target_newline_idx - search_result.start_line;

        // The line starts exactly 1 byte after the newline
        let relative_byte_offset = search_result.chunk.newlines[local_newline_idx] + 1;

        Some(search_result.start_byte + relative_byte_offset)
    }

    /// # Purpose
    /// Deletes a range of text from the tracker by converting a 1D byte range
    /// into a 2D measure (bytes + lines), and delegating to the generic B-Tree.
    pub fn delete_range(&mut self, start_byte: usize, end_byte: usize) {
        if start_byte >= end_byte {
            return; // Nothing to delete
        }

        // 1. Calculate the exact Measure (bytes + lines) for the START of the deletion
        let start_measure = if start_byte == 0 {
            LineTrackerSummary::default()
        } else {
            let res = self
                .find_by_byte(start_byte)
                .expect("Start byte out of bounds");
            let local_byte_offset = start_byte - res.start_byte;
            // Count how many newlines appear *before* this byte in the chunk
            let local_lines = res
                .chunk
                .newlines
                .iter()
                .filter(|&&p| p < local_byte_offset)
                .count();

            LineTrackerSummary {
                byte_count: start_byte,
                line_count: res.start_line + local_lines,
            }
        };

        // 2. Calculate the exact Measure (bytes + lines) for the END of the deletion
        let end_measure = {
            let res = self.find_by_byte(end_byte).expect("End byte out of bounds");
            let local_byte_offset = end_byte - res.start_byte;
            let local_lines = res
                .chunk
                .newlines
                .iter()
                .filter(|&&p| p < local_byte_offset)
                .count();

            LineTrackerSummary {
                byte_count: end_byte,
                line_count: res.start_line + local_lines,
            }
        };

        // 3. Delegate to the underlying tree's deletion logic.
        // (Assuming your generic MeasuredBTree has a `delete` or `remove_range` method)
        self.tree.delete(start_measure..end_measure).unwrap();
    }
}
