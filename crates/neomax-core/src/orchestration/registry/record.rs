use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Engine;
use crate::runs::ProbeState;

#[derive(Debug, Clone)]
pub struct OrchestratorRegistration {
    pub session: String,
    pub pid: Option<u32>,
    pub engine: Engine,
    pub account: Option<u32>,
    pub account_dir: String,
    pub project: Option<String>,
    pub branch_prefix: Option<String>,
    pub cwd: PathBuf,
    pub model: String,
    pub reserved: bool,
    pub now: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrchestratorAccount {
    Number(u32),
    Dedicated,
}

impl OrchestratorAccount {
    pub const fn number(self) -> Option<u32> {
        match self {
            Self::Number(number) => Some(number),
            Self::Dedicated => None,
        }
    }

    pub const fn is_dedicated(self) -> bool {
        matches!(self, Self::Dedicated)
    }
}

impl std::fmt::Display for OrchestratorAccount {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(number) => number.fmt(formatter),
            Self::Dedicated => formatter.write_str("orch"),
        }
    }
}

pub(super) const DEDICATED_ACCOUNT_MARKER: &str = "__neomax_orchestrator_account";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum WireAccount {
    Number(u32),
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireOrchestratorRecord {
    session: String,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default = "default_engine")]
    engine: Engine,
    #[serde(default)]
    account: Option<WireAccount>,
    #[serde(default)]
    account_dir: String,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    branch_prefix: Option<String>,
    #[serde(default)]
    cwd: PathBuf,
    #[serde(default)]
    model: String,
    #[serde(default)]
    reserved: bool,
    #[serde(default)]
    started: i64,
    #[serde(default)]
    last_seen: i64,
    #[serde(default, skip_serializing_if = "is_false")]
    live: bool,
    #[serde(skip)]
    process_state: ProbeState,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct OrchestratorRecord {
    pub session: String,
    pub pid: Option<u32>,
    pub engine: Engine,
    pub account: Option<u32>,
    pub account_dir: String,
    pub project: Option<String>,
    pub branch_prefix: Option<String>,
    pub cwd: PathBuf,
    pub model: String,
    pub reserved: bool,
    pub started: i64,
    pub last_seen: i64,
    pub live: bool,
    pub process_state: ProbeState,
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Serialize for OrchestratorRecord {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let account = self.account.map(WireAccount::Number).or_else(|| {
            self.is_dedicated_account()
                .then(|| WireAccount::Text("orch".into()))
        });
        let mut extra = self.extra.clone();
        extra.remove(DEDICATED_ACCOUNT_MARKER);
        WireOrchestratorRecord {
            session: self.session.clone(),
            pid: self.pid,
            engine: self.engine,
            account,
            account_dir: self.account_dir.clone(),
            project: self.project.clone(),
            branch_prefix: self.branch_prefix.clone(),
            cwd: self.cwd.clone(),
            model: self.model.clone(),
            reserved: self.reserved,
            started: self.started,
            last_seen: self.last_seen,
            live: self.live,
            process_state: self.process_state,
            extra,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for OrchestratorRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = WireOrchestratorRecord::deserialize(deserializer)?;
        let (account, dedicated) = match wire.account {
            None => (None, false),
            Some(WireAccount::Number(number)) => (Some(number), false),
            Some(WireAccount::Text(value)) if value.eq_ignore_ascii_case("orch") => (None, true),
            Some(WireAccount::Text(value)) => {
                return Err(DeError::custom(format!(
                    "unsupported orchestrator account selector {value:?}"
                )));
            }
        };
        let mut extra = wire.extra;
        if dedicated {
            extra.insert(
                DEDICATED_ACCOUNT_MARKER.into(),
                serde_json::Value::String("dedicated".into()),
            );
        }
        Ok(Self {
            session: wire.session,
            pid: wire.pid,
            engine: wire.engine,
            account,
            account_dir: wire.account_dir,
            project: wire.project,
            branch_prefix: wire.branch_prefix,
            cwd: wire.cwd,
            model: wire.model,
            reserved: wire.reserved,
            started: wire.started,
            last_seen: wire.last_seen,
            live: wire.live,
            process_state: wire.process_state,
            extra,
        })
    }
}

impl OrchestratorRecord {
    pub fn account_identity(&self) -> Option<OrchestratorAccount> {
        self.account.map(OrchestratorAccount::Number).or_else(|| {
            self.is_dedicated_account()
                .then_some(OrchestratorAccount::Dedicated)
        })
    }

    pub fn is_dedicated_account(&self) -> bool {
        self.extra
            .get(DEDICATED_ACCOUNT_MARKER)
            .and_then(serde_json::Value::as_str)
            == Some("dedicated")
    }

    pub(super) fn from_registration(value: OrchestratorRegistration, started: i64) -> Self {
        let dedicated = value.account.is_none() && value.reserved;
        let mut record = Self {
            session: value.session,
            pid: value.pid,
            engine: value.engine,
            account: value.account,
            account_dir: value.account_dir,
            project: value.project,
            branch_prefix: value.branch_prefix,
            cwd: value.cwd,
            model: value.model,
            reserved: value.reserved,
            started,
            last_seen: value.now,
            live: false,
            process_state: ProbeState::Unknown,
            extra: BTreeMap::new(),
        };
        if dedicated {
            record.mark_dedicated_account();
        }
        record
    }

    pub(super) fn mark_dedicated_account(&mut self) {
        self.extra.insert(
            DEDICATED_ACCOUNT_MARKER.into(),
            serde_json::Value::String("dedicated".into()),
        );
    }
}

impl OrchestratorRegistration {
    pub fn account_identity(&self) -> Option<OrchestratorAccount> {
        self.account
            .map(OrchestratorAccount::Number)
            .or_else(|| self.reserved.then_some(OrchestratorAccount::Dedicated))
    }
}

fn default_engine() -> Engine {
    Engine::Claude
}

fn is_false(value: &bool) -> bool {
    !value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_json(account: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "session": "session",
            "engine": "claude",
            "account": account,
            "account_dir": ".claude",
            "reserved": true,
            "started": 10,
            "last_seen": 20,
            "future_field": {"preserve": true}
        })
    }

    #[test]
    fn legacy_dedicated_account_is_typed_and_round_trips() {
        let record: OrchestratorRecord =
            serde_json::from_value(record_json(serde_json::json!("orch"))).unwrap();

        assert_eq!(record.account, None);
        assert_eq!(
            record.account_identity(),
            Some(OrchestratorAccount::Dedicated)
        );

        let encoded = serde_json::to_value(&record).unwrap();
        assert_eq!(encoded["account"], "orch");
        assert_eq!(encoded["future_field"]["preserve"], true);
        assert!(
            !encoded
                .as_object()
                .expect("record is an object")
                .contains_key(DEDICATED_ACCOUNT_MARKER)
        );
    }

    #[test]
    fn numeric_account_remains_compatible_with_existing_callers() {
        let record: OrchestratorRecord =
            serde_json::from_value(record_json(serde_json::json!(7))).unwrap();

        assert_eq!(record.account, Some(7));
        assert_eq!(
            record.account_identity(),
            Some(OrchestratorAccount::Number(7))
        );
        assert_eq!(serde_json::to_value(&record).unwrap()["account"], 7);
    }

    #[test]
    fn unsupported_account_selectors_are_rejected() {
        for value in [serde_json::json!("7"), serde_json::json!(false)] {
            assert!(serde_json::from_value::<OrchestratorRecord>(record_json(value)).is_err());
        }
    }
}
