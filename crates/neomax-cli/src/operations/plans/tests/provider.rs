use std::collections::BTreeMap;

use neomax_core::WorkerScope;
use neomax_core::providers::catalog::CatalogSnapshot;
use neomax_core::providers::runtime::ProviderRuntime;

use super::super::lifecycle::{PlanFactory, ProductionPlanFactory};

#[test]
fn production_factory_is_constructible_without_provider_authentication() {
    let fixture = tempfile::tempdir().unwrap();
    let paths = neomax_core::StatePaths::new(fixture.path(), fixture.path().join("state"));
    let settings = neomax_core::EffectiveSettings::resolve(
        neomax_core::SettingsFile::default(),
        paths.state.join("config.toml"),
        &BTreeMap::new(),
    )
    .unwrap();
    let provider_runtime = ProviderRuntime::from_catalog(CatalogSnapshot {
        providers: BTreeMap::new(),
    });
    assert!(provider_runtime.catalog().providers.is_empty());
    let factory = ProductionPlanFactory::new(paths, settings, WorkerScope::all(), provider_runtime);
    let _factory_trait: &dyn PlanFactory<
        Lifecycle = super::super::lifecycle::ProductionPlanLifecycle,
    > = &factory;
}
