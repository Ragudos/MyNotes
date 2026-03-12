//! Module to provide utilities for visualizing data structures in a human-readable format.
//! This module includes traits and functions that can be used to create visual representations of data structures,
use std::fmt::Debug;

/// # Purpose
///
/// Used to return a string representation of a data structure
/// for debugging purposes or for display in a
/// diagram viewer or similar tool.
pub trait Visualizer: Debug {
    /// Returns a string representation of the data structure for visualization purposes.
    fn visualize(&self) -> String;
}
