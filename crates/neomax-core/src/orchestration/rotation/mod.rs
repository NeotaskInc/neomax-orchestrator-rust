mod armed;
mod cooldown;
mod policy;
mod selectors;
mod types;

pub use armed::{ARMED_ROTATE_AGE_SECONDS, ArmedRotateClaim, ArmedRotateRecord, ArmedRotateStore};
pub use cooldown::AccountCooldownStore;
pub use policy::{engine_has_five_hour, rotation_advice, rotation_window};
pub use selectors::{
    AccountSelector, normalize_profile_path, parse_account_selector, parse_account_selectors,
};
pub use types::{RotationAdvice, RotationWindow};

#[cfg(test)]
#[path = "tests/armed.rs"]
mod armed_tests;
#[cfg(test)]
#[path = "tests/cooldown.rs"]
mod cooldown_tests;
#[cfg(test)]
#[path = "tests/policy.rs"]
mod policy_tests;
#[cfg(test)]
#[path = "tests/selectors.rs"]
mod selectors_tests;
