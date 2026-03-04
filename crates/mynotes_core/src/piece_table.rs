//! # Piece Table
//!
//! A highly optimized, B-Tree backed data structure for managing text editor buffers.
//!
//! ## What it does
//! The `PieceTable` maps 1D logical document offsets (the text the user sees) to physical
//! memory locations (the `Original` file buffer or the append-only `Add` buffer). It tracks
//! text as a sequence of `Piece` objects, which contain the length and memory pointers of the text
//! rather than the string data itself.
//!
//! ## Why it exists
//! In a naive text editor, a document is stored as a single `String`. If you insert a character
//! at the beginning of a 100MB file, the OS must shift 100MB of data in memory, resulting in an
//! $O(N)$ operation that freezes the editor.
//!
//! The `PieceTable` solves this by keeping all buffers immutable or append-only. Inserting text
//! simply means appending the new string to the end of the `Add` buffer, and updating the B-Tree
//! to point to it. This reduces insertions, deletions, and lookups to $O(\log N)$ time, allowing
//! instant edits regardless of file size.
//!
//! Deletion would simply remove the pointer or `Piece` position to the existing
//! `Original` or `Add` buffers. The still exist physically during
//! the lifetime of this `PieceTable` until reset.
//!
//! ## How it's used
//! 1. **Initialization:** The table is instantiated with a single `Piece` pointing to the
//!    entire length of the `Original` buffer.
//! 2. **Insertion:** When a user types, the text is pushed to an external `Add` buffer. The table's
//!    `insert()` method is called with the absolute document offset, the buffer offset, and the length.
//! 3. **Routing:** The `PieceTable` traverses its internal B-Tree, splitting pieces and nodes
//!    as necessary to accommodate the new pointers while maintaining tree balance.
//! 4. **Lookup:** To be implemented
//!

use std::cmp::min;
use std::ops::Range;
use thiserror::Error;

/// Alias for the `children` of `Node`,
/// which is a `usize` that refers to the index
/// of the child `Node` in the B-Tree 1D pool vector.
type PoolIndex = usize;

/// Alias for the position or `Range` of a `Piece` in a
/// text or document.
type LinePosition = Range<usize>;

pub type PieceTableResult<T> = Result<T, PieceTableError>;

/// 1 MB base capacity for `PieceTable` vector
pub const BASE_CAPACITY: usize = 1024 * 1024;
/// Minimum B-Tree degree
const T: usize = 8;
/// # Reasoning
///
/// The maximum amount of `Piece` instances a `Node` can hold. The formula is
/// `2 * T - 1` or twice the minimum B-Tree degree (branching factor)
/// because of B-Tree rules, which is, the tree should always be balanced.
///
/// During splitting, if we take one piece from the `Node` that's full,
/// we would be left with `(2 * T - 1) - 1` or `2 * T - 2`. If we were
/// to divide that between two `Node` instances (since we're splitting),
/// it would be equal to `T - 1`. With that, both `Node` instances
/// would have exactly `T - 1` `Piece` instances.
///
/// This way, there would be no need to reallocate and deallocate the `Piece` and
/// `children` vectors of a `Node` as often as it would.
///
/// # Example
///
/// Let T = 3
///
/// Minimum pieces: 3 - 1 = 2
/// Maximum pieces: 2 * 3 - 1 = 5
///
/// If a `Node` has 5 pieces `[A, B, C, D, E]`, it will split itself:
///
/// - **`[C]`:** The new node pushed to the parent
/// - **`[A, B]`:** The left node (exactly 2 items)
/// - **`[D, E]`:** The right node (exactly 2 items)
pub const PIECES_CAPACITY: usize = 2 * T - 1;
/// From minimum degree `T`, double it
/// to be the maximum degree or capacity.
pub const NODE_CHILDREN_CAPACITY: usize = 2 * T;

