use std::path::Path;

pub const GLOBAL_AREA: &str = "*";

pub fn affected_area(path: impl AsRef<Path>) -> String {
    let path = path.as_ref().to_string_lossy();
    let components = path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    let Some(first) = components.first().copied() else {
        return GLOBAL_AREA.into();
    };
    if matches!(first, "apps" | "packages") && components.len() >= 2 {
        return format!("{first}/{}", components[1]);
    }
    if first == "src" && components.len() >= 2 {
        return format!("src/{}", components[1]);
    }
    if matches!(first, "test" | "tests" | "docs" | ".github" | "scripts") {
        return first.into();
    }
    first.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_shared_repository_boundaries_to_stable_areas() {
        assert_eq!(affected_area("apps/web/src/main.ts"), "apps/web");
        assert_eq!(affected_area("packages/core/lib.rs"), "packages/core");
        assert_eq!(affected_area("src/main.rs"), "src/main.rs");
        assert_eq!(affected_area("tests/fixtures/basic.json"), "tests");
        assert_eq!(affected_area("README.md"), "README.md");
        assert_eq!(affected_area(""), GLOBAL_AREA);
        assert_eq!(affected_area("\\src\\main.rs"), "src/main.rs");
    }
}
