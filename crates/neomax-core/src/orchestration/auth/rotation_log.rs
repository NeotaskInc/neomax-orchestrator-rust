use std::path::{Path, PathBuf};

use serde::de::Error as _;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::atomic::append_lines_locked;
use crate::io::{read_file, BoundedIoError, LocalFileSource};
use crate::{Engine, Result};

use super::limits::rotation_log_read_limits;
use super::types::profile_name;
use super::writer::lock_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationEvent {
    pub ts: i64,
    pub engine: Engine,
    pub operation: String,
    pub destination: String,
    pub source: Option<String>,
    pub from_email: Option<String>,
    pub to_email: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RotationEventWire {
    ts: i64,
    engine: Engine,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    destination: Option<String>,
    #[serde(default)]
    dest: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    src: Option<String>,
    #[serde(default)]
    from_email: Option<String>,
    #[serde(default)]
    to_email: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

impl Serialize for RotationEvent {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut record = serializer.serialize_struct("RotationEvent", 10)?;
        record.serialize_field("ts", &self.ts)?;
        record.serialize_field("engine", &self.engine)?;
        record.serialize_field("operation", &self.operation)?;
        record.serialize_field("destination", &self.destination)?;
        record.serialize_field("dest", &self.destination)?;
        record.serialize_field("source", &self.source)?;
        record.serialize_field("src", &self.source)?;
        record.serialize_field("from_email", &self.from_email)?;
        record.serialize_field("to_email", &self.to_email)?;
        record.serialize_field("reason", &self.reason)?;
        record.end()
    }
}

impl<'de> Deserialize<'de> for RotationEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RotationEventWire::deserialize(deserializer)?;
        let destination = compatible_field(wire.destination, wire.dest, "destination")?
            .ok_or_else(|| D::Error::missing_field("destination or dest"))?;
        if destination.is_empty() {
            return Err(D::Error::custom("rotation event destination is empty"));
        }
        let source = compatible_field(wire.source, wire.src, "source")?;
        Ok(Self {
            ts: wire.ts,
            engine: wire.engine,
            operation: wire.operation.unwrap_or_else(|| "legacy".into()),
            destination,
            source,
            from_email: wire.from_email,
            to_email: wire.to_email,
            reason: wire.reason,
        })
    }
}

fn compatible_field<E: serde::de::Error>(
    canonical: Option<String>,
    legacy: Option<String>,
    field: &str,
) -> std::result::Result<Option<String>, E> {
    match (canonical, legacy) {
        (Some(canonical), Some(legacy)) if canonical != legacy => Err(E::custom(format!(
            "rotation event has conflicting {field} and compatibility fields"
        ))),
        (Some(value), _) | (None, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

pub struct RotationEventContext<'a> {
    pub ts: i64,
    pub engine: Engine,
    pub operation: &'a str,
    pub destination: &'a Path,
    pub source: Option<&'a Path>,
    pub from_email: Option<String>,
    pub to_email: Option<String>,
    pub reason: Option<String>,
}

impl RotationEvent {
    pub fn from_context(context: RotationEventContext<'_>) -> Self {
        Self {
            ts: context.ts,
            engine: context.engine,
            operation: context.operation.into(),
            destination: profile_name(context.destination),
            source: context.source.map(profile_name),
            from_email: context.from_email,
            to_email: context.to_email,
            reason: context.reason,
        }
    }
}

pub struct RotationLog {
    path: PathBuf,
    lock: PathBuf,
}

impl RotationLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let lock = lock_path(&path);
        Self { path, lock }
    }

    pub fn append(&self, event: &RotationEvent) -> Result<()> {
        let mut line = serde_json::to_vec(event)?;
        line.retain(|byte| *byte != b'\n' && *byte != b'\r');
        append_lines_locked(&self.path, &self.lock, &[line])
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<RotationEvent>> {
        let bytes = match read_file(&LocalFileSource, &self.path, rotation_log_read_limits()) {
            Ok(bytes) => bytes,
            Err(BoundedIoError::NotFound { .. }) => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut events = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .filter_map(|line| serde_json::from_slice::<RotationEvent>(line).ok())
            .collect::<Vec<_>>();
        if events.len() > limit {
            events.drain(..events.len() - limit);
        }
        Ok(events)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
