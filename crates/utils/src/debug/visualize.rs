use std::fmt::Debug;

/// # Purpose
///
/// Used to return a string representation of a data structure
/// for debugging purposes or for display in a
/// diagram viewer or similar tool.
pub trait Visualizer: Debug {
    fn visualize(&self) -> String;
}
