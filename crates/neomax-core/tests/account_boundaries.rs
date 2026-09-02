use std::fs;
use std::path::{Path, PathBuf};

use neomax_core::accounts::{LiveWorkSource, QuotaSnapshotSource};

#[test]
fn account_policy_sources_have_no_runtime_concrete_dependencies() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/accounts");
    let mut files = Vec::new();
    collect_rust_files(&root, &mut files);
    let forbidden = [
        "crate::runs",
        "crate::usage",
        "runs::execution",
        "UsageCacheStore",
        "ProcessProbe",
        "RunStatus",
        "RunStore",
        "SupervisorDirective",
    ];
    for path in files {
        let source = fs::read_to_string(&path).unwrap();
        for needle in forbidden {
            assert!(
                !source.contains(needle),
                "accounts policy file {} imports runtime type {needle}",
                path.display()
            );
        }
    }
}

#[test]
fn account_runtime_ports_remain_object_safe() {
    fn quota_port(_: &dyn QuotaSnapshotSource) {}
    fn live_port(_: &dyn LiveWorkSource) {}
    let _ = (quota_port, live_port);
}

fn collect_rust_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
