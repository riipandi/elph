//! Session-scoped structured work list (ReAct planning).

#[cfg(feature = "backend-turso")]
mod auto_close;
#[cfg(feature = "backend-turso")]
mod store;
#[cfg(feature = "backend-turso")]
mod tools;
mod tracker;
mod types;

#[cfg(feature = "backend-turso")]
pub use auto_close::auto_close_done_todos;
#[cfg(feature = "backend-turso")]
pub use store::{TodoStore, TodoUpdate};
#[cfg(feature = "backend-turso")]
pub use tools::{TodoHook, WorkTrackerHandle, create_todo_tools, create_todo_tools_with_hook};
pub use tracker::WorkTracker;
pub use types::{TodoItem, TodoStatus};
