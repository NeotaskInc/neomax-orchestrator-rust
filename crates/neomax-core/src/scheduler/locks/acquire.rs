use std::fs;
use std::io;
use std::path::Path;

use super::liveness::LockLiveness;
use super::owner::{LockOwner, OwnerSnapshot, create_owner, read_owner, with_exclusive_owner};
use super::paths::RepoLockPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AcquireFile {
    Created,
    Reentrant,
    Retry,
    Busy,
    Failed,
}

pub(super) fn acquire_file<L: LockLiveness>(
    path: &Path,
    owner: &LockOwner,
    liveness: &L,
) -> AcquireFile {
    match create_owner(path, owner) {
        Ok(_) => AcquireFile::Created,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => match read_owner(path) {
            OwnerSnapshot::Missing => AcquireFile::Retry,
            OwnerSnapshot::Valid(existing) if existing.runid == owner.runid => {
                AcquireFile::Reentrant
            }
            OwnerSnapshot::Valid(existing) if liveness.is_stale(Some(&existing)) => {
                reclaim(path, liveness).unwrap_or(AcquireFile::Busy)
            }
            OwnerSnapshot::Malformed if liveness.is_stale(None) => {
                reclaim(path, liveness).unwrap_or(AcquireFile::Busy)
            }
            OwnerSnapshot::Valid(_) | OwnerSnapshot::Malformed | OwnerSnapshot::Unavailable => {
                AcquireFile::Busy
            }
        },
        Err(_) => AcquireFile::Failed,
    }
}

pub(super) fn conflicts<L: LockLiveness>(
    paths: &RepoLockPaths,
    area: &str,
    runid: &str,
    liveness: &L,
) -> bool {
    let target = paths.path(area);
    let candidates = if area == "*" {
        match paths.all() {
            Ok(paths) => paths,
            Err(_) => return true,
        }
    } else {
        vec![paths.path("*")]
    };
    candidates.into_iter().any(|path| {
        if path == target {
            return false;
        }
        match read_owner(&path) {
            OwnerSnapshot::Missing => false,
            OwnerSnapshot::Valid(owner) if owner.runid == runid => false,
            OwnerSnapshot::Valid(owner) => !liveness.is_stale(Some(&owner)),
            OwnerSnapshot::Malformed => !liveness.is_stale(None),
            OwnerSnapshot::Unavailable => true,
        }
    })
}

pub(super) fn remove_owned(path: &Path, runid: &str) {
    let _ = with_exclusive_owner(path, |_, snapshot| match snapshot {
        OwnerSnapshot::Valid(owner) if owner.runid == runid => fs::remove_file(path),
        _ => Ok(()),
    });
}

fn reclaim<L: LockLiveness>(path: &Path, liveness: &L) -> Option<AcquireFile> {
    let result = with_exclusive_owner(path, |_, snapshot| {
        let stale = match &snapshot {
            OwnerSnapshot::Missing => return Ok(false),
            OwnerSnapshot::Valid(owner) => liveness.is_stale(Some(owner)),
            OwnerSnapshot::Malformed => liveness.is_stale(None),
            OwnerSnapshot::Unavailable => false,
        };
        if stale {
            fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    });
    match result {
        Ok(true) => Some(AcquireFile::Retry),
        Ok(false) => Some(AcquireFile::Busy),
        Err(_) => None,
    }
}
