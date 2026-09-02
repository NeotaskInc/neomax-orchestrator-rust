use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountSelector {
    Number(String),
    Orchestrator,
    Profile(PathBuf),
}

impl AccountSelector {
    pub fn number(&self) -> Option<&str> {
        match self {
            Self::Number(value) => Some(value),
            Self::Orchestrator | Self::Profile(_) => None,
        }
    }

    pub fn profile(&self) -> Option<&Path> {
        match self {
            Self::Profile(value) => Some(value),
            Self::Number(_) | Self::Orchestrator => None,
        }
    }
}

pub fn parse_account_selectors<S: AsRef<str>>(tokens: &[S]) -> Vec<AccountSelector> {
    tokens
        .iter()
        .filter_map(|token| parse_account_selector(token.as_ref()))
        .collect()
}

pub fn parse_account_selector(token: &str) -> Option<AccountSelector> {
    let value = token.trim().trim_end_matches(',');
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || matches!(
            lower.as_str(),
            "account" | "accounts" | "acct" | "accts" | "to" | "and"
        )
    {
        return None;
    }
    if let Some(number) = number_word(&lower) {
        return Some(AccountSelector::Number(number.to_string()));
    }
    if lower == "orch" {
        return Some(AccountSelector::Orchestrator);
    }
    if let Some(number) = wrapped_number(&lower) {
        return Some(AccountSelector::Number(number.to_string()));
    }
    if lower.chars().all(|character| character.is_ascii_digit()) {
        return Some(AccountSelector::Number(lower));
    }
    if crate::io::is_rooted_but_not_absolute(Path::new(value)) {
        return None;
    }
    Some(AccountSelector::Profile(normalize_profile_path(value)))
}

pub fn normalize_profile_path(value: impl AsRef<Path>) -> PathBuf {
    normalize_profile_path_with_environment(value, &crate::runtime::RuntimeEnvironment::process())
}

pub(super) fn normalize_profile_path_with_environment(
    value: impl AsRef<Path>,
    runtime: &crate::runtime::RuntimeEnvironment,
) -> PathBuf {
    let raw = expand_home(value.as_ref(), runtime);
    if crate::io::is_rooted_but_not_absolute(&raw) {
        return raw;
    }
    let absolute = if raw.is_absolute() {
        raw
    } else {
        runtime.current_dir().join(raw)
    };
    lexical_normalize(&absolute)
}

fn wrapped_number(value: &str) -> Option<&str> {
    let rest = value
        .strip_prefix("account")
        .or_else(|| value.strip_prefix("acct"))?;
    let rest = rest
        .strip_prefix('-')
        .or_else(|| rest.strip_prefix('_'))
        .unwrap_or(rest);
    if !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit()) {
        Some(rest)
    } else {
        None
    }
}

fn expand_home(path: &Path, runtime: &crate::runtime::RuntimeEnvironment) -> PathBuf {
    let text = path.to_string_lossy();
    let is_home = text == "~" || text.starts_with("~/") || text.starts_with("~\\");
    if is_home && runtime.home_dir().is_some() {
        return runtime.resolve_path(&text);
    }
    path.to_path_buf()
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !output.pop() && !path.is_absolute() {
                    output.push(component.as_os_str());
                }
            }
            Component::Normal(value) => output.push(value),
        }
    }
    if output.as_os_str().is_empty() {
        PathBuf::from(std::path::MAIN_SEPARATOR.to_string())
    } else {
        output
    }
}

fn number_word(value: &str) -> Option<&'static str> {
    Some(match value {
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        "five" => "5",
        "six" => "6",
        "seven" => "7",
        "eight" => "8",
        "nine" => "9",
        "ten" => "10",
        _ => return None,
    })
}
