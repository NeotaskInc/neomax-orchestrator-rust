use anyhow::{Context, Result};
use serde::Serialize;

pub fn json<T: Serialize>(value: &T) -> Result<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).context("could not encode JSON output")?
    );
    Ok(())
}
