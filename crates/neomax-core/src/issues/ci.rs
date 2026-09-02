use serde_json::Value;

pub const CI_NONRUN_CONCLUSIONS: &[&str] =
    &["STARTUP_FAILURE", "ACTION_REQUIRED", "STALE", "CANCELLED"];
pub const CI_REAL_FAIL_CONCLUSIONS: &[&str] = &["FAILURE", "TIMED_OUT"];
pub const CI_WORKFLOW_SENTINEL: &str = "Managed by Neomax `neomax ci-sync`";

pub const NEOMAX_CI_WORKFLOW: &str = "# Managed by Neomax `neomax ci-sync`\n# Edit the test command for this repo if needed; ci-sync will not clobber a hand-edited file\n# unless explicitly forced.\nname: neomax-ci\non:\n  push:\n  pull_request:\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v4\n      - uses: actions/setup-node@v4\n        with:\n          node-version: '20'\n      - name: Install\n        run: |\n          if [ -f package-lock.json ]; then npm ci; \\\n          elif [ -f package.json ]; then npm install; \\\n          else echo \"no package.json - skipping install\"; fi\n      - name: Test\n        run: |\n          if [ -f package.json ] && npm run | grep -qE '^  test'; then npm test --if-present; \\\n          else echo \"no test script - skipping\"; fi\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiConclusion {
    Success,
    Neutral,
    Skipped,
    NonRun(String),
    Failure(String),
    Pending(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiCheck {
    pub name: String,
    pub conclusion: CiConclusion,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CiClassification {
    pub real_failures: Vec<String>,
    pub nonrun: Vec<String>,
    pub pending: Vec<String>,
}

impl CiClassification {
    pub fn is_green(&self) -> bool {
        self.real_failures.is_empty() && self.pending.is_empty()
    }
}

pub fn classify_ci_checks(rollup: &[Value]) -> CiClassification {
    let mut classification = CiClassification::default();
    for value in rollup {
        let Some(check) = parse_check(value) else {
            continue;
        };
        match check.conclusion {
            CiConclusion::Success | CiConclusion::Neutral | CiConclusion::Skipped => {}
            CiConclusion::NonRun(_) => classification.nonrun.push(check.name),
            CiConclusion::Failure(_) => classification.real_failures.push(check.name),
            CiConclusion::Pending(_) => classification.pending.push(check.name),
        }
    }
    classification
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeState {
    Merged,
    Dirty,
    Behind,
    Blocked,
    Clean,
    Unknown(String),
}

impl MergeState {
    pub fn from_value(value: Option<&str>) -> Self {
        match value.unwrap_or_default().to_ascii_uppercase().as_str() {
            "MERGED" => Self::Merged,
            "DIRTY" => Self::Dirty,
            "BEHIND" => Self::Behind,
            "BLOCKED" => Self::Blocked,
            "CLEAN" => Self::Clean,
            value => Self::Unknown(value.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeInput<'a> {
    pub state: Option<&'a str>,
    pub merge_state: MergeState,
    pub url: Option<&'a str>,
    pub rollup: &'a [Value],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeGate {
    Merged,
    Stopped(String),
    Blocked(String),
    Waiting(String),
    Ready { ignored_nonrun: Vec<String> },
}

pub fn evaluate_merge_gate(input: &MergeInput<'_>, ignore_nonrun: bool) -> MergeGate {
    if input
        .state
        .is_some_and(|state| state.eq_ignore_ascii_case("MERGED"))
        || matches!(input.merge_state, MergeState::Merged)
    {
        return MergeGate::Merged;
    }
    if matches!(input.merge_state, MergeState::Dirty | MergeState::Behind) {
        return MergeGate::Blocked("PR needs rebase onto the base branch".into());
    }
    let mut checks = classify_ci_checks(input.rollup);
    if !ignore_nonrun && !checks.nonrun.is_empty() {
        checks.real_failures.extend(checks.nonrun.iter().cloned());
    }
    if !checks.real_failures.is_empty() {
        return MergeGate::Blocked(format!(
            "failing CI checks: {}",
            checks.real_failures.join(", ")
        ));
    }
    if !checks.pending.is_empty() {
        return MergeGate::Waiting(format!(
            "CI checks still running: {}",
            checks.pending.join(", ")
        ));
    }
    if input.rollup.is_empty() && matches!(input.merge_state, MergeState::Unknown(_)) {
        return MergeGate::Waiting("CI not reported yet".into());
    }
    if matches!(input.merge_state, MergeState::Blocked) {
        return MergeGate::Blocked("PR merge state is blocked".into());
    }
    MergeGate::Ready {
        ignored_nonrun: if ignore_nonrun {
            checks.nonrun
        } else {
            Vec::new()
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiSyncAction {
    Unchanged,
    Create,
    Update,
    SkipHandEdited,
}

pub fn ci_sync_action(existing: Option<&str>, force: bool) -> CiSyncAction {
    match existing {
        Some(value) if value == NEOMAX_CI_WORKFLOW => CiSyncAction::Unchanged,
        Some(value) if !force && !value.contains(CI_WORKFLOW_SENTINEL) => {
            CiSyncAction::SkipHandEdited
        }
        Some(_) => CiSyncAction::Update,
        None => CiSyncAction::Create,
    }
}

fn parse_check(value: &Value) -> Option<CiCheck> {
    let object = value.as_object()?;
    let name = object
        .get("name")
        .or_else(|| object.get("context"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("check")
        .to_string();
    let conclusion = object
        .get("conclusion")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    let state = object
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    let conclusion =
        if matches!(conclusion.as_str(), "SUCCESS" | "NEUTRAL" | "SKIPPED") || state == "SUCCESS" {
            match conclusion.as_str() {
                "SUCCESS" => CiConclusion::Success,
                "NEUTRAL" => CiConclusion::Neutral,
                "SKIPPED" => CiConclusion::Skipped,
                _ => CiConclusion::Success,
            }
        } else if CI_NONRUN_CONCLUSIONS.contains(&conclusion.as_str()) {
            CiConclusion::NonRun(conclusion)
        } else if CI_REAL_FAIL_CONCLUSIONS.contains(&conclusion.as_str())
            || matches!(state.as_str(), "FAILURE" | "ERROR")
        {
            CiConclusion::Failure(if conclusion.is_empty() {
                state
            } else {
                conclusion
            })
        } else {
            CiConclusion::Pending(if conclusion.is_empty() {
                state
            } else {
                conclusion
            })
        };
    Some(CiCheck { name, conclusion })
}
