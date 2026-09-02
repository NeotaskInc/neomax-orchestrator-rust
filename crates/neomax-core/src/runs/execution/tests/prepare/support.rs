use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{ConcurrencySettings, EffectiveSettings, Engine, SettingsFile};

pub(super) fn settings() -> EffectiveSettings {
    EffectiveSettings::resolve(
        SettingsFile {
            concurrency: ConcurrencySettings {
                max_subagents: 77,
                ..ConcurrencySettings::default()
            },
            ..SettingsFile::default()
        },
        "config.toml".into(),
        &BTreeMap::new(),
    )
    .unwrap()
}

pub(super) fn orchestrator_profile(root: &Path, label: &str, engine: Engine) -> PathBuf {
    let profile = root.join(label);
    if engine == Engine::Kimi {
        let agents = profile.join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(agents.join("neomax.md"), "# Neomax\n").unwrap();
    }
    profile
}

pub(super) fn argument_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|argument| argument == flag)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}
