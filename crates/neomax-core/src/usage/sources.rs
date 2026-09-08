use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::Engine;
use crate::providers::catalog::spec;

/// Local telemetry roots are independent of authentication and launch eligibility.
pub fn local_usage_roots(home: &Path) -> Vec<(Engine, PathBuf)> {
    let mut roots = BTreeSet::new();
    for engine in Engine::ALL {
        roots.insert((engine, home.join(spec(engine).default_profile_dir)));
    }
    if let Ok(entries) = fs::read_dir(home) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            for engine in Engine::ALL {
                let prefix = spec(engine).default_profile_dir;
                if name.starts_with(&format!("{prefix}-")) && entry.path().is_dir() {
                    roots.insert((engine, entry.path()));
                }
            }
        }
    }
    for base in [
        home.join("Library/Application Support/Claude"),
        home.join("AppData/Roaming/Claude"),
        home.join(".config/Claude"),
    ] {
        for child in ["local-agent-mode-sessions", "claude-code-sessions"] {
            for entry in walkdir::WalkDir::new(base.join(child))
                .follow_links(false)
                .max_depth(6)
                .into_iter()
                .filter_entry(|entry| !matches!(entry.file_name().to_str(), Some("skills-plugin" | "rpm" | "node_modules")))
                .flatten()
            {
                if entry.file_type().is_dir()
                    && entry.file_name() == ".claude"
                    && entry.path().join("projects").is_dir()
                {
                    roots.insert((Engine::Claude, entry.path().to_path_buf()));
                }
            }
        }
    }
    roots.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_native_and_desktop_usage_without_neomax_accounts_or_credentials() {
        let temp = tempfile::tempdir().unwrap();
        let standalone = temp.path().join(".codex-personal");
        let solo = temp.path().join(".claude-solo");
        let desktop = temp.path().join("Library/Application Support/Claude/local-agent-mode-sessions/org/user/run/.claude");
        for root in [&standalone, &solo, &desktop] {
            fs::create_dir_all(root.join("projects")).unwrap();
        }
        let roots = local_usage_roots(temp.path());
        assert!(roots.contains(&(Engine::Codex, temp.path().join(".codex"))));
        assert!(roots.contains(&(Engine::Codex, standalone)));
        assert!(roots.contains(&(Engine::Claude, solo)));
        assert!(roots.contains(&(Engine::Claude, desktop)));
        assert_eq!(roots.iter().filter(|(engine, _)| *engine == Engine::Kimi).count(), 1);
    }
}
