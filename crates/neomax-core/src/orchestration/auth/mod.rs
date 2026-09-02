pub mod backup;
pub mod claude;
pub mod codex;
pub(crate) mod limits;
pub mod permissions;
pub mod policy;
pub mod restore;
pub mod rotation_log;
pub mod service;
pub mod transaction;
pub mod types;
pub mod writer;

pub use backup::{BackupDocument, BackupStore};
pub use policy::{copy_allowed, handoff_required};
pub use rotation_log::{RotationEvent, RotationEventContext, RotationLog};
pub use service::RotationService;
pub use types::{RotationEffects, RotationOperation, RotationPaths};
pub use writer::{CredentialWriter, FsCredentialWriter};

#[cfg(test)]
mod tests;
