use std::path::PathBuf;

use super::*;

#[test]
fn memory_source_is_deterministic_and_cutoff_aware() {
    let profile = PathBuf::from("/profile");
    let source = MemoryArtifactSource::new([
        artifact(
            &profile,
            profile.join("projects/p/session.jsonl"),
            ArtifactKind::ClaudeMain,
            10,
            b"{}".to_vec(),
        ),
        artifact(
            &profile,
            profile.join("projects/p/old.jsonl"),
            ArtifactKind::ClaudeMain,
            1,
            b"{}".to_vec(),
        ),
    ]);
    let rows = source
        .discover(&profile, ArtifactKind::ClaudeMain, 5)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path.file_name().unwrap(), "session.jsonl");
}

#[test]
fn filesystem_source_rejects_journals_and_unbounded_files() {
    let temp = tempfile::tempdir().unwrap();
    let sub = temp.path().join("projects/p/s/subagents");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("journal.jsonl"), b"{}").unwrap();
    std::fs::write(sub.join("agent.jsonl"), b"{}").unwrap();
    let source = FsArtifactSource::new(1);
    assert!(source
        .discover(temp.path(), ArtifactKind::ClaudeSubagent, 0)
        .unwrap()
        .is_empty());
}

#[test]
fn filesystem_index_discovers_provider_sources_once() {
    let temp = tempfile::tempdir().unwrap();
    let profile = temp.path();
    std::fs::create_dir_all(profile.join("projects/p")).unwrap();
    std::fs::create_dir_all(profile.join("sessions/s1/agents/main")).unwrap();
    std::fs::create_dir_all(profile.join("sessions/s2")).unwrap();
    std::fs::create_dir_all(profile.join("sessions/2026")).unwrap();
    std::fs::write(profile.join("projects/p/session.jsonl"), b"{}").unwrap();
    std::fs::write(profile.join("sessions/2026/rollout-s1.jsonl"), b"{}").unwrap();
    std::fs::write(profile.join("sessions/s1/state.json"), b"{}").unwrap();
    std::fs::write(profile.join("sessions/s1/agents/main/wire.jsonl"), b"{}").unwrap();
    std::fs::write(profile.join("sessions/s2/summary.json"), b"{}").unwrap();
    std::fs::write(profile.join("sessions/s2/updates.jsonl"), b"{}").unwrap();
    std::fs::write(profile.join("opencode.db"), b"sqlite").unwrap();
    std::fs::create_dir_all(profile.join("opencode")).unwrap();
    std::fs::write(profile.join("opencode/opencode.db"), b"sqlite").unwrap();

    let index = FsArtifactSource::default().index(profile, 0).unwrap();
    assert_eq!(index.profile(), profile);
    assert_eq!(index.by_kind(ArtifactKind::ClaudeMain).count(), 1);
    assert_eq!(index.by_kind(ArtifactKind::CodexRollout).count(), 1);
    assert_eq!(index.by_kind(ArtifactKind::KimiState).count(), 1);
    assert_eq!(index.by_kind(ArtifactKind::KimiWire).count(), 1);
    assert_eq!(index.by_kind(ArtifactKind::GrokSummary).count(), 1);
    assert_eq!(index.by_kind(ArtifactKind::GrokUpdates).count(), 1);
    assert_eq!(index.by_kind(ArtifactKind::OpenCodeDatabase).count(), 2);
}

#[test]
fn index_with_home_includes_the_canonical_opencode_database_location() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let profile = home.join(".opencode");
    let database = home.join(".local/share/opencode/opencode.db");
    std::fs::create_dir_all(database.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(&database, b"sqlite").unwrap();

    let index = FsArtifactSource::default()
        .index_with_home(&profile, &home, 0)
        .unwrap();
    let database = index
        .by_kind(ArtifactKind::OpenCodeDatabase)
        .next()
        .unwrap();
    assert_eq!(
        database.path,
        home.join(".local/share/opencode/opencode.db")
    );
}
