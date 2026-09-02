use std::fs;
use std::path::{Path, PathBuf};

use super::super::discovery::{ProjectContext, RepositorySpec};
use super::super::git::{GitRunner, ProcessGit, args};

pub fn repository(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(&path).unwrap();
    let git = ProcessGit;
    run(&git, &path, &["init", "-q"]);
    run(&git, &path, &["config", "user.name", "Neomax Test"]);
    run(
        &git,
        &path,
        &["config", "user.email", "test@example.invalid"],
    );
    run(&git, &path, &["config", "commit.gpgSign", "true"]);
    run(&git, &path, &["config", "tag.gpgSign", "true"]);
    #[cfg(unix)]
    {
        let hook = path.join(".git/hooks/pre-commit");
        fs::write(
            &hook,
            "#!/bin/sh\nprintf hooked > \"$GIT_DIR/neomax-hook-fired\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(path.join("README.md"), format!("{name}\n")).unwrap();
    run(&git, &path, &["add", "README.md"]);
    run(&git, &path, &["commit", "-qm", "initial"]);
    assert!(
        !path.join(".git/neomax-hook-fired").exists(),
        "fixture hook executed despite the fail-closed hook path"
    );
    run(&git, &path, &["branch", "-M", "main"]);
    path
}

pub fn run<G: GitRunner>(git: &G, cwd: &Path, values: &[&str]) {
    let mut git_args = args([
        "-c",
        "core.hooksPath=__neomax_no_test_hooks__",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "core.fsmonitor=false",
    ]);
    git_args.extend(values.iter().map(|value| (*value).to_owned()));
    let output = git.run(cwd, &git_args).unwrap();
    assert!(
        output.success,
        "git {:?} failed: {}",
        values,
        output.stderr_text()
    );
}

pub fn context(root: &Path, repositories: &[(&str, PathBuf)]) -> ProjectContext {
    ProjectContext {
        name: "sample".into(),
        root: root.to_path_buf(),
        branch_prefix: "samp".into(),
        worktree_root: root.join(".neomax-worktrees"),
        repositories: repositories
            .iter()
            .map(|(relative, path)| RepositorySpec {
                relative: PathBuf::from(relative),
                root: path.clone(),
                label: if *relative == "." {
                    root.file_name().unwrap().to_string_lossy().into_owned()
                } else {
                    relative.replace('/', "-")
                },
            })
            .collect(),
    }
}
