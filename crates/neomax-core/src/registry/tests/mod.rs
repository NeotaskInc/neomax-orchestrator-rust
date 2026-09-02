mod consistency;
mod source;

use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn source_path(manifest_dir: &Path, module_path: &str) -> Option<PathBuf> {
    let module_path = module_path.strip_prefix("neomax_core::")?;
    let relative = module_path.split("::").collect::<PathBuf>();
    let base = manifest_dir.join("src").join(relative);
    let file = base.with_extension("rs");
    if file.is_file() {
        return Some(file);
    }
    let module = base.join("mod.rs");
    module.is_file().then_some(module)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModuleVisibility {
    Private,
    Public,
    Crate,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ModuleDeclaration {
    pub(super) path: String,
    pub(super) visibility: ModuleVisibility,
    pub(super) cfg_test_only: bool,
    pub(super) inline: bool,
}

pub(super) fn module_declarations(source: &Path, parent: &str) -> Vec<ModuleDeclaration> {
    all_module_declarations(source, parent)
        .into_iter()
        .filter(|module| !module.cfg_test_only && !module.path.ends_with("::tests"))
        .collect()
}

pub(super) fn all_module_declarations(source: &Path, parent: &str) -> Vec<ModuleDeclaration> {
    let contents = fs::read_to_string(source).expect("registered Rust source is readable");
    let mut cfg_test = false;
    let mut pending_cfg: Option<String> = None;
    let mut modules = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if let Some(mut attribute) = pending_cfg.take() {
            attribute.push_str(trimmed);
            if trimmed.contains(")]") {
                cfg_test = cfg_selects_tests(&attribute);
            } else {
                pending_cfg = Some(attribute);
            }
            continue;
        }
        if trimmed.starts_with("#[cfg(") {
            if trimmed.contains(")]") {
                cfg_test = cfg_selects_tests(trimmed);
            } else {
                pending_cfg = Some(trimmed.to_owned());
            }
            continue;
        }
        let Some((visibility, declaration)) = module_declaration(trimmed) else {
            if !trimmed.starts_with('#') {
                cfg_test = false;
            }
            continue;
        };
        let name = declaration
            .split([' ', ';', '{'])
            .next()
            .unwrap_or_default();
        if !name.is_empty() {
            modules.push(ModuleDeclaration {
                path: format!("{parent}::{name}"),
                visibility,
                cfg_test_only: cfg_test,
                inline: declaration.contains('{'),
            });
        }
        cfg_test = false;
    }
    modules
}

pub(super) fn cfg_test_only_module(manifest_dir: &Path, module_path: &str) -> bool {
    let Some(relative) = module_path.strip_prefix("neomax_core::") else {
        return false;
    };
    let mut parts = relative.split("::");
    let Some(root) = parts.next() else {
        return false;
    };
    let mut parent = root.to_owned();
    let Some(mut source) = source_path(manifest_dir, &format!("neomax_core::{parent}")) else {
        return false;
    };

    for child in parts {
        let path = format!("{parent}::{child}");
        let Some(declaration) = all_module_declarations(&source, &parent)
            .into_iter()
            .find(|declaration| declaration.path == path)
        else {
            return false;
        };
        if declaration.cfg_test_only {
            return true;
        }
        parent = path;
        let Some(next_source) = source_path(manifest_dir, &format!("neomax_core::{parent}")) else {
            return false;
        };
        source = next_source;
    }
    false
}

fn module_declaration(trimmed: &str) -> Option<(ModuleVisibility, &str)> {
    if let Some(rest) = trimmed.strip_prefix("pub") {
        let (scope, declaration) = rest.split_once(" mod ")?;
        let visibility = if scope.is_empty() {
            ModuleVisibility::Public
        } else if scope == "(crate)" {
            ModuleVisibility::Crate
        } else {
            ModuleVisibility::Public
        };
        return Some((visibility, declaration));
    }
    trimmed
        .strip_prefix("mod ")
        .map(|declaration| (ModuleVisibility::Private, declaration))
}

fn cfg_selects_tests(attribute: &str) -> bool {
    attribute.contains("test") && !attribute.contains("not(test)")
}

pub(super) fn direct_modules(source: &Path, parent: &str) -> Vec<String> {
    module_declarations(source, parent)
        .into_iter()
        .map(|module| module.path)
        .collect()
}
