use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::atomic::with_exclusive_lock;

use super::acquire::{AcquireFile, acquire_file, conflicts, remove_owned};
use super::liveness::LockLiveness;
use super::owner::LockOwner;
use super::paths::RepoLockPaths;

pub struct AreaLockManager<L> {
    locks_root: PathBuf,
    liveness: L,
    now: i64,
}

impl<L: LockLiveness> AreaLockManager<L> {
    pub fn new(locks_root: impl Into<PathBuf>, liveness: L, now: i64) -> Self {
        Self {
            locks_root: locks_root.into(),
            liveness,
            now,
        }
    }

    pub fn acquire_area_lock(&self, repo: &Path, area: &str, runid: &str) -> bool {
        self.acquire_areas(repo, [area], runid)
    }

    pub fn acquire_areas<I, S>(&self, repo: &Path, areas: I, runid: &str) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if runid.is_empty() {
            return false;
        }
        let Ok(paths) = RepoLockPaths::new(&self.locks_root, repo) else {
            return false;
        };
        let mut areas = areas
            .into_iter()
            .map(|area| {
                let area = area.as_ref().trim();
                if area.is_empty() {
                    "*".into()
                } else {
                    area.into()
                }
            })
            .collect::<BTreeSet<String>>();
        if areas.is_empty() || areas.contains("*") {
            areas = BTreeSet::from(["*".into()]);
        }
        with_exclusive_lock(&paths.transaction_lock(), || {
            Ok(self.acquire_locked(&paths, &areas, runid))
        })
        .unwrap_or(false)
    }

    fn acquire_locked(&self, paths: &RepoLockPaths, areas: &BTreeSet<String>, runid: &str) -> bool {
        let owner = LockOwner::new(runid, self.now);
        let mut created = Vec::new();
        for area in areas {
            let path = paths.path(area);
            let mut acquired = false;
            for _ in 0..2 {
                match acquire_file(&path, &owner, &self.liveness) {
                    AcquireFile::Retry => continue,
                    AcquireFile::Created => {
                        created.push(path.clone());
                        acquired = !conflicts(paths, area, runid, &self.liveness);
                        break;
                    }
                    AcquireFile::Reentrant => {
                        acquired = !conflicts(paths, area, runid, &self.liveness);
                        break;
                    }
                    AcquireFile::Busy | AcquireFile::Failed => break,
                }
            }
            if !acquired {
                for created_path in created {
                    remove_owned(&created_path, runid);
                }
                return false;
            }
        }
        true
    }

    pub fn release_area_locks<I, S>(&self, repo: &Path, areas: I, runid: &str)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let Ok(paths) = RepoLockPaths::new(&self.locks_root, repo) else {
            return;
        };
        let _ = with_exclusive_lock(&paths.transaction_lock(), || {
            for area in areas {
                let area = area.as_ref();
                let area = if area.is_empty() { "*" } else { area };
                remove_owned(&paths.path(area), runid);
            }
            Ok(())
        });
    }

    pub fn lock_path(&self, repo: &Path, area: &str) -> Option<PathBuf> {
        RepoLockPaths::new(&self.locks_root, repo)
            .ok()
            .map(|paths| paths.path(if area.is_empty() { "*" } else { area }))
    }
}
