use std::fmt::Debug;

/// # Purpose
///
/// use to visualize data structures.
pub trait Visualizer: Debug {
    fn visualize(&self) -> String;
}
