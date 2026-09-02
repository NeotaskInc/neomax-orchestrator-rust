use std::path::{Path, PathBuf};

use crate::atomic::{
    read_json_or_default, read_json_or_default_on_missing, update_json_locked_strict,
};
use crate::{Error, Result};

use super::{Task, TaskPatch, TaskRegistry, TaskStatus};

pub struct TaskStore {
    path: PathBuf,
}

impl TaskStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> TaskRegistry {
        read_json_or_default(&self.path)
    }

    pub fn try_load(&self) -> Result<TaskRegistry> {
        read_json_or_default_on_missing(&self.path)
    }

    pub fn add(
        &self,
        title: &str,
        project: Option<String>,
        status: TaskStatus,
        note: Option<String>,
        now: i64,
    ) -> Result<Task> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::InvalidArgument("task title is empty".into()));
        }
        let mut created = None;
        update_json_locked_strict::<TaskRegistry, _>(
            &self.path,
            &lock_path(&self.path),
            |state| {
                let id = loop {
                    state.seq = state
                        .seq
                        .checked_add(1)
                        .ok_or_else(|| Error::Conflict("task sequence is exhausted".into()))?;
                    let id = format!("t{}", state.seq);
                    if !state.tasks.contains_key(&id) && !state.invalid_tasks.contains_key(&id) {
                        break id;
                    }
                };
                let task = Task {
                    id: id.clone(),
                    title: title.into(),
                    project,
                    status,
                    created: now,
                    updated: now,
                    notes: note.into_iter().collect(),
                    runs: Vec::new(),
                    data: Default::default(),
                };
                state.tasks.insert(id, task.clone());
                created = Some(task);
                Ok(())
            },
        )?;
        created.ok_or_else(|| Error::Message("task was not created".into()))
    }

    pub fn update(&self, id: &str, patch: TaskPatch, now: i64) -> Result<Option<Task>> {
        let mut updated = None;
        update_json_locked_strict::<TaskRegistry, _>(
            &self.path,
            &lock_path(&self.path),
            |state| {
                let Some(task) = state.tasks.get_mut(id) else {
                    return Ok(());
                };
                if let Some(status) = patch.status {
                    task.status = status;
                }
                if let Some(title) = patch
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    task.title = title.into();
                }
                if let Some(project) = patch.project.clone() {
                    task.project = project.filter(|value| !value.is_empty());
                }
                if let Some(note) = patch
                    .note
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    task.notes.push(note.into());
                }
                if let Some(run_id) = patch.run_id.as_ref().filter(|id| !id.is_empty()) {
                    if !task.runs.contains(run_id) {
                        task.runs.push(run_id.clone());
                    }
                }
                task.updated = now;
                updated = Some(task.clone());
                Ok(())
            },
        )?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> Result<Option<Task>> {
        let mut removed = None;
        update_json_locked_strict::<TaskRegistry, _>(
            &self.path,
            &lock_path(&self.path),
            |state| {
                removed = state.tasks.remove(id);
                Ok(())
            },
        )?;
        Ok(removed)
    }

    pub fn list(&self, project: Option<&str>, include_done: bool) -> Vec<Task> {
        let mut tasks = self
            .load()
            .tasks
            .into_values()
            .filter(|task| project.is_none_or(|name| task.project.as_deref() == Some(name)))
            .filter(|task| include_done || !task.status.is_done())
            .collect::<Vec<_>>();
        tasks.sort_by(|left, right| {
            right
                .updated
                .cmp(&left.updated)
                .then_with(|| right.id.cmp(&left.id))
        });
        tasks
    }
}

fn lock_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.to_string_lossy()))
}
