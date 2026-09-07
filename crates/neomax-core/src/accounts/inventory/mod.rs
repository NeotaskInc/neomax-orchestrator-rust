mod builder;
mod quota;

pub use builder::AccountInventory;
pub use quota::{QuotaRotationAdvice, QuotaTarget, QuotaWindow, quota_advice, quota_advice_for_model};

#[cfg(test)]
mod tests;
