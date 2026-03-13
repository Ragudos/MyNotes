use std::{
    fmt::Debug,
    iter::Sum,
    ops::{Add, AddAssign, Sub, SubAssign},
};

use num_traits::SaturatingSub;

use crate::data_structures::object_pool::Poolable;

/// # Purpose
///
/// Defines the `Node` trait, which represents a node in a
/// measured B-tree. This trait is used to abstract over the
/// specific types of nodes (e.g., leaf nodes and internal nodes)
/// that can exist in a measured B-tree.
pub trait Node: Debug + Clone + Poolable {
    /// The type of measure associated with this node.
    type NodeMeasure: Measure;

    /// Gets the measure associated with this node.
    #[must_use]
    fn get_measure(&self) -> Self::NodeMeasure;

    /// Gets a mutable reference to the measure associated with this node.
    #[must_use]
    fn get_mut_measure(&mut self) -> &mut Self::NodeMeasure;

    /// Checks if the node is empty (i.e., contains no data or children).
    #[must_use]
    fn is_empty(&self) -> bool;

    /// Checks if the node is full (i.e., contains the maximum number of data items or children).
    #[must_use]
    fn is_full(&self) -> bool;
}

/// # Purpose
///
/// Defines the `Measure` trait, which represents a measure that can be
/// associated with nodes in a measured B-tree.
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
    /// Resets the measure to its default value.
    fn reset(&mut self);
}

pub trait Data: Clone + Debug + Eq + PartialEq {
    type Measure: Measure;

    /// Computes or gets the measure for this data item.
    fn get_measure(&self) -> Self::Measure;

    /// Splits the data item at the given measure, returning the right part as a new data item.
    /// The original data item is modified to represent the left part.
    fn split_off(&mut self, at: Self::Measure) -> Self;

    /// Try to merge `other` into `self`.
    ///
    /// # Returns
    ///
    /// `true` if the merge was successful,
    /// `false` otherwise.
    fn try_merge(&mut self, other: &Self) -> bool;
}
