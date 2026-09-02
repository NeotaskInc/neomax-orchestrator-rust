use std::fs;
use std::path::Path;

use super::*;
use crate::git::{invoke, output};

#[test]
fn unions_two_way_conflicts_without_losing_unique_lines() {
    let text = "before\n<<<<<<< HEAD\na\nshared\n=======\nb\nshared\n>>>>>>> part\nafter\n";
    assert_eq!(
        union_resolve(text).unwrap(),
        "before\na\nshared\nb\nafter\n"
    );
    assert!(union_resolve("<<<<<<<\na\n||||||| base\nb\n=======\nc\n>>>>>>>\n").is_none());
}

#[test]
fn union_policy_is_limited_to_append_only_paths() {
    assert!(is_union_safe(Path::new(".gitignore")));
    assert!(is_union_safe(Path::new("docs/CHANGELOG.md")));
    assert!(is_union_safe(Path::new(".neomax/events.json")));
    assert!(is_union_safe(Path::new("logs/run.ndjson")));
    assert!(!is_union_safe(Path::new("src/main.rs")));
}

#[test]
fn integrates_clean_part_branches_in_a_dedicated_worktree() {
    let fixture = RepositoryFixture::new();
    fixture.commit_on("part", "part.txt", "part\n");
    let outcome = GitPartIntegrator
        .integrate(
            &fixture.repository,
            &fixture.integration,
            "integration",
            "part",
            "p1",
        )
        .unwrap();
    assert_eq!(outcome, IntegrationOutcome::Merged);
    assert_eq!(
        output(&fixture.integration, ["show", "HEAD:part.txt"]).unwrap(),
        "part"
    );
}

#[test]
fn self_heals_only_whitelisted_conflicts_and_aborts_code_conflicts() {
    let safe = RepositoryFixture::with_base_file(".gitignore", "base\n");
    safe.commit_on("integration", ".gitignore", "ours\n");
    safe.commit_on("part", ".gitignore", "theirs\n");
    let outcome = GitPartIntegrator
        .integrate(
            &safe.repository,
            &safe.integration,
            "integration",
            "part",
            "safe",
        )
        .unwrap();
    assert!(matches!(outcome, IntegrationOutcome::SelfHealed { .. }));
    let merged = fs::read_to_string(safe.integration.join(".gitignore")).unwrap();
    assert!(merged.contains("ours"));
    assert!(merged.contains("theirs"));

    let unsafe_repo = RepositoryFixture::with_base_file("src/main.rs", "base\n");
    unsafe_repo.commit_on("integration", "src/main.rs", "ours\n");
    unsafe_repo.commit_on("part", "src/main.rs", "theirs\n");
    let outcome = GitPartIntegrator
        .integrate(
            &unsafe_repo.repository,
            &unsafe_repo.integration,
            "integration",
            "part",
            "unsafe",
        )
        .unwrap();
    assert_eq!(
        outcome,
        IntegrationOutcome::Conflict {
            files: vec!["src/main.rs".into()]
        }
    );
    assert!(output(&unsafe_repo.integration, ["status", "--porcelain"])
        .unwrap()
        .is_empty());
}

struct RepositoryFixture {
    _temp: tempfile::TempDir,
    repository: std::path::PathBuf,
    integration: std::path::PathBuf,
}

impl RepositoryFixture {
    fn new() -> Self {
        Self::with_optional_base_file(None)
    }

    fn with_base_file(path: &str, contents: &str) -> Self {
        Self::with_optional_base_file(Some((path, contents)))
    }

    fn with_optional_base_file(file: Option<(&str, &str)>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        fs::create_dir_all(&repository).unwrap();
        git(&repository, ["init", "-q"]);
        git(&repository, ["config", "user.name", "Neomax Test"]);
        git(
            &repository,
            ["config", "user.email", "test@example.invalid"],
        );
        fs::write(repository.join("base.txt"), "base\n").unwrap();
        if let Some((path, contents)) = file {
            let path = repository.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        git(&repository, ["add", "."]);
        git(&repository, ["commit", "-qm", "base"]);
        git(&repository, ["branch", "integration"]);
        git(&repository, ["branch", "part"]);
        let integration = temp.path().join("integration");
        git(
            &repository,
            [
                "worktree",
                "add",
                "-q",
                integration.to_str().unwrap(),
                "integration",
            ],
        );
        Self {
            _temp: temp,
            repository,
            integration,
        }
    }

    fn commit_on(&self, branch: &str, path: &str, contents: &str) {
        let checkout = if branch == "integration" {
            &self.integration
        } else {
            &self.repository
        };
        if branch != "integration" {
            git(checkout, ["checkout", "-q", branch]);
        }
        let path = checkout.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        git(checkout, ["add", "."]);
        git(checkout, ["commit", "-qm", &format!("change {branch}")]);
    }
}

fn git<const N: usize>(cwd: &Path, args: [&str; N]) {
    let mut safe_args = vec![
        "-c",
        "core.hooksPath=__neomax_no_test_hooks__",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "core.fsmonitor=false",
    ];
    safe_args.extend(args);
    let result = invoke(cwd, safe_args).unwrap();
    assert!(result.success, "{}", result.stderr_text());
}
