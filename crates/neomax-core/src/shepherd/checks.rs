use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CiClassification {
    pub real_failures: Vec<String>,
    pub non_running: Vec<String>,
    pub pending: Vec<String>,
}

impl From<crate::issues::CiClassification> for CiClassification {
    fn from(value: crate::issues::CiClassification) -> Self {
        Self {
            real_failures: value.real_failures,
            non_running: value.nonrun,
            pending: value.pending,
        }
    }
}

impl CiClassification {
    pub fn is_empty(&self) -> bool {
        self.real_failures.is_empty() && self.non_running.is_empty() && self.pending.is_empty()
    }

    pub fn has_blocking_failure(&self, ignore_non_running: bool) -> bool {
        !self.real_failures.is_empty() || (!ignore_non_running && !self.non_running.is_empty())
    }

    pub fn blocking_failures(&self, ignore_non_running: bool) -> Vec<String> {
        let mut names = self.real_failures.clone();
        if !ignore_non_running {
            names.extend(self.non_running.iter().cloned());
        }
        unique_sorted(names)
    }

    pub fn ignored_non_running(&self) -> Vec<String> {
        unique_sorted(self.non_running.clone())
    }

    pub fn pending_names(&self) -> Vec<String> {
        unique_sorted(self.pending.clone())
    }
}

pub fn classify_ci_checks(rollup: &[Value]) -> CiClassification {
    crate::issues::classify_ci_checks(rollup).into()
}

fn unique_sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort_unstable();
    names.dedup();
    names
}
