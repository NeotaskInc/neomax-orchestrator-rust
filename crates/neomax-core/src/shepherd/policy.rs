#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergePolicy {
    pub ignore_non_running_ci: bool,
}

impl Default for MergePolicy {
    fn default() -> Self {
        Self {
            ignore_non_running_ci: true,
        }
    }
}

impl MergePolicy {
    pub fn from_billing_environment(value: Option<&str>) -> Self {
        Self {
            ignore_non_running_ci: billing_ignore_enabled(value),
        }
    }
}

pub fn billing_ignore_enabled(value: Option<&str>) -> bool {
    value.unwrap_or("1") != "0"
}
