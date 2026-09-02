mod acquire;
mod manager;
mod owner;
mod paths;

pub mod liveness;

pub use liveness::{FALLBACK_TTL_SECONDS, FallbackTtlLiveness, LockLiveness, RunStoreLiveness};
pub use manager::AreaLockManager;
pub use owner::LockOwner;

#[cfg(test)]
mod tests;
