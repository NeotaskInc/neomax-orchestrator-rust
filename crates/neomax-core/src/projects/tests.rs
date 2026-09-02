use std::collections::BTreeMap;
use std::fs;
#[cfg(windows)]
use std::path::{Path, PathBuf};

use super::*;

fn registry(root: &std::path::Path) -> ProjectRegistry {
    ProjectRegistry::new(root.join("projects.json"), None)
}

#[test]
fn auto_registers_the_launch_directory_and_discovers_repositories() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let root = temp.path().join("workspace/My Product");
    fs::create_dir_all(root.join("api/.git")).unwrap();
    fs::create_dir_all(root.join("web/.git")).unwrap();
    fs::create_dir_all(&home).unwrap();
    let registry = registry(temp.path());
    let name = registry
        .ensure_launch_project(&root, &home, None, 100)
        .unwrap()
        .unwrap();
    assert_eq!(name, "myproduct");
    let project = &registry.load()[&name];
    assert_eq!(project.root, root.canonicalize().unwrap());
    assert_eq!(
        project.repos,
        [
            std::path::PathBuf::from("api"),
            std::path::PathBuf::from("web")
        ]
    );
    assert_eq!(
        project.agents.as_deref(),
        Some(std::path::Path::new("AGENTS.md"))
    );
    assert!(project.auto_registered);
}

#[test]
fn resolves_the_most_specific_owner_and_repository_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let outer = temp.path().join("outer");
    let inner = outer.join("inner");
    fs::create_dir_all(&inner).unwrap();
    let registry = registry(temp.path());
    let mut outer_project = Project::portable(outer.clone(), "oute".into(), 1);
    outer_project.repos = vec!["api".into()];
    registry.register("outer", outer_project, false).unwrap();
    let inner_project = Project::portable(inner.clone(), "inne".into(), 1);
    assert!(registry.register("inner", inner_project, false).is_err());
    assert_eq!(
        registry.project_of(&inner.join("file")),
        Some("outer".into())
    );
    assert_eq!(
        registry.project_of(&temp.path().join("unrelated/api")),
        Some("outer".into())
    );
}

#[test]
fn rejects_overlaps_and_keeps_registration_updates_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let registry = registry(temp.path());
    let one = temp.path().join("one");
    let nested = one.join("nested");
    fs::create_dir_all(&nested).unwrap();
    registry
        .register(
            "one",
            Project::portable(one.clone(), "one".into(), 1),
            false,
        )
        .unwrap();
    assert!(
        registry
            .register("nested", Project::portable(nested, "nest".into(), 1), false,)
            .is_err()
    );
    let removed = registry.unregister("one").unwrap().unwrap();
    assert_eq!(removed.root, one.canonicalize().unwrap());
    assert!(registry.load().is_empty());
}

#[test]
fn concurrent_project_registration_preserves_every_entry() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("projects.json");
    std::thread::scope(|scope| {
        for index in 0..8 {
            let state = &state;
            let root = temp.path().join(format!("root-{index}"));
            fs::create_dir_all(&root).unwrap();
            scope.spawn(move || {
                ProjectRegistry::new(state, None)
                    .register(
                        &format!("project-{index}"),
                        Project::portable(root, format!("p{index}"), 1),
                        false,
                    )
                    .unwrap();
            });
        }
    });
    assert_eq!(ProjectRegistry::new(state, None).load().len(), 8);
}

#[test]
fn never_auto_registers_the_home_or_filesystem_root() {
    let temp = tempfile::tempdir().unwrap();
    let registry = registry(temp.path());
    assert_eq!(
        registry
            .ensure_launch_project(temp.path(), temp.path(), None, 1)
            .unwrap(),
        None
    );
    assert_eq!(
        registry
            .ensure_launch_project(
                temp.path().ancestors().last().unwrap(),
                temp.path(),
                None,
                1,
            )
            .unwrap(),
        None
    );
}

#[test]
fn normalizes_existing_and_lexical_roots_before_overlap_checks() {
    let temp = tempfile::tempdir().unwrap();
    let registry = registry(temp.path());
    let first = temp.path().join("first");
    fs::create_dir_all(&first).unwrap();
    registry
        .register(
            "first",
            Project::portable(first.clone(), "firs".into(), 1),
            false,
        )
        .unwrap();
    registry
        .register(
            "second",
            Project::portable(first.join("../second"), "seco".into(), 1),
            false,
        )
        .unwrap();
    let expected_second = first
        .canonicalize()
        .unwrap()
        .parent()
        .unwrap()
        .join("second");
    assert_eq!(registry.load()["second"].root, expected_second);
    assert!(
        registry
            .register(
                "nested",
                Project::portable(first.join("child/../nested"), "nest".into(), 1),
                false,
            )
            .is_err()
    );
}

