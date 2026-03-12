use crate::cursor::{Cursor, Position};

pub const MAX_HISTORY: u8 = 100;

#[derive(Debug, Clone, PartialEq)]
pub enum EditAction {
    Insert {
        position: Position,
        text: Box<str>,
    },
    Delete {
        position: Position,
        end_position: Position,
        text: Box<str>,
    },
}

/// Responsible for batching multiple edit actions into a single transaction, which can be undone/redone as a unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    actions: Vec<EditAction>,
    cursor_before: Cursor,
    cursor_after: Cursor,
}

#[derive(Debug)]
pub struct History {
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
}

impl EditAction {
    pub fn new_insert(position: Position, text: Box<str>) -> Self {
        EditAction::Insert { position, text }
    }

    pub fn new_delete(position: Position, end_position: Position, text: Box<str>) -> Self {
        EditAction::Delete {
            position,
            end_position,
            text,
        }
    }
}

impl Transaction {
    pub fn new(actions: Vec<EditAction>, cursor_before: Cursor, cursor_after: Cursor) -> Self {
        Transaction {
            actions,
            cursor_before,
            cursor_after,
        }
    }

    pub fn apply(&self) {
        todo!();
    }
}

impl History {
    pub fn new() -> Self {
        History {
            undo_stack: Vec::with_capacity(MAX_HISTORY as usize),
            redo_stack: Vec::with_capacity(MAX_HISTORY as usize),
        }
    }

    pub fn undo(&mut self) -> Option<Transaction> {
        let transaction = self.undo_stack.pop()?;

        self.redo_stack.push(transaction.clone());

        Some(transaction)
    }

    pub fn redo(&mut self) -> Option<Transaction> {
        let transaction = self.redo_stack.pop()?;

        self.undo_stack.push(transaction.clone());

        Some(transaction)
    }
}
