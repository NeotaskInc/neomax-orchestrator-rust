#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Domain {
    pub name: &'static str,
    pub module: &'static str,
    pub responsibility: &'static str,
    pub owner_paths: &'static [&'static str],
}

#[doc(hidden)]
#[macro_export]
macro_rules! neomax_domain_declarations {
    ($callback:path) => {
        $callback! {
            accounts => {
                declaration: generated,
                module: "neomax_core::accounts",
                responsibility: "authentication, cooldowns, pauses, eligibility, and selection",
                owners: [
                    "accounts",
                    "accounts::claims",
                    "accounts::controls",
                    "accounts::inventory",
                    "accounts::inventory::builder",
                    "accounts::inventory::quota",
                    "accounts::ports",
                    "accounts::selection",
                    "accounts::snapshot",
                    "accounts::windows",
                ],
            },
            agent_tools => {
                declaration: generated,
                module: "neomax_core::agent_tools",
                responsibility: "provider-neutral agent command manifests, resolution, and authorization",
                owners: [
                    "agent_tools",
                    "agent_tools::commands",
                    "agent_tools::environment",
                    "agent_tools::guard",
                    "agent_tools::invocation",
                    "agent_tools::manifest",
                    "agent_tools::persistence",
                    "agent_tools::policy",
                    "agent_tools::prepared",
                    "agent_tools::resolution",
                    "agent_tools::role",
                    "agent_tools::types",
                ],
            },
            atomic => {
                declaration: generated,
                module: "neomax_core::atomic",
                responsibility: "atomic and fail-closed state persistence",
                owners: ["atomic"],
            },
            concurrency => {
                declaration: generated,
                module: "neomax_core::concurrency",
                responsibility: "global, task, account, and lane admission policy",
                owners: [
                    "concurrency",
                    "concurrency::dispatch",
                    "concurrency::dispatch::capacity",
                    "concurrency::dispatch::clock",
                    "concurrency::dispatch::constants",
                    "concurrency::dispatch::lease",
                    "concurrency::dispatch::limits",
                    "concurrency::dispatch::rejection",
                    "concurrency::dispatch::request",
                    "concurrency::dispatch::schema",
                    "concurrency::dispatch::store",
                ],
            },
            config => {
                declaration: generated,
                module: "neomax_core::config",
                responsibility: "runtime paths, defaults, engines, and worker scopes",
                owners: ["config"],
            },
            error => {
                declaration: generated,
                module: "neomax_core::error",
                responsibility: "provider-neutral error types and result boundaries",
                owners: ["error"],
            },
            git => {
                declaration: generated,
                module: "neomax_core::git",
                responsibility: "repository discovery, diffs, worktrees, branches, and merge safety",
                owners: [
                    "git",
                    "git::command",
                    "git::inspection",
                    "git::merge",
                    "git::workspace",
                    "git::worktree",
                    "git::inspection::inspector",
                    "git::inspection::runner",
                    "git::inspection::types",
                    "git::pull_request",
                    "git::pull_request::adapter",
                    "git::pull_request::ports",
                    "git::pull_request::types",
                    "git::merge::integrator",
                    "git::merge::policy",
                    "git::merge::resolver",
                    "git::merge::union",
                    "git::workspace::allocation",
                    "git::workspace::branch",
                    "git::workspace::cleanup",
                    "git::workspace::identity",
                    "git::workspace::types",
                    "git::worktree::artifacts",
                    "git::worktree::inspection",
                    "git::worktree::manager",
                    "git::worktree::state",
                ],
            },
            installation => {
                declaration: generated,
                module: "neomax_core::installation",
                responsibility: "portable installation, package manifests, and transactional updates",
                owners: [
                    "installation",
                    "installation::files",
                    "installation::install",
                    "installation::manifest",
                    "installation::package",
                    "installation::paths",
                    "installation::transaction",
                    "installation::transaction::activation",
                    "installation::transaction::platform",
                    "installation::transaction::rollback",
                    "installation::transaction::staging",
                    "installation::transaction::state",
                    "installation::transaction::validation",
                    "installation::types",
                    "installation::uninstall",
                    "installation::workflows",
                    "installation::workflows::hooks",
                    "installation::workflows::manifest",
                    "installation::workflows::staging",
                    "installation::workflows::support",
                    "installation::workflows::targets",
                    "installation::workflows::uninstall",
                    "installation::windows",
                ],
            },
            io => {
                declaration: generated,
                module: "neomax_core::io",
                responsibility: "injectable clocks, event partitioning, files, process groups, and bounded readers",
                owners: [
                    "io",
                    "io::clock",
                    "io::error",
                    "io::event_partition",
                    "io::files",
                    "io::permissions",
                    "io::permissions::other",
                    "io::permissions::unix",
                    "io::permissions::windows",
                    "io::process",
                    "io::process_group",
                    "io::process_group::other",
                    "io::process_group::unix",
                    "io::process_group::windows",
                    "io::reader",
                    "io::text",
                    "io::windows_paths",
                ],
            },
            issues => {
                declaration: generated,
                module: "neomax_core::issues",
                responsibility: "cross-repository issues, queues, CI, and delivery",
                owners: [
                    "issues",
                    "issues::ci",
                    "issues::claim",
                    "issues::claims",
                    "issues::coordinator",
                    "issues::event",
                    "issues::fingerprint",
                    "issues::mirror",
                    "issues::schema",
                    "issues::status",
                    "issues::store",
                    "issues::types",
                    "issues::coordinator::brief",
                    "issues::coordinator::driver",
                    "issues::coordinator::service",
                    "issues::coordinator::types",
                    "issues::store::audit",
                    "issues::store::claims",
                    "issues::store::core",
                    "issues::store::links",
                ],
            },
            models => {
                declaration: generated,
                module: "neomax_core::models",
                responsibility: "provider model defaults, validation, and user override resolution",
                owners: ["models"],
            },
            orchestration => {
                declaration: generated,
                module: "neomax_core::orchestration",
                responsibility: "modes, routing, credential rotation, handoff, and recovery",
                owners: [
                    "orchestration",
                    "orchestration::auth",
                    "orchestration::auth::backup",
                    "orchestration::auth::backup::document",
                    "orchestration::auth::backup::encoding",
                    "orchestration::auth::backup::legacy",
                    "orchestration::auth::backup::names",
                    "orchestration::auth::backup::store",
                    "orchestration::auth::claude",
                    "orchestration::auth::codex",
                    "orchestration::auth::limits",
                    "orchestration::auth::permissions",
                    "orchestration::auth::policy",
                    "orchestration::auth::restore",
                    "orchestration::auth::rotation_log",
                    "orchestration::commands",
                    "orchestration::continuation",
                    "orchestration::continuation::ports",
                    "orchestration::continuation::request",
                    "orchestration::handoff",
                    "orchestration::continuation::state",
                    "orchestration::handoff::advice",
                    "orchestration::handoff::baton",
                    "orchestration::handoff::command",
                    "orchestration::handoff::platform",
                    "orchestration::handoff::role",
                    "orchestration::registry",
                    "orchestration::registry::liveness",
                    "orchestration::registry::ownership",
                    "orchestration::registry::record",
                    "orchestration::rotation",
                    "orchestration::rotation::armed",
                    "orchestration::rotation::cooldown",
                    "orchestration::rotation::policy",
                    "orchestration::rotation::types",
                    "orchestration::selection",
                    "orchestration::selection::account",
                    "orchestration::auth::service",
                    "orchestration::auth::transaction",
                    "orchestration::auth::types",
                    "orchestration::auth::writer",
                    "orchestration::continuation::service",
                    "orchestration::handoff::selection",
                    "orchestration::registry::store",
                    "orchestration::rotation::selectors",
                    "orchestration::selection::priority",
                    "orchestration::selection::state",
                    "orchestration::selection::types",
                    "orchestration::selection::dynamic",
                ],
            },
            projects => {
                declaration: generated,
                module: "neomax_core::projects",
                responsibility: "portable project registration, discovery, and ownership",
                owners: [
                    "projects",
                    "projects::discovery",
                    "projects::orientation",
                    "projects::registry",
                    "projects::slug",
                    "projects::types",
                ],
            },
            providers => {
                declaration: generated,
                module: "neomax_core::providers",
                responsibility: "provider interfaces, commands, events, and adapters",
                owners: [
                    "providers",
                    "providers::auth",
                    "providers::catalog",
                    "providers::catalog::commands",
                    "providers::catalog::compat",
                    "providers::catalog::discovery",
                    "providers::catalog::eligibility",
                    "providers::catalog::environment",
                    "providers::catalog::filesystem",
                    "providers::catalog::models",
                    "providers::catalog::profile_auth",
                    "providers::catalog::profile_auth_claude",
                    "providers::catalog::profile_auth_codex",
                    "providers::catalog::profile_auth_common",
                    "providers::catalog::profile_auth_grok",
                    "providers::catalog::profile_auth_kimi",
                    "providers::catalog::profile_auth_opencode",
                    "providers::catalog::profile_auth_store",
                    "providers::catalog::profile_paths",
                    "providers::catalog::profiles",
                    "providers::catalog::ranking",
                    "providers::catalog::specs",
                    "providers::catalog::types",
                    "providers::claude",
                    "providers::codex",
                    "providers::event_types",
                    "providers::events",
                    "providers::grok",
                    "providers::kimi",
                    "providers::kimi_plan",
                    "providers::kimi_plan::config",
                    "providers::kimi_plan::credentials",
                    "providers::kimi_plan::platform",
                    "providers::kimi_plan::profile_state",
                    "providers::kimi_plan::staging",
                    "providers::opencode",
                    "providers::opencode_policy",
                    "providers::orchestrator",
                    "providers::orchestrator::command",
                    "providers::orchestrator::command::environment",
                    "providers::orchestrator::command::instructions",
                    "providers::orchestrator::command::root",
                    "providers::orchestrator::command::root::claude",
                    "providers::orchestrator::command::root::codex",
                    "providers::orchestrator::command::root::grok",
                    "providers::orchestrator::command::root::kimi",
                    "providers::orchestrator::command::root::opencode",
                    "providers::orchestrator::command::solo",
                    "providers::orchestrator::command::solo::claude",
                    "providers::orchestrator::command::solo::codex",
                    "providers::orchestrator::command::solo::grok",
                    "providers::orchestrator::command::solo::kimi",
                    "providers::orchestrator::command::solo::opencode",
                    "providers::process_secret",
                    "providers::runtime",
                    "providers::worker",
                    "providers::events::children",
                    "providers::events::claude",
                    "providers::events::codex",
                    "providers::events::codex_quota",
                    "providers::events::codex_quota::application",
                    "providers::events::codex_quota::request",
                    "providers::events::codex_quota::response",
                    "providers::events::codex_quota::rollout",
                    "providers::events::codex_quota::types",
                    "providers::events::codex_quota::window",
                    "providers::events::codex_usage",
                    "providers::events::common",
                    "providers::events::grok",
                    "providers::events::json",
                    "providers::events::kimi",
                    "providers::events::limits",
                    "providers::events::opencode",
                    "providers::events::token_usage",
                    "providers::orchestrator::types",
                    "providers::orchestrator::validation",
                ],
            },
            queue => {
                declaration: generated,
                module: "neomax_core::queue",
                responsibility: "durable FIFO admission, grants, and orphan recovery",
                owners: [
                    "queue",
                    "queue::allocation",
                    "queue::liveness",
                    "queue::store",
                    "queue::types",
                ],
            },
            registry => {
                declaration: existing,
                module: "neomax_core::registry",
                responsibility: "compile-time domain declarations and source ownership metadata",
                owners: ["registry"],
            },
            runs => {
                declaration: generated,
                module: "neomax_core::runs",
                responsibility: "durable run records, events, supervision, failover, and lifecycle",
                owners: [
                    "runs",
                    "runs::coordinator",
                    "runs::coordinator::attempt",
                    "runs::coordinator::clock",
                    "runs::coordinator::events",
                    "runs::coordinator::loop_runner",
                    "runs::events",
                    "runs::execution",
                    "runs::execution::classify",
                    "runs::execution::logs",
                    "runs::execution::monitor",
                    "runs::execution::process",
                    "runs::execution::signals",
                    "runs::execution::signals::other",
                    "runs::execution::signals::unix",
                    "runs::execution::signals::windows",
                    "runs::failover",
                    "runs::failover::model",
                    "runs::failover::order",
                    "runs::failover::planner",
                    "runs::failover::transition",
                    "runs::failover::types",
                    "runs::history",
                    "runs::history::archive",
                    "runs::history::query",
                    "runs::history::schema",
                    "runs::history::serde_helpers",
                    "runs::history::types",
                    "runs::lifecycle",
                    "runs::lifecycle::attempt",
                    "runs::lifecycle::cooldown",
                    "runs::lifecycle::types",
                    "runs::lifecycle::worktree",
                    "runs::lifecycle::pull_request",
                    "runs::live_work",
                    "runs::live_work::process",
                    "runs::live_work::process::windows",
                    "runs::live_work::process::windows::handles",
                    "runs::live_work::process::windows::inspector",
                    "runs::live_work::process::windows::parsing",
                    "runs::live_work::process::windows::remote",
                    "runs::live_work::process::windows::security",
                    "runs::liveness",
                    "runs::reconciliation",
                    "runs::reconciliation::policy",
                    "runs::reconciliation::schema",
                    "runs::reconciliation::service",
                    "runs::reconciliation::store",
                    "runs::reconciliation::types",
                    "runs::record",
                    "runs::record::wire",
                    "runs::store",
                    "runs::execution::prepare",
                    "runs::execution::tooling",
                    "runs::execution::tooling::environment",
                    "runs::execution::tooling::manifest",
                    "runs::execution::tooling::types",
                    "runs::execution::types",
                    "runs::lifecycle::finalize",
                ],
            },
            scheduler => {
                declaration: generated,
                module: "neomax_core::scheduler",
                responsibility: "plan validation, dependency state, area locks, dispatch, and reconciliation",
                owners: [
                    "scheduler",
                    "scheduler::area",
                    "scheduler::graph",
                    "scheduler::locks",
                    "scheduler::persistence",
                    "scheduler::plan",
                    "scheduler::runtime",
                    "scheduler::service",
                    "scheduler::state",
                    "scheduler::types",
                    "scheduler::validation",
                    "scheduler::locks::acquire",
                    "scheduler::locks::liveness",
                    "scheduler::locks::manager",
                    "scheduler::locks::owner",
                    "scheduler::locks::paths",
                    "scheduler::persistence::events",
                    "scheduler::persistence::record",
                    "scheduler::persistence::store",
                    "scheduler::persistence::transitions",
                    "scheduler::persistence::types",
                    "scheduler::persistence::validation",
                    "scheduler::runtime::coordinator",
                    "scheduler::runtime::admission",
                    "scheduler::runtime::clock",
                    "scheduler::runtime::coordinator::dispatching",
                    "scheduler::runtime::coordinator::model",
                    "scheduler::runtime::coordinator::reconciliation",
                    "scheduler::runtime::coordinator::stalled",
                    "scheduler::runtime::coordinator::types",
                    "scheduler::runtime::coordinator::validation",
                    "scheduler::runtime::dispatch",
                    "scheduler::runtime::readiness",
                    "scheduler::runtime::reconciliation",
                    "scheduler::runtime::transitions",
                    "scheduler::service::provider_runner",
                    "scheduler::service::provider_runner::config",
                    "scheduler::service::provider_runner::jobs",
                    "scheduler::service::provider_runner::outcome",
                    "scheduler::service::provider_runner::request",
                    "scheduler::service::provider_runner::run",
                    "scheduler::service::adapters",
                    "scheduler::service::adapters::admission",
                    "scheduler::service::adapters::recovery",
                    "scheduler::service::admission",
                    "scheduler::service::events",
                    "scheduler::service::execution",
                    "scheduler::service::lifecycle",
                    "scheduler::service::model",
                    "scheduler::service::persistence",
                    "scheduler::service::planner",
                    "scheduler::service::ports",
                    "scheduler::service::recovery",
                    "scheduler::service::runner",
                    "scheduler::service::side_effects",
                    "scheduler::service::start",
                    "scheduler::service::sync",
                    "scheduler::service::workspace",
                ],
            },
            sessions => {
                declaration: generated,
                module: "neomax_core::sessions",
                responsibility: "interactive session and native subagent telemetry",
                owners: [
                    "sessions",
                    "sessions::activity",
                    "sessions::artifacts",
                    "sessions::claude",
                    "sessions::codex",
                    "sessions::filters",
                    "sessions::grok",
                    "sessions::headers",
                    "sessions::headers::activity",
                    "sessions::headers::identity",
                    "sessions::headers::metadata",
                    "sessions::headers::usage",
                    "sessions::kimi",
                    "sessions::opencode",
                    "sessions::portal",
                    "sessions::subagents",
                    "sessions::types",
                    "sessions::grok::updates",
                    "sessions::grok::usage",
                    "sessions::artifacts::encoding",
                    "sessions::artifacts::filesystem",
                    "sessions::artifacts::index",
                    "sessions::artifacts::matching",
                    "sessions::artifacts::memory",
                    "sessions::artifacts::source",
                    "sessions::artifacts::types",
                    "sessions::kimi::wire",
                    "sessions::opencode::common",
                    "sessions::opencode::extraction",
                    "sessions::opencode::schema",
                    "sessions::opencode::sqlite",
                    "sessions::opencode::sqlite::connection",
                    "sessions::opencode::sqlite::discovery",
                    "sessions::opencode::sqlite::paths",
                    "sessions::opencode::sqlite::query",
                    "sessions::opencode::sqlite::rows",
                ],
            },
            settings => {
                declaration: generated,
                module: "neomax_core::settings",
                responsibility: "user configuration, environment precedence, and concurrency policy",
                owners: [
                    "settings",
                    "settings::capacity",
                    "settings::constants",
                    "settings::environment",
                    "settings::models",
                    "settings::persistence",
                    "settings::schema",
                    "settings::validation",
                ],
            },
            runtime => {
                declaration: generated,
                module: "neomax_core::runtime",
                responsibility: "cross-platform runtime paths, process environment, and executable resolution",
                owners: [
                    "runtime",
                    "runtime::environment",
                    "runtime::executable",
                    "runtime::paths",
                    "runtime::platform",
                ],
            },
            shepherd => {
                declaration: generated,
                module: "neomax_core::shepherd",
                responsibility: "merge readiness, local Git inspection, and delivery policy",
                owners: [
                    "shepherd",
                    "shepherd::checks",
                    "shepherd::decision",
                    "shepherd::git_inspection",
                    "shepherd::git_runner",
                    "shepherd::policy",
                    "shepherd::types",
                ],
            },
            tasks => {
                declaration: generated,
                module: "neomax_core::tasks",
                responsibility: "durable project task backlogs and task state",
                owners: [
                    "tasks",
                    "tasks::status",
                    "tasks::store",
                    "tasks::types",
                ],
            },
            usage => {
                declaration: generated,
                module: "neomax_core::usage",
                responsibility: "usage ingestion, windows, aggregation, pricing, and collection",
                owners: [
                    "usage",
                    "usage::aggregate",
                    "usage::cache",
                    "usage::ingest",
                    "usage::ledger",
                    "usage::pricing",
                    "usage::report",
                    "usage::types",
                    "usage::report::builder",
                    "usage::report::details",
                    "usage::report::metrics",
                    "usage::report::rows",
                ],
            },
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __neomax_define_domain_metadata {
    (
        $(
            $name:ident => {
                declaration: $declaration:ident,
                module: $module:literal,
                responsibility: $responsibility:literal,
                owners: [$($owner:literal),* $(,)?],
            }
        ),* $(,)?
    ) => {
        pub const DOMAINS: &[Domain] = &[
            $(Domain {
                name: stringify!($name),
                module: $module,
                responsibility: $responsibility,
                owner_paths: &[$($owner),*],
            }),*
        ];

        pub const PUBLIC_MODULES: &[&str] = &[
            $(stringify!($name)),*
        ];

        pub const PUBLIC_MODULE_PATHS: &[&str] = &[
            $($module),*
        ];
    };
}

crate::neomax_domain_declarations!(crate::__neomax_define_domain_metadata);

pub fn validate_architecture() -> crate::Result<()> {
    use std::collections::BTreeSet;

    if DOMAINS.is_empty() {
        return Err(crate::Error::InvalidArgument(
            "domain registry is empty".into(),
        ));
    }

    let mut names = BTreeSet::new();
    let mut modules = BTreeSet::new();
    let mut owners: BTreeSet<&str> = BTreeSet::new();
    for domain in DOMAINS {
        if domain.name.is_empty() || domain.responsibility.trim().is_empty() {
            return Err(crate::Error::InvalidArgument(format!(
                "domain metadata is incomplete: {:?}",
                domain.name
            )));
        }
        if domain.module != format!("neomax_core::{}", domain.name) {
            return Err(crate::Error::InvalidArgument(format!(
                "domain {} has an invalid module path {}",
                domain.name, domain.module
            )));
        }
        if !names.insert(domain.name) {
            return Err(crate::Error::InvalidArgument(format!(
                "duplicate domain name: {}",
                domain.name
            )));
        }
        if !modules.insert(domain.module) {
            return Err(crate::Error::InvalidArgument(format!(
                "duplicate domain module: {}",
                domain.module
            )));
        }
        if domain.owner_paths.is_empty() || !domain.owner_paths.contains(&domain.name) {
            return Err(crate::Error::InvalidArgument(format!(
                "domain {} has no root owner path",
                domain.name
            )));
        }
        for owner in domain.owner_paths {
            if owner.is_empty()
                || (*owner != domain.name && !owner.starts_with(&format!("{}::", domain.name)))
            {
                return Err(crate::Error::InvalidArgument(format!(
                    "owner {} escapes domain {}",
                    owner, domain.name
                )));
            }
            if !owners.insert(*owner) {
                return Err(crate::Error::InvalidArgument(format!(
                    "duplicate owner across domain registry: {}",
                    owner
                )));
            }
        }
    }
    if PUBLIC_MODULES.len() != DOMAINS.len() || PUBLIC_MODULE_PATHS.len() != DOMAINS.len() {
        return Err(crate::Error::InvalidArgument(
            "public module metadata is out of sync with domains".into(),
        ));
    }
    for (index, domain) in DOMAINS.iter().enumerate() {
        if PUBLIC_MODULES[index] != domain.name || PUBLIC_MODULE_PATHS[index] != domain.module {
            return Err(crate::Error::InvalidArgument(format!(
                "public module metadata disagrees with domain {}",
                domain.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
