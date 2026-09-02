use crate::{Engine, Error, Result};

const DEFAULT_PRIORITY: [Engine; 5] = [
    Engine::Claude,
    Engine::Codex,
    Engine::Kimi,
    Engine::Grok,
    Engine::Opencode,
];

pub fn engine_priority(raw: Option<&str>) -> Result<Vec<Engine>> {
    let mut engines = Vec::new();
    if let Some(raw) = raw.filter(|value| !value.trim().is_empty()) {
        for item in raw.split([',', '+']) {
            let value = item.trim();
            if value.is_empty() {
                continue;
            }
            let engine = value.parse::<Engine>().map_err(|_| {
                Error::InvalidArgument(format!("invalid engine priority entry {value}"))
            })?;
            if !engines.contains(&engine) {
                engines.push(engine);
            }
        }
    }
    for engine in DEFAULT_PRIORITY {
        if !engines.contains(&engine) {
            engines.push(engine);
        }
    }
    Ok(engines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_deduplicates_and_completes_priority_order() {
        let order = engine_priority(Some("opencode+codex,opencode")).unwrap();
        assert_eq!(order[0], Engine::Opencode);
        assert_eq!(order[1], Engine::Codex);
        assert_eq!(order.len(), Engine::ALL.len());
        assert!(engine_priority(Some("invalid")).is_err());
    }
}
