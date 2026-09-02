use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

pub fn fixture_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(relative)
}

pub fn fixture_text(relative: &str) -> String {
    fs::read_to_string(fixture_path(relative))
        .unwrap_or_else(|error| panic!("read fixture {relative}: {error}"))
}

pub fn fixture_json(relative: &str) -> Value {
    serde_json::from_str(&fixture_text(relative))
        .unwrap_or_else(|error| panic!("parse JSON fixture {relative}: {error}"))
}

pub fn fixture_as<T: DeserializeOwned>(relative: &str) -> T {
    serde_json::from_value(fixture_json(relative))
        .unwrap_or_else(|error| panic!("decode fixture {relative}: {error}"))
}

pub fn platform_fixture_path(path: &str) -> PathBuf {
    assert!(
        path.starts_with('/'),
        "fixture path must start at its synthetic root"
    );
    assert!(
        path.split('/')
            .all(|component| component != "." && component != ".."),
        "fixture path must not contain traversal"
    );
    #[cfg(windows)]
    {
        let mut native = PathBuf::from(r"C:\neomax-fixtures");
        for component in path.trim_start_matches('/').split('/') {
            if !component.is_empty() {
                native.push(component);
            }
        }
        native
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(path)
    }
}

pub fn fixture_text_with_platform_paths(relative: &str) -> String {
    let mut value = fixture_json(relative);
    rewrite_platform_paths(&mut value);
    serde_json::to_string_pretty(&value)
        .unwrap_or_else(|error| panic!("encode platform fixture {relative}: {error}"))
}

fn rewrite_platform_paths(value: &mut Value) {
    match value {
        Value::String(path) if path.starts_with('/') => {
            *path = platform_fixture_path(path).to_string_lossy().into_owned();
        }
        Value::Array(values) => {
            for value in values {
                rewrite_platform_paths(value);
            }
        }
        Value::Object(values) => {
            let original = std::mem::take(values);
            for (key, mut value) in original {
                rewrite_platform_paths(&mut value);
                let key = if key.starts_with('/') {
                    platform_fixture_path(&key).to_string_lossy().into_owned()
                } else {
                    key
                };
                values.insert(key, value);
            }
        }
        _ => {}
    }
}

pub fn assert_json_roundtrip<T>(value: &T, expected: &Value)
where
    T: serde::Serialize,
{
    let actual = serde_json::to_value(value).expect("serialize compatibility value");
    assert_eq!(&actual, expected);
}

pub fn assert_fixture_is_sanitized(relative: &str) {
    let text = fixture_text(relative);
    for forbidden in ["/Users/", "/home/", "private.example", "user@example"] {
        assert!(
            !text.contains(forbidden),
            "fixture {relative} contains forbidden personal value {forbidden}"
        );
    }
}

#[test]
fn every_compatibility_fixture_is_sanitized() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let files = fixture_files(&root);
    assert!(
        !files.is_empty(),
        "compatibility fixture directory is empty"
    );
    for path in files {
        let text = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("read compatibility fixture {}: {error}", path.display())
        });
        let relative = path.strip_prefix(&root).unwrap_or(&path).to_string_lossy();
        for forbidden in ["/Users/", "/home/", "private.example", "user@example"] {
            assert!(
                !text.contains(forbidden),
                "fixture {relative} contains forbidden personal value {forbidden}"
            );
        }
        for forbidden in ["accessToken", "api_key", "private_key", "Bearer "] {
            assert!(
                !text.contains(forbidden),
                "fixture {relative} contains credential-like marker {forbidden}"
            );
        }
    }
}

fn fixture_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(fixture_files(&path));
        } else if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files
}
