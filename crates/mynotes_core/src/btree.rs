//! # Abstract Measured B-Tree
//!
//! A highly optimized, arena-allocated generic B-Tree designed to back spatial or measured
//! data structures like Ropes, Piece Tables, and Interval Trees.
//!
//! ## What it does
//! Unlike a standard key-value map, this `MeasuredBTree` does not route using absolute keys.
//! Instead, it routes based on a cumulative `Measure` (such as text length, line counts, or
//! arbitrary weights) provided by the `MeasuredBTreeData` trait. It maintains a balanced tree where
//! internal nodes cache the total measure of their subtrees, allowing rapid traversal to an
//! abstract accumulated offset.
//!
//! It manages its own memory using a 1D `pool` vector. Nodes are referenced via a `PoolIndex`
//! (`usize`) rather than standard Rust heap pointers (`Box` or `Rc`).
//!
//! ## Why it exists
//! When building text editors or managing massive continuous sequences, standard data
//! structures fall short:
//!
//! * **Standard `Vec`:** Inserting or deleting items requires shifting memory, resulting in
//!   $O(N)$ operations that freeze applications on large datasets.
//! * **Standard `MeasuredBTreeMap`:** Relies on discrete, absolute keys. If you insert a character at
//!   index 0 of a document, you would theoretically need to update the keys of every subsequent
//!   item, which degrades to $O(N)$.
//! * **Pointer-based Trees:** Standard node allocations scatter data across the heap, causing
//!   memory fragmentation and costly CPU cache misses.
//!
//! This `MeasuredBTree` solves these problems. By routing via cumulative measures, insertions and
//! deletions take $O(\log N)$ time because only the path from the modified leaf up to the
//! root needs its measures updated. Furthermore, the flattened `pool` architecture, combined
//! with `free_leaves_list` and `free_internals_list`, allows the tree to recycle deleted nodes
//! in $O(1)$ time, practically eliminating allocation overhead during heavy, continuous mutations.
//!
//! ## How it's used
//! 1. **Trait Implementation:** Define your core data type (e.g., a `Piece`) and implement the
//!    `MeasuredBTreeData` trait. This dictates how your data calculates its measure, splits itself
//!    (`split_at`), and combines with adjacent data (`try_merge`).
//! 2. **Initialization:** The `MeasuredBTree` is instantiated, and an initial root node is pushed
//!    into the `pool`.
//! 3. **Traversal:** Lookups use an absolute measure (e.g., a document character index). The
//!    tree descends by evaluating and subtracting the `measure` of preceding siblings from the
//!    target index until it isolates the correct leaf and data index.
//! 4. **Mutation:** When new data is inserted, the target leaf data is split using
//!    `MeasuredBTreeData::split_at`. If a node exceeds `DATA_CAPACITY` or `NODE_CHILDREN_CAPACITY`,
//!    it splits exactly in half, promoting the split upwards to maintain structural balance.

use crate::visualizer::Visualizer;
use num_traits::SaturatingSub;
use std::cmp::min;
use std::fmt::Debug;
use std::fmt::Write;
use std::iter::Sum;
use std::mem::take;
use std::ops::{Add, AddAssign, Range, Sub, SubAssign};
use thiserror::Error;

/// Alias for the `children` of `Node`,
/// which is a `usize` that refers to the index
/// of the child `Node` in the B-Tree 1D pool vector.
pub type PoolIndex = usize;
/// - **`M`**: The `Measure` of this `MeasuredBTree` for error formatting.
pub type MeasuredBTreeResult<T, M> = Result<T, MeasuredBTreeError<M>>;

/// 1024B base capacity for `PieceTable` vector
pub const BASE_CAPACITY: usize = 1024;
/// Minimum B-Tree degree
const T: usize = 16;
/// # Reasoning
///
/// The maximum amount of `MeasuredBTreeData` instances a `Node::Leaf` can hold. The formula is
/// `2 * T - 1` or twice the minimum B-Tree degree (branching factor)
/// because of B-Tree rules, which is, the tree should always be balanced.
///
/// During splitting, if we take one piece from the `Node` that's full,
/// we would be left with `(2 * T - 1) - 1` or `2 * T - 2`. If we were
/// to divide that between two `Node` instances (since we're splitting),
/// it would be equal to `T - 1`. With that, both `Node` instances
/// would have exactly `T - 1` `MeasuredBTreeData` instances.
///
/// This way, there would be no need to reallocate and deallocate the `MeasuredBTreeData` and
/// `children` vectors of a `Node` as often as it would.
///
/// # Example
///
/// Let T = 3
///
/// Minimum items: 3 - 1 = 2
/// Maximum items: 2 * 3 - 1 = 5
///
/// If a `Node` has 5 items `[A, B, C, D, E]`, it will split itself:
///
/// - **`[C]`:** The new node pushed to the parent
/// - **`[A, B]`:** The left node (exactly 2 items)
/// - **`[D, E]`:** The right node (exactly 2 items)
pub const DATA_CAPACITY: usize = 2 * T - 1;
/// From minimum degree `T`, double it
/// to be the maximum degree or capacity.
pub const NODE_CHILDREN_CAPACITY: usize = 2 * T;

pub trait Measure:
    Debug
    + Default
    + Clone
    + Copy
    + PartialOrd
    + Ord
    + Add<Output = Self>
    + AddAssign
    + Sub<Output = Self>
    + SubAssign
    + SaturatingSub
    + Sum
{
    fn clear(&mut self);
}

pub trait MeasuredBTreeData: Clone + Debug + Eq + PartialEq {
    type Measure: Measure;

    fn measure(&self) -> Self::Measure;

    /// Modifies `self` to become the left half, and returns the right half.
    fn split_at(&mut self, offset: Self::Measure) -> Self;

    /// Try to merge `other` into `self`
    ///
    /// # Returns
    ///
    /// true if `other` is consumed.
    fn try_merge(&mut self, other: &Self) -> bool;
}

#[derive(Error, Debug)]
pub enum MeasuredBTreeError<T: Debug> {
    #[error("The requested target ({requested:?}) is out of bounds (max: {max:?}).")]
    OutOfBounds { max: T, requested: T },
}

#[derive(Debug, Clone)]
pub enum MeasuredBTreeNode<T: MeasuredBTreeData> {
    Leaf {
        measure: T::Measure,
        data: Vec<T>,
    },
    Internal {
        measure: T::Measure,
        children: Vec<PoolIndex>,
    },
}

