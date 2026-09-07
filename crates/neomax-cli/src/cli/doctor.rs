use anyhow::{Result, bail};
use neomax_core::providers::catalog::{MapEnvironment, RealFileSystem};
use neomax_core::{SettingsFile, StatePaths};

pub(crate) fn run(args: &[String]) -> Result<()> {
    if args
        .iter()
        .any(|arg| !matches!(arg.as_str(), "--json" | "--help" | "-h"))
    {
        bail!("usage: neomax doctor [--json]");
    }
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!(
            "neomax doctor [--json]\nInspect local account, quota, configuration, and tool-manifest evidence. No login, refresh, provider launch, or repair is performed."
        );
        return Ok(());
    }
    let paths = StatePaths::discover()?;
    let environment = MapEnvironment::new(std::env::vars())
        .with_home(&paths.home)
        .with_current_dir(std::env::current_dir()?);
    let accounts = neomax_core::orchestration::diagnostics::inspect_accounts(
        &paths,
        &environment,
        &RealFileSystem,
        crate::context::unix_now(),
    )?;
    let settings_path = SettingsFile::discover_path()?;
    let settings_valid = SettingsFile::load(&settings_path)
        .and_then(|file| {
            neomax_core::EffectiveSettings::resolve(
                file,
                settings_path.clone(),
                &std::env::vars().collect(),
            )
        })
        .is_ok();
    let manifest_path = paths
        .state
        .join(neomax_core::agent_tools::canonical_manifest_relative_path());
    let manifest = if !manifest_path.exists() {
        "not_created"
    } else if neomax_core::agent_tools::ManifestStore::new(&manifest_path)
        .read_private_canonical()
        .is_ok()
    {
        "valid"
    } else {
        "invalid"
    };
    let report = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"read_only":true,"settings_valid":settings_valid,"manifest":manifest,"accounts":accounts,"refresh_verified":false});
    if args.iter().any(|arg| arg == "--json") {
        return crate::output::json(&report);
    }
    println!(
        "Neomax {} | settings {} | manifest {manifest}",
        env!("CARGO_PKG_VERSION"),
        if settings_valid { "valid" } else { "invalid" }
    );
    for account in accounts {
        println!(
            "{} {}: {:?}; quota {}",
            account.engine,
            account.email.as_deref().unwrap_or(&account.account),
            account.credential.health,
            account.quota.freshness
        );
    }
    println!("Local evidence only. Refresh tokens and account access were not verified remotely.");
    Ok(())
}
