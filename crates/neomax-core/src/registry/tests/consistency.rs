use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::super::{DOMAINS, PUBLIC_MODULE_PATHS, PUBLIC_MODULES, validate_architecture};

#[test]
fn domain_registry_has_unique_public_owners() {
    let names = DOMAINS
        .iter()
        .map(|domain| domain.name)
        .collect::<HashSet<_>>();
    assert_eq!(names.len(), DOMAINS.len());
    assert_eq!(PUBLIC_MODULES.len(), DOMAINS.len());
    assert_eq!(PUBLIC_MODULE_PATHS.len(), DOMAINS.len());
    assert_eq!(
        PUBLIC_MODULES,
        &DOMAINS.iter().map(|domain| domain.name).collect::<Vec<_>>()
    );
    assert_eq!(
        PUBLIC_MODULE_PATHS,
        &DOMAINS
            .iter()
            .map(|domain| domain.module)
            .collect::<Vec<_>>()
    );
    assert!(DOMAINS.iter().all(|domain| {
        let owners = domain.owner_paths.iter().copied().collect::<HashSet<_>>();
        !domain.name.is_empty()
            && domain.module == format!("neomax_core::{}", domain.name)
            && !domain.responsibility.is_empty()
            && !domain.owner_paths.is_empty()
            && owners.len() == domain.owner_paths.len()
    }));

    let owners = DOMAINS
        .iter()
        .flat_map(|domain| domain.owner_paths.iter().copied())
        .collect::<Vec<_>>();
    let unique_owners = owners.iter().copied().collect::<HashSet<_>>();
    assert_eq!(
        unique_owners.len(),
        owners.len(),
        "a source owner may belong to only one domain"
    );
}

#[test]
fn runtime_architecture_validation_accepts_the_generated_registry() {
    validate_architecture().unwrap();
}

#[test]
fn root_exports_are_generated_instead_of_maintained_separately() {
    let lib = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("core root is readable");
    let explicit = lib
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub mod "))
        .map(|name| name.trim_end_matches(';'))
        .filter(|name| {
            name.chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric())
        })
        .collect::<Vec<_>>();
    assert_eq!(explicit, vec!["registry"]);
}
