use crate::btree::{MeasuredBTree, MeasuredBTreeData, MeasuredBTreeResult};

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub enum BufferKind {
    Add,
    Original,
}

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct Piece {
    pub buffer_kind: BufferKind,
    pub start: u64,
    pub end: u64,
}

impl MeasuredBTreeData for Piece {
    type Measure = u64;

    fn get_measure(&self) -> Self::Measure {
        self.end - self.start
    }

    fn split_off(&mut self, offset: Self::Measure) -> Self {
        debug_assert!(offset > 0 && offset < self.get_measure());

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
        if self.buffer_kind == other.buffer_kind && self.end == other.start {
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
    pub fn get_at(&self, target: u64) -> Option<(&Piece, u64)> {
        self.tree.get_at(target)
    }

    pub fn insert(
        &mut self,
        doc_offset: u64,
        buf_offset: u64,
        length: u64,
        buffer_kind: BufferKind,
    ) -> MeasuredBTreeResult<(), u64> {
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

    pub fn delete(&mut self, start: u64, length: u64) -> MeasuredBTreeResult<(), u64> {
        if self.tree.pool.is_empty() {
            return Ok(());
        }

        self.tree.delete(start..start + length)
    }

    /// Returns a zero-allocation iterator over the pieces in order.
    pub fn iter(&self) -> impl Iterator<Item = &Piece> {
        self.tree.iter()
    }

    pub fn get_all_pieces(&self) -> Vec<Piece> {
        self.tree.get_all_data()
    }
}