#[test]
fn repository_basename_fallback_refuses_ambiguous_owners() {
    let temp = tempfile::tempdir().unwrap();
    let registry = registry(temp.path());
    for name in ["one", "two"] {
        let root = temp.path().join(name);
        fs::create_dir_all(&root).unwrap();
        let mut project = Project::portable(root, name.into(), 1);
        project.repos = vec!["api".into()];
        registry.register(name, project, false).unwrap();
    }
    assert_eq!(
        registry.project_of(&temp.path().join("unrelated/api")),
        None
    );
}

#[test]
fn orientation_requires_the_current_path_to_be_under_the_project_root() {
    let temp = tempfile::tempdir().unwrap();
    let registry = registry(temp.path());
    let root = temp.path().join("project");
    fs::create_dir_all(&root).unwrap();
    let project = Project::portable(root.clone(), "proj".into(), 1);
    registry.register("project", project, false).unwrap();
    assert!(registry.orientation_of(&root.join("src")).is_some());
    assert!(
        registry
            .orientation_of(&temp.path().join("other"))
            .is_none()
    );
}

#[test]
fn mutations_refuse_to_overwrite_a_corrupt_registry() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("projects.json");
    fs::write(&path, b"{").unwrap();
    let registry = ProjectRegistry::new(&path, None);
    assert!(
        registry
            .register(
                "project",
                Project::portable(temp.path().join("project"), "proj".into(), 1),
                false,
            )
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), b"{");
}

#[test]
fn mutations_use_the_merged_seed_and_state_view() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("projects.json");
    let seed = temp.path().join("project/projects.local.json");
    let seeded_root = temp.path().join("seeded");
    let replacement_root = temp.path().join("replacement");
    fs::create_dir_all(&seeded_root).unwrap();
    fs::create_dir_all(&replacement_root).unwrap();
    fs::create_dir_all(seed.parent().unwrap()).unwrap();
    let seeded = Project::portable(seeded_root.clone(), "seed".into(), 1);
    fs::write(
        &seed,
        serde_json::to_vec(&BTreeMap::from([("seeded", seeded.clone())])).unwrap(),
    )
    .unwrap();
    let registry = ProjectRegistry::new(&state, Some(seed.clone()));

    let replacement = Project::portable(replacement_root.clone(), "repl".into(), 2);
    let canonical_replacement_root = replacement_root.canonicalize().unwrap();
    assert!(
        registry
            .register("seeded", replacement.clone(), false)
            .is_err()
    );
    registry
        .register("seeded", replacement.clone(), true)
        .unwrap();
    let persisted: BTreeMap<String, Project> =
        serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert_eq!(
        persisted.get("seeded").unwrap().root,
        canonical_replacement_root
    );
    assert_eq!(
        registry.load().get("seeded").unwrap().root,
        canonical_replacement_root
    );

    let removed = registry.unregister("seeded").unwrap();
    assert_eq!(removed.unwrap().root, canonical_replacement_root);
    let persisted: BTreeMap<String, Project> =
        serde_json::from_slice(&fs::read(&state).unwrap()).unwrap();
    assert!(!persisted.contains_key("seeded"));
    assert_eq!(registry.load().get("seeded").unwrap().root, seeded_root);
}

#[test]
fn unregistering_an_unknown_project_does_not_materialize_seed_state() {
    let temp = tempfile::tempdir().unwrap();
    let state = temp.path().join("projects.json");
    let seed = temp.path().join("projects.local.json");
    fs::write(&seed, b"{}").unwrap();
    let registry = ProjectRegistry::new(&state, Some(seed));

    assert!(registry.unregister("missing").unwrap().is_none());
    assert!(!state.exists());
}

#[cfg(windows)]
#[test]
fn rejects_rooted_and_drive_relative_project_paths() {
    let temp = tempfile::tempdir().unwrap();
    let registry = registry(temp.path());

    assert!(
        registry
            .register(
                "rooted",
                Project::portable(PathBuf::from(r"\workspace"), "root".into(), 1),
                false,
            )
            .is_err()
    );
    assert!(
        registry
            .register(
                "drive-relative",
                Project::portable(PathBuf::from(r"C:workspace"), "drive".into(), 1),
                false,
            )
            .is_err()
    );
    assert_eq!(registry.project_of(Path::new(r"\workspace\src")), None);
    assert_eq!(registry.project_of(Path::new(r"C:workspace\src")), None);
}
