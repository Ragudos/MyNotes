use crate::{
    data_structures::object_pool::Poolable,
    types::{BTREE_CHILD_CAPACITY, BTREE_DATA_CAPACITY, ObjectPoolIndex},
};

use super::traits::{Data, Measure, Node};

/// A leaf node in the measured B-tree, which contains a vector
/// of data items and a measure that summarizes the data in the node.
#[derive(Debug, Clone)]
pub struct Leaf<T>
where
    T: Data,
{
    data_vec: Vec<T>,
    measure: T::Measure,
}

/// An internal node in the measured B-tree, which contains a vector
/// of child node indices and a measure that summarizes the data in the node.
#[derive(Debug, Clone)]

pub struct Internal<T>
where
    T: Data,
{
    children: Vec<ObjectPoolIndex>,
    measure: T::Measure,
}

impl<T> Poolable for Leaf<T>
where
    T: Data,
{
    fn reset(&mut self) {
        self.data_vec.clear();
        self.measure.reset();
    }

    fn new() -> Self {
        Self {
            data_vec: Vec::with_capacity(BTREE_DATA_CAPACITY as usize),
            measure: T::Measure::default(),
        }
    }
}

impl<T> Node for Leaf<T>
where
    T: Data,
{
    type NodeMeasure = T::Measure;

    #[inline]
    fn get_measure(&self) -> Self::NodeMeasure {
        self.measure
    }

    #[inline]
    fn get_mut_measure(&mut self) -> &mut Self::NodeMeasure {
        &mut self.measure
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.data_vec.is_empty()
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.data_vec.len() >= BTREE_DATA_CAPACITY as usize
    }
}

impl<T> Poolable for Internal<T>
where
    T: Data,
{
    fn reset(&mut self) {
        self.children.clear();
        self.measure.reset();
    }

    fn new() -> Self {
        Self {
            children: Vec::with_capacity(BTREE_CHILD_CAPACITY as usize),
            measure: T::Measure::default(),
        }
    }
}

impl<T> Node for Internal<T>
where
    T: Data,
{
    type NodeMeasure = T::Measure;

    #[inline]
    fn get_measure(&self) -> Self::NodeMeasure {
        self.measure
    }

    #[inline]
    fn get_mut_measure(&mut self) -> &mut Self::NodeMeasure {
        &mut self.measure
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.children.len() >= BTREE_CHILD_CAPACITY as usize
    }
}
