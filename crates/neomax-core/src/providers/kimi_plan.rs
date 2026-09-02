mod config;
mod credentials;
mod platform;
mod profile_state;
mod staging;

#[cfg(test)]
#[path = "kimi_plan/tests/mod.rs"]
mod tests;

pub use staging::{PreparedHome, prepare};
