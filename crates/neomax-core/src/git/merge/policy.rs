use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

pub fn is_union_safe(path: &Path) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };
    union_safe_pattern().is_match(path)
}

fn union_safe_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?i)(^|/)CHANGELOG(\.[a-z]+)?$|(^|/)\.gitignore$|(^|/)\.neomax/|\.(log|ndjson)$",
        )
        .expect("union-safe path pattern is valid")
    })
}
