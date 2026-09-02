use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::Path;

use super::super::DOMAINS;
use super::{
    ModuleVisibility, all_module_declarations, cfg_test_only_module, direct_modules,
    module_declarations, source_path,
};

#[test]
fn every_registered_owner_is_a_real_rust_module() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for domain in DOMAINS {
        assert!(
            source_path(manifest_dir, domain.module).is_some(),
            "missing source for registered domain {} ({})",
            domain.name,
            domain.module
        );
        for owner in domain.owner_paths {
            assert!(
                *owner == domain.name || owner.starts_with(&format!("{}::", domain.name)),
                "owner {owner} escapes domain {}",
                domain.name
            );
            let qualified = format!("neomax_core::{owner}");
            assert!(
                source_path(manifest_dir, &qualified).is_some(),
                "missing source for registered owner {qualified}"
            );
            assert!(
                !cfg_test_only_module(manifest_dir, &qualified),
                "registered owner {qualified} is cfg(test)-only"
            );
        }
    }
}

#[test]
fn every_direct_production_child_is_registered() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for domain in DOMAINS {
        let Some(source) = source_path(manifest_dir, domain.module) else {
            continue;
        };
        for child in direct_modules(&source, domain.name) {
            assert!(
                domain.owner_paths.contains(&child.as_str()),
                "direct production module {child} is missing from its registry entry"
            );
        }
    }
}

#[test]
fn every_recursive_production_source_module_is_registered() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for domain in DOMAINS {
        let mut pending = VecDeque::from([domain.module.to_owned()]);
        let mut visited = HashSet::new();
        while let Some(module) = pending.pop_front() {
            if !visited.insert(module.clone()) {
                continue;
            }
            let Some(source) = source_path(manifest_dir, &module) else {
                panic!("missing source for registered module {module}");
            };
            let parent = module.trim_start_matches("neomax_core::");
            for declaration in module_declarations(&source, parent) {
                let child = declaration.path;
                let qualified = format!("neomax_core::{child}");
                let Some(child_source) = source_path(manifest_dir, &qualified) else {
                    assert!(
                        declaration.inline,
                        "out-of-line production module {qualified} has no source file"
                    );
                    continue;
                };
                assert!(
                    domain.owner_paths.contains(&child.as_str()),
                    "production source module {child} is missing from the {} domain registry",
                    domain.name
                );
                assert!(child_source.is_file());
                pending.push_back(qualified);
            }
        }
    }
}

#[test]
fn live_work_windows_module_resolves_from_its_declaring_process_module() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        source_path(
            manifest_dir,
            "neomax_core::runs::live_work::process::windows"
        ),
        Some(manifest_dir.join("src/runs/live_work/process/windows/mod.rs"))
    );
}

#[test]
fn every_recursive_public_boundary_is_registered() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    for domain in DOMAINS {
        let mut pending = VecDeque::from([domain.module.to_owned()]);
        let mut visited = HashSet::new();
        while let Some(module) = pending.pop_front() {
            if !visited.insert(module.clone()) {
                continue;
            }
            let Some(source) = source_path(manifest_dir, &module) else {
                panic!("missing source for registered module {module}");
            };
            for declaration in
                all_module_declarations(&source, module.trim_start_matches("neomax_core::"))
            {
                if declaration.cfg_test_only {
                    continue;
                }
                if !matches!(
                    declaration.visibility,
                    ModuleVisibility::Public | ModuleVisibility::Crate
                ) {
                    continue;
                }
                if declaration.inline {
                    continue;
                }
                let child = declaration.path;
                assert!(
                    domain.owner_paths.contains(&child.as_str()),
                    "public module {child} is missing from the {} domain registry",
                    domain.name
                );
                assert!(
                    source_path(manifest_dir, &format!("neomax_core::{child}")).is_some(),
                    "public module {child} has no source file"
                );
                pending.push_back(format!("neomax_core::{child}"));
            }
        }
    }
}

#[test]
fn module_sources_are_not_ambiguous_between_file_and_directory_forms() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust_files = Vec::new();
    collect_rust_files(&source_root, &mut rust_files);

    for source in rust_files {
        if source.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
            let Some(module_dir) = source.parent() else {
                continue;
            };
            let Some(name) = module_dir.file_name() else {
                continue;
            };
            let sibling = module_dir.with_file_name(format!("{}.rs", name.to_string_lossy()));
            assert!(
                !sibling.is_file(),
                "module has both directory and file sources: {} and {}",
                sibling.display(),
                source.display()
            );
        } else {
            let Some(stem) = source.file_stem() else {
                continue;
            };
            let directory_form = source.with_file_name(stem);
            let module_form = directory_form.join("mod.rs");
            assert!(
                !module_form.is_file(),
                "module has both file and directory sources: {} and {}",
                source.display(),
                module_form.display()
            );
        }
    }
}

fn collect_rust_files(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    let entries = fs::read_dir(directory).expect("source directory is readable");
    for entry in entries {
        let entry = entry.expect("source directory entry is readable");
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}
