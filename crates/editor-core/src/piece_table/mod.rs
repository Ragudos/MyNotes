/// # Piece Table Module.
///
/// Contains mod files for handling `piece_table` data.
/// Uses a Piece Table to group lines of `spiece_table` separated
/// by a newline '\n' character.
pub mod piece;
pub mod table;

/// 1 KB of initialized buffer vector for piece table's
/// text buffer (to be added)
pub const BASELINE_CAPACITY: usize = 1024;
