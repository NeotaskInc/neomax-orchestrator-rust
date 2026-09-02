use std::collections::BTreeSet;
use std::path::Path;

use crate::git::inspection::GitCommandRunner;
use crate::{Error, Result};

use super::state::WorktreeInspection;

pub(super) fn inspect_with_runner<R: GitCommandRunner>(
    runner: &R,
    repository: &Path,
    worktree: &Path,
    base: &str,
    branch: &str,
) -> Result<WorktreeInspection> {
    let status = runner.run(
        worktree,
        &[
            "status".into(),
            "--porcelain=v1".into(),
            "--ignored=matching".into(),
            "-z".into(),
        ],
    )?;
    if !status.success {
        return Err(Error::Message(status.stderr));
    }
    let ahead = runner.run(
        repository,
        &[
            "rev-list".into(),
            "--count".into(),
            format!("{base}..{branch}"),
        ],
    )?;
    if !ahead.success {
        return Err(Error::Message(ahead.stderr));
    }
    let commits_ahead = ahead
        .stdout
        .trim()
        .parse::<u64>()
        .map_err(|error| Error::Message(format!("invalid Git ahead count: {error}")))?;
    let mut files_touched = committed_files(runner, repository, base, branch)?;
    files_touched.extend(status_files(status.stdout.as_bytes()));
    Ok(WorktreeInspection {
        dirty: !status.stdout.is_empty(),
        commits_ahead,
        files_touched,
    })
}

fn committed_files<R: GitCommandRunner>(
    runner: &R,
    repository: &Path,
    base: &str,
    branch: &str,
) -> Result<BTreeSet<String>> {
    let output = runner.run(
        repository,
        &[
            "diff".into(),
            "--name-only".into(),
            "-z".into(),
            format!("{base}..{branch}"),
        ],
    )?;
    if !output.success {
        return Err(Error::Message(output.stderr));
    }
    Ok(nul_paths(output.stdout.as_bytes()).collect())
}

fn status_files(bytes: &[u8]) -> BTreeSet<String> {
    let entries = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut files = BTreeSet::new();
    let mut index = 0;
    while index < entries.len() {
        let entry = entries[index];
        if entry.len() < 4 {
            index += 1;
            continue;
        }
        let renamed = matches!(entry[0], b'R' | b'C') || matches!(entry[1], b'R' | b'C');
        files.insert(String::from_utf8_lossy(&entry[3..]).into_owned());
        index += if renamed { 2 } else { 1 };
    }
    files
}

fn nul_paths(bytes: &[u8]) -> impl Iterator<Item = String> + '_ {
    bytes
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_modified_added_and_renamed_porcelain_records() {
        let files = status_files(b" M changed\0?? added\0R  renamed\0old\0!! ignored\0");
        assert_eq!(
            files,
            BTreeSet::from([
                "added".into(),
                "changed".into(),
                "ignored".into(),
                "renamed".into(),
            ])
        );
    }
}
