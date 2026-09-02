use std::fs;

use neomax_core::projects::Project;

use crate::projects;
use crate::tests::{fixture, seed_path};

#[test]
fn registers_portable_project_defaults_and_finds_the_launch_project() {
    let fixture = fixture();
    let root = fixture.context.cwd.join("app");
    fs::create_dir_all(&root).expect("project root");

    projects::register(
        &fixture.context,
        &[
            "--name=demo".into(),
            format!("--root={}", root.display()),
            "--prefix=demo".into(),
        ],
    )
    .expect("register project");

    let project = fixture
        .context
        .project_registry()
        .load()
        .remove("demo")
        .expect("registered project");
    assert_eq!(project.root, root.canonicalize().expect("canonical root"));
    assert_eq!(project.branch_prefix.as_deref(), Some("demo"));
    assert_eq!(
        project.brain.as_deref().and_then(|path| path.to_str()),
        Some("CLAUDE.md")
    );
    assert_eq!(
        project.agents.as_deref().and_then(|path| path.to_str()),
        Some("AGENTS.md")
    );
    assert_eq!(fixture.context.project_for_cwd(), None);

    fs::create_dir_all(root.join("src")).expect("nested source root");
    let nested_context = crate::context::RuntimeContext::for_test(
        fixture.context.paths.clone(),
        fixture.context.settings.clone(),
        root.join("src"),
        fixture.context.now,
        fixture.context.liveness.clone(),
        None,
    );
    assert_eq!(nested_context.project_for_cwd().as_deref(), Some("demo"));
}

#[test]
fn local_seed_participates_in_overrides_and_durable_unregister_state() {
    let fixture = fixture();
    let seed = seed_path(&fixture);
    let seeded_root = fixture.context.cwd.join("seeded");
    fs::create_dir_all(&seeded_root).expect("seeded root");
    let seeded = Project::portable(seeded_root, "seed".into(), fixture.context.now);
    fs::write(
        &seed,
        serde_json::to_vec(&serde_json::json!({ "seeded": seeded })).expect("seed JSON"),
    )
    .expect("write local seed");
    let seeded_context = crate::context::RuntimeContext::for_test(
        fixture.context.paths.clone(),
        fixture.context.settings.clone(),
        fixture.context.cwd.clone(),
        fixture.context.now,
        fixture.context.liveness.clone(),
        Some(seed.clone()),
    );

    projects::list(&seeded_context, &[]).expect("list projects");
    assert!(
        seeded_context
            .project_registry()
            .load()
            .contains_key("seeded")
    );
    let replacement_root = fixture.context.cwd.join("replacement");
    fs::create_dir_all(&replacement_root).expect("replacement root");
    projects::register(
        &seeded_context,
        &[
            "--force".to_owned(),
            "--name".to_owned(),
            "seeded".to_owned(),
            "--root".to_owned(),
            replacement_root.to_str().expect("UTF-8 root").to_owned(),
        ],
    )
    .expect("override seed project");
    assert_eq!(
        seeded_context.project_registry().load()["seeded"].root,
        replacement_root
            .canonicalize()
            .expect("canonical replacement root")
    );

    projects::register(
        &seeded_context,
        &["--unregister".to_owned(), "seeded".to_owned()],
    )
    .expect("unregister merged project");
    assert!(seed.exists());
    let persisted: std::collections::BTreeMap<String, Project> =
        serde_json::from_slice(&fs::read(&fixture.context.paths.projects).expect("state JSON"))
            .expect("project state");
    assert!(!persisted.contains_key("seeded"));
    assert!(
        seeded_context
            .project_registry()
            .load()
            .contains_key("seeded")
    );
}

#[test]
fn unregister_removes_a_registered_project_without_touching_its_root() {
    let fixture = fixture();
    let root = fixture.context.cwd.join("remove-me");
    fs::create_dir_all(&root).expect("project root");
    projects::register(
        &fixture.context,
        &[
            "--name".to_owned(),
            "remove-me".to_owned(),
            "--root".to_owned(),
            root.to_str().expect("UTF-8 root").to_owned(),
        ],
    )
    .expect("register project");
    projects::register(
        &fixture.context,
        &["--unregister".to_owned(), "remove-me".to_owned()],
    )
    .expect("unregister project");
    assert!(
        !fixture
            .context
            .project_registry()
            .load()
            .contains_key("remove-me")
    );
    assert!(root.is_dir());
}
