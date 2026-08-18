//! Multi-process worker coordination: session leases, registry, mailbox, file claims.

#[cfg(feature = "backend-turso")]
mod file_lease;
#[cfg(feature = "backend-turso")]
mod lease;
#[cfg(feature = "backend-turso")]
mod mailbox;
mod path_claim;
mod pid;
#[cfg(feature = "backend-turso")]
mod registry;
#[cfg(feature = "backend-turso")]
mod tools;
mod types;

pub use pid::pid_alive;

#[cfg(feature = "backend-turso")]
pub use file_lease::FileLeaseStore;
#[cfg(feature = "backend-turso")]
pub use lease::{LeaseConflict, LeaseError, SessionLease, SessionLeaseStore};
#[cfg(feature = "backend-turso")]
pub use mailbox::MailboxStore;
pub use path_claim::{PathClaimContext, SharedPathClaim, content_hash, file_content_fingerprint, normalize_claim_path};
#[cfg(feature = "backend-turso")]
pub use registry::WorkerRegistry;
#[cfg(feature = "backend-turso")]
pub use tools::{WorkerToolContext, create_intercom_tools, create_worker_tools};
pub use types::{FileLease, LiveWorker, MessageKind, MessageStatus, WorkerMessage, WorkerRecord, WorkerStatus};
