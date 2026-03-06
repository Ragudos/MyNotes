use crate::btree::{MeasuredBTree, MeasuredBTreeData, MeasuredBTreeResult};

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub enum BufferKind {
    Add,
    Original,
}

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct Piece {
    pub buffer_kind: BufferKind,
    pub start: usize,
    pub end: usize,
}

impl MeasuredBTreeData for Piece {
    type Measure = usize;

    fn measure(&self) -> Self::Measure {
        self.end - self.start
    }

    fn split_at(&mut self, offset: Self::Measure) -> Self {
        debug_assert!(offset >= Self::Measure::default() && offset < self.measure());

        let split_point = self.start + offset;
        let right_piece = Piece {
            buffer_kind: self.buffer_kind,
            start: split_point,
            end: self.end,
        };

        self.end = split_point;

        right_piece
    }

    fn try_merge(&mut self, other: &Self) -> bool {
        // To merge, they must be from the same buffer
        // AND the end of `self` must exactly touch the start of `other`.
        if self.buffer_kind == other.buffer_kind && self.end == other.start {
            // Extend self to encompass other
            self.end = other.end;
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Default)]
pub struct PieceTable {
    pub tree: MeasuredBTree<Piece>,
}

impl PieceTable {
    pub fn new() -> Self {
        Self {
            tree: MeasuredBTree::new(),
        }
    }

    #[inline]
    #[must_use]
    pub fn get_at(&self, target: usize) -> Option<(&Piece, usize)> {
        self.tree.get_at(target)
    }

    pub fn insert(
        &mut self,
        doc_offset: usize,
        buf_offset: usize,
        length: usize,
        buffer_kind: BufferKind,
    ) -> MeasuredBTreeResult<(), usize> {
        // Prevent empty strings from creating zero-length pieces in the tree
        if length == 0 {
            return Ok(());
        }

        // 1. Construct the dumb piece
        let new_piece = Piece {
            buffer_kind,
            start: buf_offset,
            end: buf_offset + length,
        };

        // 2. Delegate the mathematical routing, splitting, and merging to the B-Tree
        // (Assuming your tree returns a Result we can map to `()`, or a custom BTreeError)
        self.tree.insert(doc_offset, new_piece)
    }

    pub fn delete(&mut self, start: usize, length: usize) -> MeasuredBTreeResult<(), usize> {
        if self.tree.pool.is_empty() {
            return Ok(());
        }

        self.tree.delete(start..start + length)
    }
}
