//! Undo snapshots for editor state.

/// Stack of cloned editor snapshots.
#[derive(Debug)]
pub struct UndoStack<S: Clone> {
    stack: Vec<S>,
}

impl<S: Clone> UndoStack<S> {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    pub fn push(&mut self, state: S) {
        self.stack.push(state);
    }

    pub fn pop(&mut self) -> Option<S> {
        self.stack.pop()
    }

    pub fn clear(&mut self) {
        self.stack.clear();
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }
}

impl<S: Clone> Default for UndoStack<S> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_and_clear() {
        let mut stack = UndoStack::new();
        stack.push(1);
        stack.push(2);
        assert_eq!(stack.len(), 2);
        assert_eq!(stack.pop(), Some(2));
        stack.clear();
        assert_eq!(stack.len(), 0);
    }
}
