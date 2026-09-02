use anyhow::{Result, bail};
use neomax_core::sessions::SessionRecord;

#[derive(Debug, Clone, Default)]
pub(crate) struct SessionFilters {
    pub project: Option<String>,
    pub engine: Option<neomax_core::Engine>,
    pub active: bool,
    pub terminal: bool,
}

impl SessionFilters {
    pub(crate) fn matches(&self, record: &SessionRecord) -> bool {
        if self.engine.is_some_and(|engine| record.engine != engine) {
            return false;
        }
        if self
            .project
            .as_deref()
            .is_some_and(|project| record.project.as_deref() != Some(project))
        {
            return false;
        }
        if self.active && !(record.active || record.working) {
            return false;
        }
        if self.terminal && !is_terminal(record) {
            return false;
        }
        true
    }
}

pub(crate) fn parse_engine(value: &str) -> Result<neomax_core::Engine> {
    value
        .parse()
        .map_err(|_| anyhow::anyhow!("unknown provider {value}"))
}

pub(crate) fn validate(filters: &SessionFilters) -> Result<()> {
    if filters.active && filters.terminal {
        bail!("--active and --terminal cannot be used together")
    }
    Ok(())
}

pub(crate) fn is_terminal(record: &SessionRecord) -> bool {
    record.done || record.archived
}

#[cfg(test)]
mod tests {
    use super::*;
    use neomax_core::Engine;
    use neomax_core::sessions::SessionRecord;

    fn record(id: &str, active: bool, working: bool, done: bool) -> SessionRecord {
        SessionRecord {
            id: id.into(),
            engine: Engine::Kimi,
            project: Some("project".into()),
            active,
            working,
            done,
            ..SessionRecord::default()
        }
    }

    #[test]
    fn active_filter_includes_working_records() {
        let filters = SessionFilters {
            active: true,
            ..SessionFilters::default()
        };
        assert!(filters.matches(&record("working", false, true, false)));
        assert!(!filters.matches(&record("idle", false, false, false)));
    }

    #[test]
    fn terminal_filter_is_explicit_and_project_scoped() {
        let filters = SessionFilters {
            terminal: true,
            project: Some("project".into()),
            engine: Some(Engine::Kimi),
            ..SessionFilters::default()
        };
        assert!(filters.matches(&record("done", false, false, true)));
        assert!(!filters.matches(&record("other", false, false, false)));
        let mut other = record("other-project", false, false, true);
        other.project = Some("other".into());
        assert!(!filters.matches(&other));
    }

    #[test]
    fn active_and_terminal_are_mutually_exclusive() {
        assert!(
            validate(&SessionFilters {
                active: true,
                terminal: true,
                ..SessionFilters::default()
            })
            .is_err()
        );
    }
}
