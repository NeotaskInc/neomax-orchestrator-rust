mod builder;
mod quota;

pub use builder::AccountInventory;
pub use quota::{QuotaRotationAdvice, QuotaTarget, QuotaWindow, quota_advice};

#[cfg(test)]
mod tests;
