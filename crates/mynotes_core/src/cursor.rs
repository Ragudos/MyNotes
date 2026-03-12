use std::{
    cmp::{max, min},
    mem::swap,
};

/// Represents a specific location in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    row: u64,
    /// The byte offset or character index within the line.
    col: u64,
}

/// Represents a cursor and its associated selection range.
/// Uses the "Anchor and Head" directional selection model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    /// The preferred visual column. Used to maintain horizontal position
    /// when moving vertically across shorter lines.
    preferred_column: Option<u64>,
    /// The fixed starting point of a selection.
    anchor: Position,
    /// The active, moving end of a selection (where the blinking caret is).
    head: Position,
}

impl Position {
    #[must_use]
    pub fn new(row: u64, column: u64) -> Self {
        Self { row, col: column }
    }

    #[inline]
    #[must_use]
    pub fn get_row(&self) -> u64 {
        self.row
    }

    #[inline]
    #[must_use]
    pub fn get_col(&self) -> u64 {
        self.col
    }

    #[inline]
    pub fn set_row(&mut self, row: u64) {
        self.row = row;
    }

    #[inline]
    pub fn set_col(&mut self, col: u64) {
        self.col = col;
    }
}

impl Cursor {
    #[must_use]
    pub fn new(row: u64, column: u64) -> Self {
        Self {
            anchor: Position::new(row, column),
            head: Position::new(row, column),
            preferred_column: Some(column),
        }
    }

    #[must_use]
    pub fn new_selection(anchor: Position, head: Position) -> Self {
        Self {
            anchor,
            head,
            preferred_column: Some(head.col),
        }
    }

    /// Returns true if this is just a cursor (no text selected).
    #[inline]
    #[must_use]
    pub fn get_no_selection(&self) -> bool {
        self.anchor == self.head
    }

    /// Returns the top-left most position of the selection.
    /// Crucial for `TextBuffer::delete()` which expects a normalized range.
    #[inline]
    #[must_use]
    pub fn get_start(&self) -> &Position {
        min(&self.anchor, &self.head)
    }

    #[inline]
    #[must_use]
    pub fn get_end(&self) -> &Position {
        max(&self.anchor, &self.head)
    }

    #[inline]
    #[must_use]
    pub fn range(&self) -> (&Position, &Position) {
        if self.anchor <= self.head {
            (&self.anchor, &self.head)
        } else {
            (&self.head, &self.anchor)
        }
    }

    #[inline]
    #[must_use]
    pub fn range_mut(&mut self) -> (&mut Position, &mut Position) {
        if self.anchor <= self.head {
            (&mut self.anchor, &mut self.head)
        } else {
            (&mut self.head, &mut self.anchor)
        }
    }

    #[inline]
    pub fn set_head(&mut self, pos: Position) {
        self.head = pos;
        self.preferred_column = Some(pos.col);
    }

    #[inline]
    pub fn set_anchor(&mut self, pos: Position) {
        self.anchor = pos;
    }

    pub fn clear_selection(&mut self) {
        self.anchor = self.head;
    }

    pub fn invert(&mut self) {
        swap(&mut self.anchor, &mut self.head);

        self.preferred_column = Some(self.head.col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_creation() {
        let cursor = Cursor::new(5, 10);
        assert_eq!(cursor.anchor, Position::new(5, 10));
        assert_eq!(cursor.head, Position::new(5, 10));
        assert_eq!(cursor.preferred_column, Some(10));
    }

    #[test]
    fn test_cursor_selection() {
        let anchor = Position::new(3, 5);
        let head = Position::new(6, 15);
        let cursor = Cursor::new_selection(anchor, head);

        assert_eq!(cursor.anchor, anchor);
        assert_eq!(cursor.head, head);
        assert_eq!(cursor.preferred_column, Some(15));
    }

    #[test]
    fn test_cursor_no_selection() {
        let mut cursor = Cursor::new(2, 8);

        assert!(cursor.get_no_selection());
        cursor.set_head(Position::new(2, 10));
        assert!(!cursor.get_no_selection());
        cursor.clear_selection();
        assert!(cursor.get_no_selection());
    }

    #[test]
    fn test_cursor_range() {
        let cursor = Cursor::new_selection(Position::new(4, 20), Position::new(2, 10));
        let (start, end) = cursor.range();

        assert_eq!(start, &Position::new(2, 10));
        assert_eq!(end, &Position::new(4, 20));
    }

    #[test]
    fn test_cursor_invert() {
        let mut cursor = Cursor::new_selection(Position::new(1, 5), Position::new(3, 15));

        cursor.invert();
        assert_eq!(cursor.anchor, Position::new(3, 15));
        assert_eq!(cursor.head, Position::new(1, 5));
        assert_eq!(cursor.preferred_column, Some(5));

        let (start, end) = cursor.range();

        assert_eq!(start, &Position::new(1, 5));
        assert_eq!(end, &Position::new(3, 15));
        assert_eq!(cursor.preferred_column, Some(5));
        // Inverting again should restore original state
        cursor.invert();
        assert_eq!(cursor.anchor, Position::new(1, 5));
        assert_eq!(cursor.head, Position::new(3, 15));
        assert_eq!(cursor.preferred_column, Some(15));

        let (start, end) = cursor.range();

        assert_eq!(start, &Position::new(1, 5));
        assert_eq!(end, &Position::new(3, 15));
        assert_eq!(cursor.preferred_column, Some(15));

        // Inverting a cursor with no selection should have no effect
        let mut cursor = Cursor::new(2, 8);

        cursor.invert();
        assert_eq!(cursor.anchor, Position::new(2, 8));
        assert_eq!(cursor.head, Position::new(2, 8));
        assert_eq!(cursor.preferred_column, Some(8));

        let (start, end) = cursor.range();

        assert_eq!(start, &Position::new(2, 8));
        assert_eq!(end, &Position::new(2, 8));
        assert_eq!(cursor.preferred_column, Some(8));
    }
}
