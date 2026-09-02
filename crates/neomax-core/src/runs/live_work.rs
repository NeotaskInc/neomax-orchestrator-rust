mod process;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::accounts::{LiveWorkSnapshot, LiveWorkSource};
use crate::{Engine, Result};

use super::{ProcessProbe, RunStatus, RunStore, worker_alive};

pub(crate) use process::{ClaudeProcessSource, SystemClaudeProcessSource};

/// Adapts durable run state and optional ambient process state to account policy.
pub struct RunLiveWorkSource<'a> {
    pub runs: &'a RunStore,
    pub probe: &'a dyn ProcessProbe,
}

pub struct AmbientRunLiveWorkSource<'a> {
    runs: &'a RunStore,
    probe: &'a dyn ProcessProbe,
    ambient: Box<dyn ClaudeProcessSource + 'a>,
}

impl<'a> RunLiveWorkSource<'a> {
    pub fn new(runs: &'a RunStore, probe: &'a dyn ProcessProbe) -> Self {
        Self { runs, probe }
    }

    pub fn with_system(
        runs: &'a RunStore,
        probe: &'a dyn ProcessProbe,
    ) -> AmbientRunLiveWorkSource<'a> {
        AmbientRunLiveWorkSource {
            runs,
            probe,
            ambient: Box::new(SystemClaudeProcessSource::new()),
        }
    }
}

impl LiveWorkSource for RunLiveWorkSource<'_> {
    fn live_work(&self) -> Result<LiveWorkSnapshot> {
        Ok(LiveWorkSnapshot {
            counts: live_counts_with_source(self.runs, self.probe, None)?,
        })
    }
}

impl LiveWorkSource for AmbientRunLiveWorkSource<'_> {
    fn live_work(&self) -> Result<LiveWorkSnapshot> {
        Ok(LiveWorkSnapshot {
            counts: live_counts_with_source(self.runs, self.probe, Some(self.ambient.as_ref()))?,
        })
    }
}

fn live_counts_with_source(
    runs: &RunStore,
    probe: &dyn ProcessProbe,
    ambient: Option<&dyn ClaudeProcessSource>,
) -> Result<BTreeMap<(Engine, PathBuf), u32>> {
    let mut counts = BTreeMap::new();
    let mut registered_claude_pids = BTreeSet::new();
    for run in runs.all()? {
        if run.status != RunStatus::Running || !worker_alive(&run, probe) {
            continue;
        }
        *counts.entry((run.engine, run.profile.clone())).or_default() += 1;
        if run.engine == Engine::Claude {
            if let Some(pid) = run.worker_pid {
                registered_claude_pids.insert(pid);
            }
        }
    }
    if let Some(source) = ambient {
        process::add_ambient_counts(&mut counts, &registered_claude_pids, &source.processes()?);
    }
    Ok(counts)
}

#[cfg(test)]
#[path = "live_work/tests.rs"]
mod tests;