#[derive(Debug, Clone)]
pub struct MeasuredBTree<T: MeasuredBTreeData> {
    pub pool: Vec<MeasuredBTreeNode<T>>,
    pub root_idx: Option<PoolIndex>,

    free_leaves_list: Vec<PoolIndex>,
    free_internals_list: Vec<PoolIndex>,
}

impl<T> Measure for T
where
    T: Default
        + Clone
        + Copy
        + Debug
        + PartialOrd
        + Ord
        + Add<Output = Self>
        + AddAssign
        + Sub<Output = Self>
        + SubAssign
        + SaturatingSub
        + Sum,
{
    fn clear(&mut self) {
        *self = Self::default();
    }
}

#[must_use]
pub fn get_min_cap(is_leaf: bool) -> usize {
    if is_leaf {
        DATA_CAPACITY / 2
    } else {
        NODE_CHILDREN_CAPACITY / 2
    }
}

impl<T> MeasuredBTreeNode<T>
where
    T: MeasuredBTreeData,
{
    pub fn new_leaf() -> Self {
        Self::Leaf {
            data: Vec::with_capacity(DATA_CAPACITY),
            measure: T::Measure::default(),
        }
    }

    pub fn new_internal() -> Self {
        Self::Internal {
            children: Vec::with_capacity(NODE_CHILDREN_CAPACITY),
            measure: T::Measure::default(),
        }
    }

    /// # Returns
    ///
    /// A cloned `Measure`.
    #[inline]
    #[must_use]
    pub fn measure(&self) -> T::Measure {
        match self {
            Self::Leaf { measure, .. } => *measure,
            Self::Internal { measure, .. } => *measure,
        }
    }

    #[inline]
    #[must_use]
    pub fn mut_measure(&mut self) -> &mut T::Measure {
        match self {
            Self::Leaf { measure, .. } => measure,
            Self::Internal { measure, .. } => measure,
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.measure() == T::Measure::default()
    }

    #[inline]
    pub fn clear(&mut self) {
        match self {
            Self::Leaf { data, measure } => {
                data.clear();
                measure.clear();
            }
            Self::Internal { children, measure } => {
                children.clear();
                measure.clear();
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn is_leaf(&self) -> bool {
        match self {
            Self::Leaf { .. } => true,
            Self::Internal { .. } => false,
        }
    }

    /// # Purpose
    ///
    /// Used to check if this `MeasuredBTreeNode` has a length past or equal to its capacity.
    /// Defaults to `false` for `MeasuredBTreeNode::Internal` since they don't
    /// hold data.
    ///
    /// # Returns
    ///
    /// - **`true`**: If `data.len() >= DATA_CAPACITY` if and only if `children.len() >= NODE_CHILDREN_CAPACITY`.
    /// - **`false`**: If `data.len() < DATA_CAPACITY` or if `children.len() < NODE_CHILDREN_CAPACITY`.
    #[inline]
    #[must_use]
    pub fn is_full(&self) -> bool {
        match self {
            Self::Leaf { data, .. } => data.len() >= DATA_CAPACITY,
            Self::Internal { children, .. } => children.len() >= NODE_CHILDREN_CAPACITY,
        }
    }
}

impl<T> Default for MeasuredBTree<T>
where
    T: MeasuredBTreeData,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> MeasuredBTree<T>
where
    T: MeasuredBTreeData,
{
    pub fn new() -> Self {
        Self {
            pool: Vec::with_capacity(BASE_CAPACITY),
            root_idx: None,
            free_internals_list: Vec::new(),
            free_leaves_list: Vec::new(),
        }
    }

    /// # Purpose
    ///
    /// Deallocates a node and adds its index to the `free_list`.
    ///
    /// This is useful to avoid having to unnecessarily resize the
    /// vector on every deletion of a `MeasuredBTreeNode`.
    ///
    /// A `pool_idx` greater than the length of the `pool` is always a no-op.
    ///
    /// # Examples
    ///
    /// ```
    /// use mynotes_core::btree::MeasuredBTree;
    ///
    /// let tree = MeasuredBTree::new();
    ///
    /// tree.deallocate_node(1); // no op
    ///
    /// // Force allocate a node (pool is initially empty)
    /// let idx1 = tree.allocate_node(true); // Creates Leaf
    ///
    /// assert_eq!(idx1, 0);
    /// assert_eq!(tree.pool.len(), 1);
    ///
    /// // Deallocate it, pushing it to the free list
    /// tree.deallocate_node(idx1);
    ///
    /// // Next allocation should recycle idx 0 without growing the pool
    /// let idx2 = tree.allocate_node(true);
    ///
    /// assert_eq!(tree, 0);
    /// assert_eq!(tree.pool.len(), 1);
    /// ```
    pub fn deallocate_node(&mut self, pool_idx: PoolIndex) {
        if pool_idx >= self.pool.len() {
            return;
        }

        let node = &mut self.pool[pool_idx];

        node.clear();

        if node.is_leaf() {
            self.free_leaves_list.push(pool_idx);
        } else {
            self.free_internals_list.push(pool_idx);
        }
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
    /// use mynotes_core::btree::MeasuredBTree;
    ///
    /// let mut tree = MeasuredBTree::new();
    ///
    /// // Force allocate a node (pool is initially empty)
    /// let idx1 = tree.allocate_node(true); // Creates Leaf
    /// assert_eq!(idx1, 0);
    /// assert_eq!(tree.pool.len(), 1);
    /// // Deallocate it, pushing it to the free list
    /// tree.deallocate_node(idx1);
    ///
    /// // Next allocation should recycle idx 0 without growing the pool
    /// let idx2 = tree.allocate_node(true);
    ///
    /// assert_eq!(tree, 0);
    /// assert_eq!(tree.pool.len(), 1);
    /// ```
    pub fn allocate_node(&mut self, is_leaf: bool) -> PoolIndex {
        let pooled_idx = if is_leaf {
            self.free_leaves_list.pop()
        } else {
            self.free_internals_list.pop()
        };

        if let Some(idx) = pooled_idx {
            return idx;
        }

        let idx = self.pool.len();

        self.pool.push(if is_leaf {
            MeasuredBTreeNode::new_leaf()
        } else {
            MeasuredBTreeNode::new_internal()
        });

        idx
    }

    pub fn remove_child_from_parent(&mut self, parent_pool_idx: PoolIndex, child_index: usize) {
        let MeasuredBTreeNode::Internal { children, .. } = &mut self.pool[parent_pool_idx] else {
            unreachable!("removing child from parent is only available for internal nodes.");
        };

        let child_pool_idx = children.remove(child_index);

        self.deallocate_node(child_pool_idx);
    }

    pub fn update_internal_node_measure(&mut self, pool_index: PoolIndex) {
        let child_len =
            if let MeasuredBTreeNode::Internal { children, .. } = &mut self.pool[pool_index] {
                children.len()
            } else {
                unreachable!("update_internal_node_measure called on non internal node");
            };
        let new_measure = (0..child_len)
            .map(|i| {
                let child_idx = match &self.pool[pool_index] {
                    MeasuredBTreeNode::Internal { children, .. } => children[i],
                    _ => unreachable!("update_internal_node_measure called on non internal node"),
                };

                self.pool[child_idx].measure()
            })
            .sum::<T::Measure>();

        *self.pool[pool_index].mut_measure() = new_measure;
    }

    pub fn borrow_from_left_leaf(&mut self, left_pool_idx: PoolIndex, right_pool_idx: PoolIndex) {
        // 1. Pop the last piece from the left sibling
        let borrowed_piece = {
            let MeasuredBTreeNode::Leaf {
                data: left_data,
                measure: left_measure,
            } = &mut self.pool[left_pool_idx]
            else {
                unreachable!()
            };
            let piece = left_data
                .pop()
                .expect("Left leaf underflowed during borrow");

            // Recalculate left measure
            *left_measure = left_data.iter().map(|p| p.measure()).sum();
            piece
        };

        // 2. Insert it at the beginning of the right sibling
        {
            let MeasuredBTreeNode::Leaf {
                data: right_data,
                measure: right_measure,
            } = &mut self.pool[right_pool_idx]
            else {
                unreachable!()
            };
            right_data.insert(0, borrowed_piece);

            // Recalculate right measure
            *right_measure = right_data.iter().map(|p| p.measure()).sum();
        }
    }

    pub fn borrow_from_right_leaf(&mut self, right_pool_idx: PoolIndex, left_pool_idx: PoolIndex) {
        // 1. Remove the first piece from the right sibling
        let borrowed_piece = {
            let MeasuredBTreeNode::Leaf {
                data: right_data,
                measure: right_measure,
            } = &mut self.pool[right_pool_idx]
            else {
                unreachable!()
            };
            let piece = right_data.remove(0); // Panics if right_data is empty, which it shouldn't be

            // Recalculate right measure
            *right_measure = right_data.iter().map(|p| p.measure()).sum();
            piece
        };

        // 2. Push it to the end of the left sibling
        {
            let MeasuredBTreeNode::Leaf {
                data: left_data,
                measure: left_measure,
            } = &mut self.pool[left_pool_idx]
            else {
                unreachable!()
            };
            left_data.push(borrowed_piece);

            // Recalculate left measure
            *left_measure = left_data.iter().map(|p| p.measure()).sum();
        }
    }

    pub fn merge_leaves(&mut self, keep_pool_idx: PoolIndex, drop_pool_idx: PoolIndex) {
        // 1. Drain all data from the dropped leaf
        let mut dropped_data = {
            let MeasuredBTreeNode::Leaf { data, .. } = &mut self.pool[drop_pool_idx] else {
                unreachable!()
            };

            take(data)
        };

        // 2. Append all the drained data to the kept leaf
        {
            let MeasuredBTreeNode::Leaf {
                data: keep_data,
                measure: keep_measure,
            } = &mut self.pool[keep_pool_idx]
            else {
                unreachable!()
            };

            keep_data.append(&mut dropped_data);

            // Recalculate kept measure
            *keep_measure = keep_data.iter().map(|p| p.measure()).sum();
        }

        // 3. The drop_idx leaf is now empty. Deallocate it!
        self.deallocate_node(drop_pool_idx);
    }

    pub fn borrow_from_left_internal(
        &mut self,
        left_pool_idx: PoolIndex,
        right_pool_idx: PoolIndex,
    ) {
        // Pop the last child from the left sibling
        let borrowed_child = {
            let MeasuredBTreeNode::Internal {
                children: left_children,
                ..
            } = &mut self.pool[left_pool_idx]
            else {
                unreachable!()
            };
            left_children.pop().unwrap()
        };

        // Insert it at the beginning of the right sibling
        {
            let MeasuredBTreeNode::Internal {
                children: right_children,
                ..
            } = &mut self.pool[right_pool_idx]
            else {
                unreachable!()
            };
            right_children.insert(0, borrowed_child);
        }

        self.update_internal_node_measure(left_pool_idx);
        self.update_internal_node_measure(right_pool_idx);
    }

    pub fn borrow_from_right_internal(
        &mut self,
        right_pool_idx: PoolIndex,
        left_pool_idx: PoolIndex,
    ) {
        // Remove the first child from the right sibling
        let borrowed_child = {
            let MeasuredBTreeNode::Internal {
                children: right_children,
                ..
            } = &mut self.pool[right_pool_idx]
            else {
                unreachable!()
            };
            right_children.remove(0)
        };

        // Push it to the end of the left sibling
        {
            let MeasuredBTreeNode::Internal {
                children: left_children,
                ..
            } = &mut self.pool[left_pool_idx]
            else {
                unreachable!()
            };
            left_children.push(borrowed_child);
        }

        self.update_internal_node_measure(left_pool_idx);
        self.update_internal_node_measure(right_pool_idx);
    }

    pub fn merge_internals(&mut self, keep_pool_idx: PoolIndex, drop_pool_idx: PoolIndex) {
        // Drain all children from the dropped node
        let mut dropped_children = {
            let MeasuredBTreeNode::Internal { children, .. } = &mut self.pool[drop_pool_idx] else {
                unreachable!()
            };

            take(children)
        };

        // Append them to the kept node
        {
            let MeasuredBTreeNode::Internal {
                children: keep_children,
                ..
            } = &mut self.pool[keep_pool_idx]
            else {
                unreachable!()
            };

            keep_children.append(&mut dropped_children);
        }

        self.update_internal_node_measure(keep_pool_idx);

        // The drop_idx node is now empty and abandoned. Send it back to the pool!
        self.deallocate_node(drop_pool_idx);
    }

    /// # Purpose
    ///
    /// Splits a full `Leaf` node in half to maintain B-Tree balance properties.
    /// The right half of the pieces is extracted and moved into a newly allocated leaf node.
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The `PoolIndex` of the `MeasuredBTreeNode::Leaf` that has exceeded its capacity.
    ///
    /// # Returns
    ///
    /// - **`PoolIndex`**: The index of the newly created right sibling node.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `MeasuredBTreeNode::Internal` instead of a `Leaf`
    ///
    /// # Examples
    ///
    /// ```
    /// // Assume `pool_idx` points to a Leaf node that just exceeded `DATA_CAPACITY`.
    /// // let new_sibling_idx = table.split_leaf(pool_idx);
    ///
    /// // The original node now retains the left half of the data.
    /// // The newly allocated node (`new_sibling_idx`) holds the right half.
    /// // The total weight of both nodes combined equals the original node's weight.
    /// ```
    fn split_leaf(&mut self, pool_idx: PoolIndex) -> PoolIndex {
        let new_leaf_idx = self.allocate_node(true);
        let mut right_data = Vec::with_capacity(DATA_CAPACITY);

        {
            let MeasuredBTreeNode::Leaf { data, measure } = &mut self.pool[pool_idx] else {
                unreachable!("`split_leaf` is called within a `MeasuredBTreeNode::LeafNode`")
            };
            let split_at = data.len() / 2;

            right_data.extend(data.drain(split_at..));
            *measure = data.iter().map(|p| p.measure()).sum();
        }

        let MeasuredBTreeNode::Leaf {
            data: new_data,
            measure: new_measure,
        } = &mut self.pool[new_leaf_idx]
        else {
            unreachable!("`split_leaf` is called within a `MeasuredBTreeNode::LeafNode`")
        };
        let right_weight = right_data.iter().map(|d| d.measure()).sum::<T::Measure>();

        *new_data = right_data;
        *new_measure = right_weight;

        new_leaf_idx
    }

    /// # Purpose
    ///
    /// Splits a full `Internal` node in half to maintain B-Tree balance properties.
    /// The right half of the child pointers is moved into a newly allocated internal node.
    ///
    /// # Parameters
    ///
    /// - `pool_idx`: The `PoolIndex` of the `MeasuredBTreeNode::Internal` that has exceeded its capacity.
    ///
    /// # Returns
    ///
    /// - **`PoolIndex`**: The index of the newly created right sibling node.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `MeasuredBTreeNode::Leaf` instead of an `Internal` node.
    /// - If `allocate_node(false)` returns a `MeasuredBTreeNode::Leaf`.
    #[must_use]
    fn split_internal(&mut self, pool_idx: PoolIndex) -> PoolIndex {
        let right_children = match &mut self.pool[pool_idx] {
            MeasuredBTreeNode::Internal { children, .. } => children.split_off(children.len() / 2),
            MeasuredBTreeNode::Leaf { .. } => {
                unreachable!("`split_internal` is called within a `MeasuredBTreeNode::Internal`")
            }
        };

        {
            let left_weight = match &self.pool[pool_idx] {
                MeasuredBTreeNode::Internal { children, .. } => children
                    .iter()
                    .map(|pool_idx| self.pool[*pool_idx].measure())
                    .sum::<T::Measure>(),
                MeasuredBTreeNode::Leaf { .. } => {
                    unreachable!("`split_internal` is called within a `Node::Leaf`")
                }
            };

            *self.pool[pool_idx].mut_measure() = left_weight;
        }

        let new_idx = self.allocate_node(false);
        let right_children_measure = right_children
            .iter()
            .map(|pool_idx| self.pool[*pool_idx].measure())
            .sum::<T::Measure>();

        if let MeasuredBTreeNode::Internal { children, measure } = &mut self.pool[new_idx] {
            *measure = right_children_measure;
            *children = right_children;
        }

        new_idx
    }

    /// # Purpose
    ///
    /// The primary entry point for inserting data for this `MeasuredBTree`. Validates bounds,
    /// initializes the tree if it is empty, and spawns a new root if the original root splits.
    ///
    /// # Parameters
    ///
    /// - **`target`:** The target `T::Measure`to insert the new data `T`.
    /// - **`data`**: The new data `T` to be inserted.
    ///
    /// # Returns
    ///
    /// - **`MeasuredBTreeResult<(), T::Measure>`**: `Ok(())` upon successful insertion.
    ///
    /// # Errors
    ///
    /// - Returns `MeasuredBTreeError::OutOfBounds` if `target` exceeds the total `Measure` of this `MeasuredBTree`.
    ///
    /// # Panics
    ///
    /// - **When `root_idx` is `None`:**
    ///     - When `allocate_node` returns an index to a non-leaf node despite passing `true` to its parameter
    ///       to create a leaf node, or if the index it returns does not exist in the pool of this `MeasuredBTree`.
    /// - **`When `root_idx` is changed because of a split:**
    ///     - When `allocate_node` returns an index to a leaf node (It should be a non-leaf node) despite
    ///       passing `false` to its parameter to create a leaf node, or if the index it returns does not exist in
    ///       the pool of this `MeasuredBTree`.
    ///
    pub fn insert(&mut self, target: T::Measure, data: T) -> MeasuredBTreeResult<(), T::Measure> {
        let Some(prev_root_idx) = self.root_idx else {
            return self.init_empty_root(target, data);
        };

        let total_measure = self.pool[prev_root_idx].measure();

        if target > total_measure {
            return Err(MeasuredBTreeError::OutOfBounds {
                max: total_measure,
                requested: target,
            });
        }

        if let Some(new_sibling_idx) = self.insert_recursive(prev_root_idx, target, data) {
            self.grow_tree_height(prev_root_idx, new_sibling_idx);
        }

        Ok(())
    }

    /// Creates the very first leaf node when inserting into an empty tree.
    fn init_empty_root(
        &mut self,
        target: T::Measure,
        data: T,
    ) -> MeasuredBTreeResult<(), T::Measure> {
        let zero = T::Measure::default();

        if target > zero {
            return Err(MeasuredBTreeError::OutOfBounds {
                requested: target,
                max: zero,
            });
        }

        let new_root_idx = self.allocate_node(true);
        let MeasuredBTreeNode::Leaf {
            data: leaf_data,
            measure,
        } = &mut self.pool[new_root_idx]
        else {
            unreachable!("`allocate_node(true)` must return a Leaf node.");
        };

        *measure = data.measure();
        leaf_data.push(data);

        self.root_idx = Some(new_root_idx);

        Ok(())
    }

    /// Wraps a split root into a new Internal node, increasing the tree's height by 1.
    fn grow_tree_height(&mut self, old_root_idx: PoolIndex, new_sibling_idx: PoolIndex) {
        let new_root_idx = self.allocate_node(false);

        // Grab measures before mutating the pool
        let left_measure = self.pool[old_root_idx].measure();
        let right_measure = self.pool[new_sibling_idx].measure();

        let MeasuredBTreeNode::Internal { children, measure } = &mut self.pool[new_root_idx] else {
            unreachable!("`allocate_node(false)` must return an Internal node.");
        };

        children.push(old_root_idx);
        children.push(new_sibling_idx);
        *measure = left_measure + right_measure;

        self.root_idx = Some(new_root_idx);
    }

    /// # Purpose
    ///
    /// Recursively routes an insertion request down the B-Tree based on the document offset.
    /// Optimistically updates node weights on the way down, and handles child splits on the way back up.
    ///
    /// # Parameters
    ///
    /// - **`pool_idx`**: The index of the current node being traversed.
    /// - **`target`**: The `T::Measure` to find where the new data `T` will be inserted.
    /// - **`data`**: The new data `T` to be inserted.
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
        target: T::Measure,
        data: T,
    ) -> Option<PoolIndex> {
        *self.pool[pool_idx].mut_measure() += data.measure();

        if self.pool[pool_idx].is_leaf() {
            self.insert_into_leaf(pool_idx, target, data);

            return self.pool[pool_idx]
                .is_full()
                .then(|| self.split_leaf(pool_idx));
        }

        let mut target_child_idx = 0usize;
        let mut child_pool_idx = 0usize;
        let mut local_target = target;

        let MeasuredBTreeNode::Internal { children, .. } = &self.pool[pool_idx] else {
            unreachable!(
                "index at `node_idx` in pool is not a `Node::Internal` despite passing a guard check."
            );
        };

        let last_idx = children.len() - 1;

        for (i, pool_idx) in children.iter().enumerate() {
            let child_measure = self.pool[*pool_idx].measure();

            if local_target <= child_measure || i == last_idx {
                target_child_idx = i;
                child_pool_idx = *pool_idx;

                break;
            }

            local_target -= child_measure;
        }

        if let Some(new_sibling_idx) = self.insert_recursive(child_pool_idx, local_target, data) {
            let MeasuredBTreeNode::Internal { children, .. } = &mut self.pool[pool_idx] else {
                unreachable!(
                    "index at `node_idx` in pool is not a `MeasuredBTreeNode::Internal` despite passing a guard check."
                );
            };

            children.insert(target_child_idx + 1, new_sibling_idx);

            if children.len() >= NODE_CHILDREN_CAPACITY {
                return Some(self.split_internal(pool_idx));
            }
        }

        None
    }

    /// # Purpose
    ///
    /// Inserts a new `T` into a specific leaf node. If the new `T` is contiguous
    /// with an existing `T` from the same buffer, it tries to merge them to save memory. Otherwise,
    /// it inserts the new `T` instead.
    ///
    /// # Parameters
    ///
    /// - **`pool_idx`:** The `PoolIndex` of the leaf node.
    /// - **`target`:** The `T::Measure` of the node for finding the specific entry point to insert the new `T`
    /// - **`data`:** The new data `T` to be inserted.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `Node::Internal` instead of a `Leaf`.
    fn insert_into_leaf(&mut self, pool_idx: PoolIndex, target: T::Measure, data: T) {
        let MeasuredBTreeNode::Leaf {
            data: leaf_data, ..
        } = &mut self.pool[pool_idx]
        else {
            unreachable!("`insert_into_leaf` called on a `Node::Internal`");
        };
        let mut data_idx = 0usize;
        let mut local_target = target;
        let zero = T::Measure::default();

        while data_idx < leaf_data.len() && local_target > leaf_data[data_idx].measure() {
            local_target -= leaf_data[data_idx].measure();
            data_idx += 1;
        }

        // If we are inside an item, split it.
        if data_idx < leaf_data.len()
            && local_target > zero
            && local_target < leaf_data[data_idx].measure()
        {
            let right_half = leaf_data[data_idx].split_at(local_target);

            if leaf_data[data_idx].try_merge(&data) {
                leaf_data.insert(data_idx + 1, right_half);
            } else {
                leaf_data.insert(data_idx + 1, data);
                leaf_data.insert(data_idx + 2, right_half);
            }

            return;
        }

        // We are at a clean boundary
        // Find where to insert and which item to try merging with
        let (insert_idx, prev_idx) = if data_idx == leaf_data.len() || local_target == zero {
            (data_idx, data_idx.checked_sub(1))
        } else {
            // local_target == leaf_data[data_idx].measure()
            (data_idx + 1, Some(data_idx))
        };

        // 5. Try merging with the previous item
        if let Some(p_idx) = prev_idx
            && leaf_data[p_idx].try_merge(&data)
        {
            return;
        }

        // 6. Merge failed, just insert it normally as a distinct item
        leaf_data.insert(insert_idx, data);
    }

    /// # Purpose
    ///
    /// Removes a specified range of data.
    ///
    /// # Parameters
    ///
    /// - `range`: The range to delete.
    ///
    /// # Returns
    ///
    /// - **`MeasuredBTreeResult<(), T::Measure>`**: `Ok(())` upon successful deletion.
    ///
    /// # Errors
    ///
    /// - Returns `MeasuredBTreeError::OutOfBounds` if `range` exceeds the total `T::Measure`.
    pub fn delete(&mut self, range: Range<T::Measure>) -> MeasuredBTreeResult<(), T::Measure> {
        if range.end < range.start {
            return Ok(());
        }

        let length = range.end - range.start;
        let zero = T::Measure::default();

        if length == zero {
            return Ok(());
        }

        let Some(prev_root_idx) = self.root_idx else {
            return Err(MeasuredBTreeError::OutOfBounds {
                requested: range.start,
                max: zero,
            });
        };

        let total_measure = self.pool[prev_root_idx].measure();

        if range.end > total_measure {
            return Err(MeasuredBTreeError::OutOfBounds {
                requested: range.end,
                max: total_measure,
            });
        }

        if let Some(new_sibling_idx) = self.delete_recursive(prev_root_idx, range.start, length) {
            self.grow_tree_height(prev_root_idx, new_sibling_idx);
        }

        self.shrink_tree_height();

        Ok(())
    }

    /// Checks if the root has collapsed and shrinks the tree height by 1.
    fn shrink_tree_height(&mut self) {
        let Some(root_idx) = self.root_idx else {
            return;
        };

        match &self.pool[root_idx] {
            MeasuredBTreeNode::Internal { children, .. } if children.len() == 1 => {
                // The root merged down to a single child. That child becomes the new root.
                self.root_idx = Some(children[0]);

                self.deallocate_node(root_idx);
            }
            MeasuredBTreeNode::Leaf { data, .. } if data.is_empty() => {
                // The last leaf in the tree is empty.
                self.root_idx = None;

                self.deallocate_node(root_idx);
            }
            _ => {} // Root is fine
        }
    }

    fn balance_child(&mut self, parent_pool_idx: PoolIndex, child_i: usize) {
        let child_pool_idx = self.get_child_idx(parent_pool_idx, child_i);
        let parent_len = self.node_len(parent_pool_idx);
        let is_leaf = self.is_child_leaf(parent_pool_idx, child_i);
        let min_cap = get_min_cap(is_leaf);
        // 1. Define siblings as Option<(usize, bool)>.
        // ThisG allows `.flatten()` to perfectly skip out-of-bounds (None) siblings!
        let siblings = [
            child_i.checked_sub(1).map(|i| (i, false)), // Left
            (child_i + 1 < parent_len).then(|| (child_i + 1, true)), // Right
        ];

        // 2. Try Borrowing
        // .flatten() safely unwraps the Some values and ignores the Nones.
        for (sibling_idx, is_right) in siblings.into_iter().flatten() {
            let sibling_pool_idx = self.get_child_idx(parent_pool_idx, sibling_idx);

            if self.node_len(sibling_pool_idx) <= min_cap {
                continue;
            }

            match (is_leaf, is_right) {
                (true, true) => self.borrow_from_right_leaf(sibling_pool_idx, child_pool_idx),
                (true, false) => self.borrow_from_left_leaf(sibling_pool_idx, child_pool_idx),
                (false, true) => self.borrow_from_right_internal(sibling_pool_idx, child_pool_idx),
                (false, false) => self.borrow_from_left_internal(sibling_pool_idx, child_pool_idx),
            }

            return;
        }

        // 3. Fallback: Merge
        // .flatten().next() grabs the first valid sibling (preferring left, then right)
        let Some((sibling_idx, is_right)) = siblings.into_iter().flatten().next() else {
            unreachable!("The first valid sibling must exist.");
        };
        let sibling_pool_idx = self.get_child_idx(parent_pool_idx, sibling_idx);
        let (left_pool_idx, right_pool_idx) = if is_right {
            (child_pool_idx, sibling_pool_idx)
        } else {
            (sibling_pool_idx, child_pool_idx)
        };

        if is_leaf {
            self.merge_leaves(left_pool_idx, right_pool_idx);
        } else {
            self.merge_internals(left_pool_idx, right_pool_idx);
        }

        self.remove_child_from_parent(
            parent_pool_idx,
            if is_right { sibling_idx } else { child_i },
        );
    }

    /// Iterates through children and triggers borrow/merge logic for any that underflowed.
    fn rebalance_children(&mut self, parent_pool_idx: PoolIndex) {
        let mut idx = 0;

        while idx < self.node_len(parent_pool_idx) {
            let mut merged = false;

            // LOOP! Because a piece table deletion can wipe out multiple pieces,
            // we must keep borrowing from the sibling until we reach min_cap!
            loop {
                let child_pool_idx = self.get_child_idx(parent_pool_idx, idx);
                // Just ensure you have access to is_leaf here like you do in balance_child
                let is_leaf = self.pool[child_pool_idx].is_leaf();
                let min_cap = get_min_cap(is_leaf);

                // If the child is healthy, break out of the borrow loop
                if self.node_len(child_pool_idx) >= min_cap {
                    break;
                }

                let len_before = self.node_len(parent_pool_idx);

                self.balance_child(parent_pool_idx, idx);

                // If a merge happened, the parent's children array shifted.
                if self.node_len(parent_pool_idx) < len_before {
                    merged = true;
                    break;
                }
            }

            // If we merged, restart the scan to prevent out-of-bounds indexing
            if merged {
                idx = 0;
                continue;
            }

            // Only move to the next child if the current one is fully healed
            idx += 1;
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
    /// - **`pool_idx`:** The index of the current node.
    /// - **`target`:** The local offset where the deletion starts.
    /// - **`length`:** The length of data to delete.
    ///
    /// # Panics
    ///
    /// - **If a `MeasuredBTreeNode` at `pool_idx` is not a `MeasuredBTreeNode::Internal`:**
    ///     - This happens if the guard before the panic is not correct. Before asserting that `self.pool[node_idx]`
    ///       is a `MeasuredBTreeNode::Internal`, there should first be a check if it's a `Node::LeafNode` and handle it accordingly.
    ///
    /// # Returns
    ///
    /// - **`Option<PoolIndex>`**: The index of a new right sibling if the node had to split.
    #[must_use]
    fn delete_recursive(
        &mut self,
        pool_idx: PoolIndex,
        target: T::Measure,
        length: T::Measure,
    ) -> Option<PoolIndex> {
        *self.pool[pool_idx].mut_measure() -= length;

        if self.pool[pool_idx].is_leaf() {
            self.delete_in_leaf(pool_idx, target, length);

            return self.pool[pool_idx]
                .is_full()
                .then(|| self.split_leaf(pool_idx));
        }

        let mut idx = 0;
        let mut local_length = length;
        let mut local_target = target;
        let zero = T::Measure::default();

        while local_length > zero {
            let (child_idx, child_measure) = {
                let MeasuredBTreeNode::Internal { children, .. } = &self.pool[pool_idx] else {
                    unreachable!(
                        "Node at `pool_idx` must be `Internal` since `is_leaf` guard returned false earlier in `delete_recursive`."
                    );
                };
                let Some(&c) = children.get(idx) else { break };

                (c, self.pool[c].measure())
            };

            if local_target >= child_measure {
                local_target -= child_measure;
                idx += 1;

                continue;
            }

            let delete_measure = local_length.min(child_measure.saturating_sub(&local_target));

            if let Some(new_sibling) =
                self.delete_recursive(child_idx, local_target, delete_measure)
            {
                let MeasuredBTreeNode::Internal { children, .. } = &mut self.pool[pool_idx] else {
                    unreachable!(
                        "`MeasuredBTreeNode` at `pool_idx` must be `Internal` since `is_leaf` guard returned false earlier in `delete_recursive`."
                    );
                };

                children.insert(idx + 1, new_sibling);
                idx += 1;
            }

            local_length -= delete_measure;
            local_target = zero;
            idx += 1;
        }

        self.rebalance_children(pool_idx);

        self.pool[pool_idx]
            .is_full()
            .then(|| self.split_internal(pool_idx))
    }

    /// # Purpose
    ///
    /// Removes a specified length of text from a leaf node.
    /// Handles partial piece deletions, complete piece removals, and piece splitting
    /// (when a deletion occurs strictly inside the bounds of a single piece).
    ///
    /// # Parameters
    ///
    /// - **`pool_idx`:** The `PoolIndex` of the leaf node.
    /// - **`target`:*** The target `T::Measure` to find where to delete in the leaf.
    ///
    /// # Panics
    ///
    /// - If `pool_idx` points to a `MeasuredBTreeNode::Internal` instead of a `Leaf`.
    fn delete_in_leaf(&mut self, pool_idx: PoolIndex, target: T::Measure, length: T::Measure) {
        let MeasuredBTreeNode::Leaf {
            data: leaf_data, ..
        } = &mut self.pool[pool_idx]
        else {
            unreachable!("`delete_in_leaf` called on a `MeasuredBTreeNode::Internal`");
        };

        let mut idx = 0;
        let mut local_target = target;
        let zero = T::Measure::default();

        while idx < leaf_data.len() && local_target >= leaf_data[idx].measure() {
            local_target -= leaf_data[idx].measure();
            idx += 1;
        }

        let mut local_length = length;

        while idx < leaf_data.len() && local_length > zero {
            let data_measure = leaf_data[idx].measure();
            let overlap = min(local_length, data_measure - local_target);

            if local_target == zero && overlap == data_measure {
                leaf_data.remove(idx);
                // Do not increment `i`, the next piece shifts into this index
            } else if local_target == zero {
                // 2. Left trim: Deletion starts at the beginning of the data.
                // `split_at(overlap)` makes `self` the deleted left part, and returns the right part to keep.
                let right_keep = leaf_data[idx].split_at(overlap);

                leaf_data[idx] = right_keep; // Overwrite the piece with the kept portion

                idx += 1;
            } else if local_target + overlap == data_measure {
                // 3. Right trim: Deletion ends exactly at the end of the data.
                // `split_at(target)` makes `self` the left part to keep.
                // We simply drop the returned right part (the deleted portion) into the void.
                let _discarded_right = leaf_data[idx].split_at(target);

                idx += 1;
            } else {
                // 4. Middle split: Deletion is strictly inside the data.
                // We need to keep the left side and the right side, but drop the middle.

                // First, split off the left side to keep it
                let mut mid_and_right = leaf_data[idx].split_at(target);
                // Then, split the remainder to isolate the middle (which we drop) and the right (which we keep)
                let right_keep = mid_and_right.split_at(overlap);

                leaf_data.insert(idx + 1, right_keep);

                idx += 2; // Skip both the left-keep and right-keep pieces
            }

            local_length -= overlap;
            local_target = zero;
        }
    }

    /// # Purpose
    pub fn clear(&mut self) {
        self.pool.clear();
        self.free_leaves_list.clear();
        self.free_internals_list.clear();
        self.pool.shrink_to(BASE_CAPACITY);

        self.root_idx = None;
    }

    /// # Returns
    ///
    /// The total `Measure` inside this MeasuredBTree, or
    /// metric about its data's totality.
    #[inline]
    #[must_use]
    pub fn total_measure(&self) -> T::Measure {
        let Some(root_idx) = self.root_idx else {
            return T::Measure::default();
        };

        self.pool[root_idx].measure()
    }

    /// # Purpose
    #[inline]
    #[must_use]
    pub fn node_len(&self, pool_index: PoolIndex) -> usize {
        match &self.pool[pool_index] {
            MeasuredBTreeNode::Leaf { data, .. } => data.len(),
            MeasuredBTreeNode::Internal { children, .. } => children.len(),
        }
    }

    /// # Returns
    ///
    /// true if empty, otherwise false.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root_idx.is_none() || self.total_measure() == T::Measure::default()
    }

    /// # Purpose
    ///
    /// # Panics
    #[inline]
    #[must_use]
    pub fn get_child_idx(&self, parent_idx: PoolIndex, i: usize) -> PoolIndex {
        match &self.pool[parent_idx] {
            MeasuredBTreeNode::Internal { children, .. } => children[i],
            _ => unreachable!("get_child_idx can only be called on an internal node"),
        }
    }

    /// # Purpose
    ///
    /// # Panics
    #[inline]
    #[must_use]
    pub fn is_child_leaf(&self, parent_pool_idx: PoolIndex, child_i: usize) -> bool {
        match &self.pool[parent_pool_idx] {
            MeasuredBTreeNode::Internal { children, .. } => self.pool[children[child_i]].is_leaf(),
            _ => unreachable!(),
        }
    }

    #[inline]
    #[must_use]
    pub fn get_at(&self, target: T::Measure) -> Option<(&T, T::Measure)> {
        let root_idx = self.root_idx?;

        self.get_recursive(root_idx, target)
    }

    /// Mutable version of `get_at`. Useful if you need to update an item's metadata
    /// in place without altering its measure.
    #[inline]
    #[must_use]
    pub fn get_mut_at(&mut self, target: T::Measure) -> Option<(&mut T, T::Measure)> {
        let root_idx = self.root_idx?;

        self.get_mut_recursive(root_idx, target)
    }

    /// # Purpose
    ///
    /// This is incredibly useful for writing Iterators that need to start exactly at
    /// a specific cursor position.
    ///
    /// # Returns
    ///
    /// A tuple of:
    ///
    /// - **`PoolIndex:`** the index of the node in the pool containing the target `Measure`.
    /// - **`usize`**: the index of the `Measure` in the `MeasuredBTreeNode`.
    /// - **`usize`**: the `Measure` of the item in the `data`, or its offset.
    ///
    /// The raw memory pool index of the leaf node containing the target measure,
    /// the index of the item within that leaf's `data` array, and the local offset.
    #[must_use]
    pub fn get_location(&self, target: T::Measure) -> Option<(PoolIndex, usize, T::Measure)> {
        let mut pool_idx = self.root_idx?;
        let mut local_target = target;

        if local_target >= self.pool[pool_idx].measure() {
            return None;
        }

        loop {
            match &self.pool[pool_idx] {
                MeasuredBTreeNode::Internal { children, .. } => {
                    pool_idx = children
                        .iter()
                        .find_map(|&pool_idx| {
                            let child_weight = self.pool[pool_idx].measure();

                            if local_target < child_weight {
                                Some(pool_idx)
                            } else {
                                local_target -= child_weight;

                                None
                            }
                        })
                        .expect("Bounds check guarantees that a routing child exists.");
                }
                MeasuredBTreeNode::Leaf { data, .. } => {
                    let data_idx = data
                        .iter()
                        .enumerate()
                        .find_map(|(idx, data_entry)| {
                            let entry_measure = data_entry.measure();

                            if local_target < entry_measure {
                                Some(idx)
                            } else {
                                local_target -= entry_measure;

                                None
                            }
                        })
                        .expect("Bounds check guarantees that a data entry exists.");

                    return Some((pool_idx, data_idx, local_target));
                }
            }
        }
    }

    /// # Purpose
    ///
    /// Fetches a standard slice of items if your target cleanly aligns with item boundaries.
    /// Mostly used for debugging or contiguous reads.
    pub fn get_leaf_data(&self, pool_idx: PoolIndex) -> Option<&[T]> {
        if let Some(MeasuredBTreeNode::Leaf { data, .. }) = self.pool.get(pool_idx) {
            Some(data)
        } else {
            None
        }
    }

    fn get_recursive(&self, pool_idx: PoolIndex, target: T::Measure) -> Option<(&T, T::Measure)> {
        match &self.pool[pool_idx] {
            MeasuredBTreeNode::Leaf { data, .. } => {
                let mut current_offset = T::Measure::default();

                for item in data {
                    let item_measure = item.measure();
                    let next_offset = current_offset + item_measure;

                    // If target falls inside this item (or exactly at its start)
                    if target >= current_offset && target < next_offset {
                        let local_offset = target - current_offset;
                        return Some((item, local_offset));
                    }

                    current_offset = next_offset;
                }
                None // Out of bounds within the leaf
            }
            MeasuredBTreeNode::Internal { children, .. } => {
                let mut local_target = target;
                let last_idx = children.len().saturating_sub(1);

                for (i, &child_idx) in children.iter().enumerate() {
                    let child_measure = self.pool[child_idx].measure();

                    if local_target < child_measure || i == last_idx {
                        return self.get_recursive(child_idx, local_target);
                    }

                    local_target -= child_measure;
                }
                None
            }
        }
    }

    fn get_mut_recursive(
        &mut self,
        pool_idx: PoolIndex,
        target: T::Measure,
    ) -> Option<(&mut T, T::Measure)> {
        let mut local_target = target;
        let route = match &self.pool[pool_idx] {
            MeasuredBTreeNode::Leaf { .. } => None,
            MeasuredBTreeNode::Internal { children, .. } => {
                let mut next_idx = 0;
                let last = children.len().saturating_sub(1);

                for (i, &child_idx) in children.iter().enumerate() {
                    let child_measure = self.pool[child_idx].measure();

                    if local_target < child_measure || i == last {
                        next_idx = child_idx;

                        break;
                    }

                    local_target -= child_measure;
                }

                Some(next_idx)
            }
        };

        if let Some(next_idx) = route {
            self.get_mut_recursive(next_idx, local_target)
        } else if let MeasuredBTreeNode::Leaf { data, .. } = &mut self.pool[pool_idx] {
            let mut current = T::Measure::default();

            for item in data {
                let next = current + item.measure();

                if target >= current && local_target < next {
                    return Some((item, local_target - current));
                }

                current = next;
            }
            None
        } else {
            unreachable!("Failed to get an internal node.");
        }
    }
}

impl<T> Visualizer for MeasuredBTree<T>
where
    T: MeasuredBTreeData,
{
    fn visualize(&self) -> String {
        let mut dot = String::new();
        let _ = writeln!(&mut dot, "digraph BTree {{");
        let _ = writeln!(&mut dot, "  node [shape=record, height=.1];");

        if let Some(root_idx) = self.root_idx {
            self.dot_recursive(root_idx, &mut dot);
        } else {
            let _ = writeln!(&mut dot, "  empty [label=\"Empty Tree\"];");
        }

        let _ = writeln!(&mut dot, "}}");
        dot
    }
}

impl<T> MeasuredBTree<T>
where
    T: MeasuredBTreeData,
{
    pub fn dot_recursive(&self, pool_idx: PoolIndex, dot: &mut String) {
        match &self.pool[pool_idx] {
            MeasuredBTreeNode::Internal { children, measure } => {
                // 1. Draw the node itself
                let _ = write!(
                    dot,
                    "  node{} [label=\"{{I: {} | w: {:?}",
                    pool_idx, pool_idx, measure
                );
                for i in 0..children.len() {
                    let _ = write!(dot, " | <c{}> *", i); // Child pointers
                }
                let _ = writeln!(dot, "}}\"];");

                // 2. Draw arrows to children and recurse
                for (i, &child_idx) in children.iter().enumerate() {
                    let _ = writeln!(dot, "  node{}:c{} -> node{};", pool_idx, i, child_idx);
                    self.dot_recursive(child_idx, dot);
                }
            }
            MeasuredBTreeNode::Leaf { data, measure } => {
                // 1. Start the label
                let mut label = format!("{{L: {} | w: {:?}", pool_idx, measure);

                // 2. Append each piece, but sanitize the string first!
                for item in data {
                    let debug_str = format!("{:?}", item);
                    let safe_str = debug_str
                        .replace('{', "\\{")
                        .replace('}', "\\}")
                        .replace('<', "\\<")
                        .replace('>', "\\>")
                        .replace('|', "\\|");

                    let wrapped_str = safe_str
                        .chars()
                        .collect::<Vec<char>>()
                        .chunks(35)
                        .map(|chunk| chunk.iter().collect::<String>())
                        .collect::<Vec<String>>()
                        .join("\\l");

                    let _ = write!(label, " | {}", wrapped_str);
                }

                // 3. Close the label and write to the main dot string
                let _ = writeln!(
                    dot,
                    "  node{} [label=\"{}}}\", style=filled, fillcolor=lightgrey];",
                    pool_idx, label
                );
            }
        }
    }
}