#[derive(Error, Debug)]
pub enum PieceTableError {
    #[error("The requested ({requested:?}) is out of bounds in max length ({max_length:?}).")]
    OutOfBounds { max_length: usize, requested: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKind {
    Original,
    Add,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub buf_kind: BufferKind,
    pub position: LinePosition,
}

#[derive(Clone, Debug)]
pub enum Node {
    Leaf {
        /// Total `length` of all pieces in this node + subtrees.
        total_len: usize,
        pieces: Vec<Piece>,
    },
    Internal {
        /// Total `length` of all pieces in this node + subtrees.
        total_len: usize,
        children: Vec<PoolIndex>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct PieceTable {
    pub pool: Vec<Node>,
    free_leaves_list: Vec<PoolIndex>,
    free_internals_list: Vec<PoolIndex>,
    /// Bookmark the `PoolIndex` of the B-Tree's root because it changes.
    /// `Option` is used since, on initialization, the B-Tree is empty.
    pub root_idx: Option<PoolIndex>,
}

pub struct PieceTableIterator<'pool> {
    pool: &'pool [Node],
    /// Stack of `(NodeIndex, NextChildIndex)`. Tracks our path back up the tree.
    stack: Vec<(PoolIndex, usize)>,
    /// The current leaf node and the index of the piece we are looking at
    current_leaf: Option<(PoolIndex, usize)>,
    /// How many characters we still need to yield before stopping
    remaining_len: usize,
    /// If the start index lands in the middle of a piece, we need to offset our slice
    offset_in_piece: usize,
}

impl Piece {
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.position.end - self.position.start
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.position.start == self.position.end
    }
}

impl Node {
    #[must_use]
    pub fn new(is_leaf: bool) -> Self {
        if is_leaf {
            Self::Leaf {
                pieces: Vec::with_capacity(PIECES_CAPACITY),
                total_len: 0,
            }
        } else {
            Self::Internal {
                children: Vec::with_capacity(if is_leaf { 0 } else { NODE_CHILDREN_CAPACITY }),
                total_len: 0,
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Node::Leaf { total_len, .. } => *total_len,
            Node::Internal { total_len, .. } => *total_len,
        }
    }

    #[inline]
    #[must_use]
    pub fn mut_len(&mut self) -> &mut usize {
        match self {
            Node::Leaf { total_len, .. } => total_len,
            Node::Internal { total_len, .. } => total_len,
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        match self {
            Node::Leaf { pieces, total_len } => {
                pieces.clear();

                *total_len = 0;
            }
            Node::Internal {
                children,
                total_len,
            } => {
                children.clear();

                *total_len = 0;
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        match self {
            Node::Leaf { .. } => true,
            Node::Internal { .. } => false,
        }
    }

    /// # Purpose
    ///
    /// Used to check if this `Node` has a length past or equal to its capacity.
    /// Defaults to `false` for `Node::Internal` since they don't
    /// hold data.
    ///
    /// # Returns
    ///
    /// - **`true`**: If `pieces.len() >= PIECES_CAPACITY` if and only if this `Node` is a `Node::LeafNode`.
    /// - **`false`**: If `pieces.len() < PIECES_CAPACITY` or if `Node` is `Node::Internal`.
    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        match self {
            Node::Leaf { pieces, .. } => pieces.len() >= PIECES_CAPACITY,
            Node::Internal { .. } => false,
        }
    }

    /// # Purpose
    ///
    /// Used when needing the `Vec<Piece>` of a `Node::Leaf` without the
    /// hassle of matching the `Node` enum wrapper.
    ///
    /// # Panics
    ///
    /// - **If `Node` is a `Node::Internal`:**
    ///     - Make sure that this `Node` is a `Node::Leaf` when calling this function
    #[inline]
    #[must_use]
    pub fn get_mut_pieces(&mut self) -> &mut Vec<Piece> {
        match self {
            Node::Leaf { pieces, .. } => pieces,
            _ => panic!("`get_mut_pieces` is called within a `Node::Internal`"),
        }
    }

    /// # Purpose
    ///
    /// The logic to get the location of an absolute index. Used by `PieceTable`.
    ///
    /// # Logic
    ///
    /// Mutates the `abs_idx` and `current_idx` in-place and only returns
    /// the necessary information, which is the `piece_idx`.
    ///
    /// # Returns
    ///
    /// The index of the `Piece` that the `abs_idx` belongs to in the vector
    /// of this node.
    pub fn get_location(
        &self,
        abs_idx: &mut usize,
        current_idx: &mut PoolIndex,
        pool: &[Node],
    ) -> usize {
        match self {
            Node::Internal { children, .. } => {
                for &pool_idx in children {
                    let child_weight = pool[pool_idx].len();

                    if *abs_idx < child_weight {
                        *current_idx = pool_idx;

                        return pool[pool_idx].get_location(abs_idx, current_idx, pool);
                    }

                    *abs_idx -= child_weight;
                }

                unreachable!("Bounds check guarantees that a routing child exists");
            }
            Node::Leaf { pieces, .. } => {
                for (piece_idx, piece) in pieces.iter().enumerate() {
                    let piece_len = piece.len();

                    if *abs_idx < piece_len {
                        return piece_idx;
                    }

                    *abs_idx -= piece_len;
                }

                unreachable!("Bounds check guarantees that a piece exists");
            }
        }
    }
}

impl PieceTable {
    pub fn new() -> Self {
        Self {
            pool: Vec::with_capacity(BASE_CAPACITY),
            free_leaves_list: Vec::new(),
            free_internals_list: Vec::new(),
            root_idx: None,
        }
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.pool.iter().map(|node| node.len()).sum()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// # Purpose
    ///
    /// Allocates a node and reusing a freed index if there's one
    /// available.
    ///
    /// This is useful to avoid having the OS give us memory allocations
    /// when inserting a node using `push` as often as it would.
    ///
    /// If there's no available position or index, this function
    /// will simply `push` to the pool.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mynotes_core::piece_table::PieceTable;
    /// let mut table = PieceTable::new();
    ///
    /// // Force allocate a node (pool is initially empty)
    /// let idx1 = table.allocate_node(true); // Creates Leaf
    /// assert_eq!(idx1, 0);
    /// assert_eq!(table.pool.len(), 1);
    ///
    /// // Deallocate it, pushing it to the free list
    /// table.deallocate_node(idx1);
    ///
    /// // Next allocation should recycle idx 0 without growing the pool
    /// let idx2 = table.allocate_node(true);
    /// assert_eq!(idx2, 0);
    /// assert_eq!(table.pool.len(), 1);
    /// ```
    pub fn allocate_node(&mut self, is_leaf: bool) -> PoolIndex {
        // 1. Try to pop from the correct specific free list
        let pooled_idx = if is_leaf {
            self.free_leaves_list.pop()
        } else {
            self.free_internals_list.pop()
        };

        // 2. If we got one, it was already cleared during deallocation.
        if let Some(idx) = pooled_idx {
            return idx;
        }

        // 3. Fallback: pool is completely empty for this variant, push a new one.
        let idx = self.pool.len();

        self.pool.push(Node::new(is_leaf));

        idx
    }

    /// # Purpose
    ///
    /// Deallocates a node and adds its index to the `free_list`.
    ///
    /// This is useful to avoid having to unnecessarily resize the
    /// vector on every deletion of a `Node`.
    pub fn deallocate_node(&mut self, idx: PoolIndex) {
        let node = &mut self.pool[idx];

        node.clear();

        if node.is_leaf() {
            self.free_leaves_list.push(idx);
        } else {
            self.free_internals_list.push(idx);
        }
    }

    /// # Purpose
    ///
    /// Used to find the location of an item in this B-Tree using an absolute index
    /// as if it were stored in a flat 1D array.
    ///
    /// # Returns
    ///
    /// A tuple of:
    ///
    /// - **`PoolIndex:`** the index of the node in the pool.
    /// - **`usize`**: the index of the `Piece` in the `Node`.
    /// - **`usize`**: the index of the item in the `Piece`, or its offset.
    ///
    /// # Examples
    ///
    /// ```
    ///  use mynotes_core::piece_table::{PieceTable, BufferKind};
    /// let mut table = PieceTable::new();
    /// table.insert(0, 0, 10, BufferKind::Add).unwrap();
    ///
    /// // Valid lookup
    /// let loc = table.get_location(4);
    /// assert!(loc.is_some());
    /// assert_eq!(loc.unwrap().2, 4);
    ///
    /// // Out of bounds lookup
    /// let out_of_bounds = table.get_location(10);
    /// assert!(out_of_bounds.is_none(), "Index equal to length is out of bounds");
    /// ```
    pub fn get_location(&self, absolute_idx: usize) -> Option<(PoolIndex, usize, usize)> {
        // Return early if there's no `root_idx`.
        let mut current_idx = self.root_idx?;
        let mut abs_idx = absolute_idx;

        // If the absolute index is greater than the total length
        // of the `Node`, then it's out of bounds
        if abs_idx >= self.pool[current_idx].len() {
            return None;
        }

        let piece_idx =
            self.pool[current_idx].get_location(&mut abs_idx, &mut current_idx, &self.pool);

        Some((current_idx, piece_idx, abs_idx))
    }
    /// # Purpose
    ///
    /// Splits a full `Leaf` node in half to maintain B-Tree balance properties.
    /// The right half of the pieces is extracted and moved into a newly allocated leaf node.
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The `PoolIndex` of the `Node::Leaf` that has exceeded its capacity.
    ///
    /// # Returns
    ///
    /// - **`PoolIndex`**: The index of the newly created right sibling node.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `Node::Internal` instead of a `Leaf`
    ///
    /// # Examples
    ///
    /// ```
    /// // Assume `pool_idx` points to a Leaf node that just exceeded `PIECES_CAPACITY`.
    /// // let new_sibling_idx = table.split_leaf(pool_idx);
    ///
    /// // The original node now retains the left half of the pieces.
    /// // The newly allocated node (`new_sibling_idx`) holds the right half.
    /// // The total weight of both nodes combined equals the original node's weight.
    /// ```
    fn split_leaf(&mut self, pool_idx: PoolIndex) -> PoolIndex {
        let new_leaf_idx = self.allocate_node(true);
        let mut right_pieces = Vec::with_capacity(PIECES_CAPACITY);

        {
            let Node::Leaf { pieces, total_len } = &mut self.pool[pool_idx] else {
                unreachable!("`split_leaf` is called within a `Node::LeafNode`")
            };

            let split_at = pieces.len() / 2;
            // `drain` removes the right half without shrinking the left half's capacity.
            // `extend` pushes them into `right_pieces` without exceeding its capacity.
            right_pieces.extend(pieces.drain(split_at..));
            *total_len = pieces.iter().map(|p| p.len()).sum();
        }

        let Node::Leaf {
            pieces: new_leaf_pieces,
            total_len: new_leaf_total_len,
        } = &mut self.pool[new_leaf_idx]
        else {
            unreachable!("`split_leaf` is called within a `Node::LeafNode`")
        };

        let right_weight = right_pieces.iter().map(|p| p.len()).sum::<usize>();
        *new_leaf_pieces = right_pieces;
        *new_leaf_total_len = right_weight;

        new_leaf_idx
    }

    /// # Purpose
    ///
    /// Splits a full `Internal` node in half to maintain B-Tree balance properties.
    /// The right half of the child pointers is moved into a newly allocated internal node.
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The `PoolIndex` of the `Node::Internal` that has exceeded its capacity.
    ///
    /// # Returns
    ///
    /// - **`PoolIndex`**: The index of the newly created right sibling node.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `Node::Leaf` instead of an `Internal` node.
    /// - If `allocate_node(false)` returns a `Node::Leaf`.
    fn split_internal(&mut self, pool_idx: PoolIndex) -> PoolIndex {
        let right_children = match &mut self.pool[pool_idx] {
            Node::Internal { children, .. } => children.split_off(children.len() / 2),
            Node::Leaf { .. } => {
                unreachable!("`split_internal` is called within a `Node::Internal`")
            }
        };

        {
            let left_weight = match &self.pool[pool_idx] {
                Node::Internal { children, .. } => children
                    .iter()
                    .map(|pool_idx| self.pool[*pool_idx].len())
                    .sum::<usize>(),
                Node::Leaf { .. } => {
                    unreachable!("`split_internal` is called within a `Node::Leaf`")
                }
            };

            if let Node::Internal { total_len, .. } = &mut self.pool[pool_idx] {
                *total_len = left_weight;
            }
        }

        let new_idx = self.allocate_node(false);
        let right_children_len = right_children
            .iter()
            .map(|pool_idx| self.pool[*pool_idx].len())
            .sum();

        if let Node::Internal {
            children,
            total_len,
        } = &mut self.pool[pool_idx]
        {
            *total_len = right_children_len;
            *children = right_children;
        }

        new_idx
    }

    /// # Purpose
    ///
    /// Inserts a new `Piece` of text into a specific leaf node. If the new piece is contiguous
    /// with an existing piece from the same buffer, it merges them to save memory. Otherwise,
    /// it splices the pieces array.
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The `PoolIndex` of the leaf node.
    /// - `doc_offset`: The absolute position in the document text where the insertion occurs.
    /// - `buf_offset`: The physical index in the memory buffer where the raw text resides.
    /// - `length`: The length of the text being inserted.
    /// - `buf_kind`: The buffer (`Add` or `Original`) kind to distinguish the two.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `Node::Internal` instead of a `Leaf`.
    ///
    /// # Examples
    ///
    /// ```
    /// // Assume `table` has a root leaf with piece "HelloWorld" (len 10)
    /// // We splice a space " " (len 1) at document offset 5.
    /// // table.insert_into_leaf(root_idx, 5, 100, 1, BufferKind::Add);
    ///
    /// // The piece array inside the leaf will now contain 3 pieces:
    /// // 1. "Hello" (len 5)
    /// // 2. " " (len 1, BufferKind::Add)
    /// // 3. "World" (len 5)
    /// ```
    fn insert_into_leaf(
        &mut self,
        pool_idx: PoolIndex,
        doc_offset: usize,
        buf_offset: usize,
        length: usize,
        buf_kind: BufferKind,
    ) {
        let Node::Leaf { pieces, .. } = &mut self.pool[pool_idx] else {
            panic!("`insert_into_leaf` called on a `Node::Internal`");
        };

        let mut piece_idx = 0usize;
        let mut local_offset = doc_offset;

        while piece_idx < pieces.len() && local_offset > pieces[piece_idx].len() {
            local_offset -= pieces[piece_idx].len();
            piece_idx += 1;
        }

        let new_piece = Piece {
            buf_kind,
            position: buf_offset..buf_offset + length,
        };

        let prev_idx = if piece_idx == pieces.len() || local_offset == 0 {
            piece_idx.checked_sub(1)
        } else if local_offset == pieces[piece_idx].len() {
            Some(piece_idx)
        } else {
            None
        };

        if let Some(prev_piece) = prev_idx.and_then(|i| pieces.get_mut(i))
            && prev_piece.buf_kind == new_piece.buf_kind
            && prev_piece.position.end == new_piece.position.start
        {
            prev_piece.position.end = new_piece.position.end;

            return;
        }

        if piece_idx == pieces.len() {
            pieces.push(new_piece);
        } else if local_offset == 0 {
            pieces.insert(piece_idx, new_piece);
        } else if local_offset == pieces[piece_idx].len() {
            pieces.insert(piece_idx + 1, new_piece);
        } else {
            let piece = pieces[piece_idx].clone();

            pieces.splice(
                piece_idx..=piece_idx,
                [
                    Piece {
                        buf_kind: piece.buf_kind,
                        position: piece.position.start..piece.position.start + local_offset,
                    },
                    new_piece,
                    Piece {
                        buf_kind: piece.buf_kind,
                        position: piece.position.start + local_offset..piece.position.end,
                    },
                ],
            );
        }
    }

    /// # Purpose
    ///
    /// Recursively routes an insertion request down the B-Tree based on the document offset.
    /// Optimistically updates node weights on the way down, and handles child splits on the way back up.
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The index of the current node being traversed.
    /// - `doc_offset`: The target document offset for the insertion.
    /// - `buf_offset`: The memory buffer offset of the new text.
    /// - `length`: The length of the text being inserted.
    /// - `buf_kind`: The buffer containing the text.
    ///
    /// # Returns
    ///
    /// - **`Option<PoolIndex>`**: Returns `Some(PoolIndex)` containing the index of a new right sibling
    ///   if the child node split. Returns `None` if the insertion was accommodated without splitting.
    ///
    /// # Panics
    ///
    /// - **If a `Node` at `node_idx` is not a `Node::Internal`:**
    ///     - This happens if the guard before the panic is not correct. Before asserting that `self.pool[node_idx]`
    ///       is a `Node::Internal`, there should first be a check if it's a `Node::LeafNode` and handle it accordingly.
    #[must_use]
    fn insert_recursive(
        &mut self,
        pool_idx: PoolIndex,
        doc_offset: usize,
        buf_offset: usize,
        length: usize,
        buf_kind: BufferKind,
    ) -> Option<PoolIndex> {
        *self.pool[pool_idx].mut_len() += length;

        if self.pool[pool_idx].is_leaf() {
            self.insert_into_leaf(pool_idx, doc_offset, buf_offset, length, buf_kind);

            if self.pool[pool_idx].is_full() {
                return Some(self.split_leaf(pool_idx));
            }

            return None;
        }

        let mut target_child_idx = 0usize;
        let mut child_pool_idx = 0usize;
        let mut local_doc_offset = doc_offset;

        let Node::Internal { children, .. } = &self.pool[pool_idx] else {
            panic!(
                "index at `node_idx` in pool is not a `Node::Internal` despite passing a guard check."
            );
        };
        let last_idx = children.len() - 1;

        for (i, pool_idx) in children.iter().enumerate() {
            let child_len = self.pool[*pool_idx].len();

            if local_doc_offset <= child_len || i == last_idx {
                target_child_idx = i;
                child_pool_idx = *pool_idx;

                break;
            }

            local_doc_offset -= child_len;
        }

        if let Some(new_sibling) = self.insert_recursive(
            child_pool_idx,
            local_doc_offset,
            buf_offset,
            length,
            buf_kind,
        ) {
            let Node::Internal { children, .. } = &mut self.pool[pool_idx] else {
                panic!(
                    "index at `node_idx` in pool is not a `Node::Internal` despite passing a guard check."
                );
            };

            children.insert(target_child_idx + 1, new_sibling);

            if children.len() >= NODE_CHILDREN_CAPACITY {
                return Some(self.split_internal(pool_idx));
            }
        }

        None
    }
    /// # Purpose
    ///
    /// The primary entry point for inserting text into the Piece Table. Validates bounds,
    /// initializes the tree if it is empty, and spawns a new root if the original root splits.
    ///
    /// # Parameters
    ///
    /// - `doc_offset`: Where in the document text the insertion should occur.
    /// - `buf_offset`: Where in the physical memory buffer the raw text is stored.
    /// - `length`: The length of the inserted text.
    /// - `buf_kind`: The target memory buffer (`Original` or `Add`).
    ///
    /// # Returns
    ///
    /// - **`PieceTableResult<()>`**: `Ok(())` upon successful insertion.
    ///
    /// # Errors
    ///
    /// - Returns `PieceTableError::OutOfBounds` if `doc_offset` exceeds the total text length.
    ///
    /// # Panics
    ///
    /// - **When `root_idx` is `None`:**
    ///     - When `allocate_node` returns an index to a non-leaf node despite passing `true` to its parameter
    ///       to create a leaf node, or if the index it returns does not exist in the pool of this `PieceTable`.
    /// - **`When `root_idx` is changed because of a split:**
    ///     - When `allocate_node` returns an index to a leaf node (It should be a non-leaf node) despite
    ///       passing `false` to its parameter to create a leaf node, or if the index it returns does not exist in
    ///       the pool of this `PieceTable`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mynotes_core::piece_table::{PieceTable, BufferKind};
    /// let mut table = PieceTable::new();
    ///
    /// // Insert "Hello" (length 5) at document offset 0, physical buffer offset 0
    /// table.insert(0, 0, 5, BufferKind::Original).unwrap();
    ///
    /// // Insert "World" (length 5) contiguous to it
    /// table.insert(5, 5, 5, BufferKind::Original).unwrap();
    ///
    /// // The table should have merged them into one piece of length 10
    /// let (node_idx, piece_idx, local_offset) = table.get_location(7).unwrap();
    /// assert_eq!(local_offset, 7, "Index 7 should be at local offset 7 in the merged piece");
    /// ```
    pub fn insert(
        &mut self,
        doc_offset: usize,
        buf_offset: usize,
        length: usize,
        buf_kind: BufferKind,
    ) -> PieceTableResult<()> {
        let Some(prev_root_idx) = self.root_idx else {
            if doc_offset > 0 {
                return Err(PieceTableError::OutOfBounds {
                    requested: doc_offset,
                    max_length: 0,
                });
            }

            let new_root_idx = self.allocate_node(true);

            let Node::Leaf { pieces, total_len } = &mut self.pool[new_root_idx] else {
                panic!(
                    "`allocate_node` either created a non-leaf node or returned an invalid index."
                );
            };

            *total_len = length;

            self.root_idx = Some(new_root_idx);

            pieces.push(Piece {
                buf_kind,
                position: buf_offset..buf_offset + length,
            });

            return Ok(());
        };

        let total_doc_length = self.pool[prev_root_idx].len();

        if doc_offset > total_doc_length {
            return Err(PieceTableError::OutOfBounds {
                requested: doc_offset,
                max_length: total_doc_length,
            });
        }

        if let Some(new_sibling_idx) =
            self.insert_recursive(prev_root_idx, doc_offset, buf_offset, length, buf_kind)
        {
            let new_root_idx = self.allocate_node(false);

            let prev_root_weight = self.pool[prev_root_idx].len();
            let sibling_weight = self.pool[new_sibling_idx].len();

            let Node::Internal {
                children,
                total_len,
            } = &mut self.pool[new_root_idx]
            else {
                panic!("`allocate_node` either created a leaf node or returned an invalid index.")
            };

            children.push(prev_root_idx);
            children.push(new_sibling_idx);

            *total_len = prev_root_weight + sibling_weight;

            self.root_idx = Some(new_root_idx);
        }

        Ok(())
    }

    /// # Purpose
    ///
    /// Removes a specified length of text from a leaf node.
    /// Handles partial piece deletions, complete piece removals, and piece splitting
    /// (when a deletion occurs strictly inside the bounds of a single piece).
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The `PoolIndex` of the leaf node.
    /// - `doc_offset`: The local offset within this leaf where the deletion begins.
    /// - `length`: The number of bytes to remove.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `Node::Internal` instead of a `Leaf`.
    fn delete_in_leaf(&mut self, pool_idx: PoolIndex, mut doc_offset: usize, mut length: usize) {
        let Node::Leaf { pieces, .. } = &mut self.pool[pool_idx] else {
            panic!("`delete_in_leaf` called on a `Node::Internal`");
        };

        let mut i = 0;

        while i < pieces.len() && length > 0 {
            let piece_len = pieces[i].len();

            if doc_offset >= piece_len {
                // No overlap, skip this piece
                doc_offset -= piece_len;
                i += 1;

                continue;
            }

            // We found an overlap between the deletion range and this piece
            let overlap = min(length, piece_len - doc_offset);

            if doc_offset == 0 && overlap == piece_len {
                // 1. Exact match: Delete the entire piece
                pieces.remove(i);
                // Do not increment `i`, the next piece shifts into this index
            } else if doc_offset == 0 {
                // 2. Left trim: Deletion starts at the beginning of the piece
                pieces[i].position.start += overlap;
                i += 1;
            } else if doc_offset + overlap == piece_len {
                // 3. Right trim: Deletion ends exactly at the end of the piece
                pieces[i].position.end -= overlap;
                i += 1;
            } else {
                // 4. Middle split: Deletion is strictly inside the piece
                let original_end = pieces[i].position.end;

                // Trim the left piece
                pieces[i].position.end = pieces[i].position.start + doc_offset;

                // Create the right piece
                let right_piece = Piece {
                    buf_kind: pieces[i].buf_kind,
                    position: (pieces[i].position.start + doc_offset + overlap)..original_end,
                };

                pieces.insert(i + 1, right_piece);

                i += 2; // Skip both the left and right pieces
            }

            length -= overlap;
            doc_offset = 0; // For subsequent pieces, the deletion always starts at 0
        }
    }

    /// # Purpose
    ///
    /// Recursively routes a deletion request down the B-Tree.
    /// Decrements node weights, passes overlapping deletions to children, and handles
    /// any node splits that occur if a piece was split in half.
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The index of the current node.
    /// - `doc_offset`: The local offset where the deletion starts.
    /// - `length`: The length of text to delete.
    ///
    /// # Returns
    ///
    /// - **`Option<PoolIndex>`**: The index of a new right sibling if the node had to split.
    #[must_use]
    fn delete_recursive(
        &mut self,
        pool_idx: PoolIndex,
        mut doc_offset: usize,
        mut length: usize,
    ) -> Option<PoolIndex> {
        // Optimistically reduce the total length of this node
        *self.pool[pool_idx].mut_len() -= length;

        if self.pool[pool_idx].is_leaf() {
            self.delete_in_leaf(pool_idx, doc_offset, length);

            return self.pool[pool_idx]
                .is_full()
                .then(|| self.split_leaf(pool_idx));
        }

        let mut i = 0;

        while length > 0 {
            // 1. Grab child info (borrow scopes naturally drop here)
            let (child_idx, child_len) = {
                let Node::Internal { children, .. } = &self.pool[pool_idx] else {
                    unreachable!(
                        "Node at `pool_idx` must be `Internal` since `is_leaf` guard returned false earlier in `delete_recursive`."
                    );
                };
                let Some(&c) = children.get(i) else { break };

                (c, self.pool[c].len())
            };
            // 2. Math replaces branching: if doc_offset >= child_len, delete_len is 0.
            let delete_len = length.min(child_len.saturating_sub(doc_offset));

            if delete_len > 0 {
                if let Some(new_sibling) = self.delete_recursive(child_idx, doc_offset, delete_len)
                {
                    let Node::Internal { children, .. } = &mut self.pool[pool_idx] else {
                        unreachable!(
                            "Node at `pool_idx` must be `Internal` since `is_leaf` guard returned false earlier in `delete_recursive`."
                        );
                    };
                    children.insert(i + 1, new_sibling);
                    i += 1; // Skip the newly spawned sibling
                }

                length -= delete_len;
                doc_offset = 0;
            } else {
                doc_offset -= child_len; // Skip this child
            }

            i += 1;
        }

        // 3. Compact return
        let Node::Internal { children, .. } = &self.pool[pool_idx] else {
            unreachable!(
                "Node at `pool_idx` must be `Internal` since `is_leaf` guard returned false earlier in `delete_recursive`."
            );
        };

        (children.len() >= NODE_CHILDREN_CAPACITY).then(|| self.split_internal(pool_idx))
    }

    /// # Purpose
    ///
    /// Removes a specified range of text from the document.
    ///
    /// # Parameters
    ///
    /// - `doc_offset`: The absolute starting index of the text to remove.
    /// - `length`: The number of characters/bytes to remove.
    ///
    /// # Returns
    ///
    /// - **`PieceTableResult<()>`**: `Ok(())` upon successful deletion.
    ///
    /// # Errors
    ///
    /// - Returns `PieceTableError::OutOfBounds` if `doc_offset + length` exceeds the document length.
    pub fn delete(&mut self, doc_offset: usize, length: usize) -> PieceTableResult<()> {
        if length == 0 {
            return Ok(());
        }

        let Some(prev_root_idx) = self.root_idx else {
            return Err(PieceTableError::OutOfBounds {
                requested: doc_offset,
                max_length: 0,
            });
        };

        let total_doc_length = self.pool[prev_root_idx].len();

        if doc_offset + length > total_doc_length {
            return Err(PieceTableError::OutOfBounds {
                requested: doc_offset + length,
                max_length: total_doc_length,
            });
        }

        if let Some(new_sibling_idx) = self.delete_recursive(prev_root_idx, doc_offset, length) {
            let new_root_idx = self.allocate_node(false);

            let prev_root_weight = self.pool[prev_root_idx].len();
            let sibling_weight = self.pool[new_sibling_idx].len();

            let Node::Internal {
                children,
                total_len,
            } = &mut self.pool[new_root_idx]
            else {
                panic!("`allocate_node` either created a leaf node or returned an invalid index.")
            };

            children.push(prev_root_idx);
            children.push(new_sibling_idx);

            *total_len = prev_root_weight + sibling_weight;
            self.root_idx = Some(new_root_idx);
        }

        Ok(())
    }

    /// # Purpose
    ///
    /// Creates a lazy iterator that yields exact `Piece` slices for a specific document range.
    /// Jumps to the start index in O(log N) time and yields subsequent pieces in O(1) time.
    pub fn iter_range(&self, start: usize, length: usize) -> PieceTableIterator<'_> {
        let mut start = start;
        let mut stack = Vec::with_capacity(length);
        let mut offset_in_piece = 0;

        // Fast path for empty requests or empty trees
        if length == 0 || self.root_idx.is_none() {
            return PieceTableIterator {
                pool: &self.pool,
                stack,
                current_leaf: None,
                remaining_len: 0,
                offset_in_piece: 0,
            };
        }

        let mut current = self.root_idx.unwrap();

        while let Node::Internal { children, .. } = &self.pool[current] {
            let mut found = false;

            for (i, pool_idx) in children.iter().enumerate() {
                let child_len = self.pool[*pool_idx].len();

                if start < child_len {
                    stack.push((current, i + 1));
                    current = *pool_idx;
                    found = true;

                    start -= child_len;
                }
            }

            // If requested index is out of bounds, return an empty iterator
            if !found {
                return PieceTableIterator {
                    pool: &self.pool,
                    stack,
                    current_leaf: None,
                    remaining_len: 0,
                    offset_in_piece: 0,
                };
            }
        }

        // 2. We are at the Leaf. Find the exact piece containing the start index.
        let Node::Leaf { pieces, .. } = &self.pool[current] else {
            unreachable!(
                "After iterating through internal nodes, the final `current` should be a leaf, but it's not."
            )
        };
        let mut current_leaf = None;

        for (piece_idx, piece) in pieces.iter().enumerate() {
            let p_len = piece.len();

            if start < p_len {
                offset_in_piece = start;
                current_leaf = Some((current, piece_idx));
                break;
            }

            start -= p_len;
        }

        PieceTableIterator {
            pool: &self.pool,
            stack,
            current_leaf,
            remaining_len: length,
            offset_in_piece,
        }
    }
}

// Helper method to keep `next()` clean
impl<'a> PieceTableIterator<'a> {
    /// Pops back up the B-Tree stack and drills down to the next adjacent leaf
    fn advance_to_next_leaf(&mut self) -> Option<(PoolIndex, usize)> {
        while let Some((internal_idx, next_child_idx)) = self.stack.pop() {
            let Node::Internal { children, .. } = &self.pool[internal_idx] else {
                unreachable!("Stack must only contain Internal nodes.");
            };

            if next_child_idx < children.len() {
                // Update stack for when we eventually return to this internal node
                self.stack.push((internal_idx, next_child_idx + 1));

                // Drill completely down the left side of this new branch to find the next leaf
                let mut curr = children[next_child_idx];

                while let Node::Internal {
                    children: sub_children,
                    ..
                } = &self.pool[curr]
                {
                    self.stack.push((curr, 1)); // We visit index 0, so next is index 1
                    curr = sub_children[0];
                }

                // Return the new leaf, starting at piece index 0
                return Some((curr, 0));
            }
        }

        None // The stack is empty; we've reached the absolute end of the document
    }
}

impl<'pool> Iterator for PieceTableIterator<'pool> {
    type Item = Piece;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_len == 0 {
            return None; // We've yielded all requested characters!
        }

        loop {
            let (leaf_idx, piece_idx) = self.current_leaf?;
            let Node::Leaf { pieces, .. } = &self.pool[leaf_idx] else {
                unreachable!("`current_leaf` must always point to a Leaf node.");
            };

            // 1. Have we exhausted all pieces in this leaf? Time to jump to the right sibling.
            if piece_idx >= pieces.len() {
                self.current_leaf = self.advance_to_next_leaf();
                continue;
            }

            let piece = &pieces[piece_idx];
            let p_len = piece.len();

            // 2. Skip empty pieces or pieces we've fully bypassed via `offset_in_piece`
            if p_len == 0 || self.offset_in_piece >= p_len {
                self.current_leaf = Some((leaf_idx, piece_idx + 1));
                self.offset_in_piece = 0;
                continue;
            }

            // 3. Calculate exactly how much of this piece we are allowed to yield
            let available_in_piece = p_len - self.offset_in_piece;
            let yield_len = std::cmp::min(self.remaining_len, available_in_piece);
            // 4. Create a perfectly sized temporary Piece for the caller
            let yielded_piece = Piece {
                buf_kind: piece.buf_kind,
                position: (piece.position.start + self.offset_in_piece)
                    ..(piece.position.start + self.offset_in_piece + yield_len),
            };

            // 5. Advance our internal state for the next call
            self.remaining_len -= yield_len;
            self.current_leaf = Some((leaf_idx, piece_idx + 1));
            // Crucial: Only the very first piece yielded might have an offset.
            // All subsequent pieces start at index 0.
            self.offset_in_piece = 0;

            return Some(yielded_piece);
        }
    }
}
