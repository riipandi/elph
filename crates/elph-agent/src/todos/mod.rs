//! Session-scoped structured work list (ReAct planning).

mod auto_close;
mod store;
mod tools;
mod tracker;
mod types;

pub use auto_close::auto_close_done_todos;
pub use store::{TodoStore, TodoUpdate};
pub use tools::{TodoHook, WorkTrackerHandle, create_todo_tools, create_todo_tools_with_hook};
pub use tracker::WorkTracker;
pub use types::{TodoItem, TodoStatus};
