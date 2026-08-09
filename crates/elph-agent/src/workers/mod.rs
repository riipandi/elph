//! Multi-process worker coordination: session leases, registry, mailbox, file claims.

mod file_lease;
mod lease;
mod mailbox;
mod path_claim;
mod registry;
mod tools;
mod types;

pub use file_lease::FileLeaseStore;
pub use lease::{LeaseConflict, LeaseError, SessionLease, SessionLeaseStore};
pub use mailbox::MailboxStore;
pub use path_claim::{PathClaimContext, SharedPathClaim, normalize_claim_path};
pub use registry::WorkerRegistry;
pub use tools::{WorkerToolContext, create_worker_tools};
pub use types::{
    FileLease, LiveWorker, MessageKind, MessageStatus, WorkerMessage, WorkerRecord, WorkerStatus,
};
