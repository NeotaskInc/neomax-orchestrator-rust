use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use neomax_core::io::is_rooted_but_not_absolute;
use neomax_core::orchestration::commands::Launcher;
use neomax_core::orchestration::rotation::{ArmedRotateStore, normalize_profile_path};
use serde_json::json;

use crate::context::RuntimeContext;
use crate::error;
use crate::output;

const SOLO_THRESHOLD: f64 = 99.0;

pub(crate) fn arm_profile(
    context: &RuntimeContext,
    profile: impl AsRef<std::path::Path>,
    session: Option<&str>,
) -> Result<()> {
    let store = ArmedRotateStore::in_state_dir(&context.paths.state);
    store.arm(
        profile.as_ref(),
        SOLO_THRESHOLD,
        SOLO_THRESHOLD,
        &[],
        true,
        context.now,
    )?;
    let _ = store.claim(profile, session, context.now)?;
    Ok(())
}
#[derive(Debug, Clone, PartialEq)]
struct SoloOptions {
    profile: Option<PathBuf>,
    threshold: f64,
    weekly_threshold: f64,
    prefer: Vec<String>,
    session: Option<String>,
    arm: bool,
    auto: bool,
    disarm: bool,
    claim: bool,
    json: bool,
}

impl Default for SoloOptions {
    fn default() -> Self {
        Self {
            profile: None,
            threshold: 99.0,
            weekly_threshold: 99.0,
            prefer: Vec::new(),
            session: None,
            arm: false,
            auto: false,
            disarm: false,
            claim: false,
            json: false,
        }
    }
}

pub(super) fn execute(
    _launcher: Launcher,
    args: &[String],
    context: &RuntimeContext,
) -> Result<()> {
    let options = error::usage(SoloOptions::parse(args))?;
    let selected_profile = options
        .profile
        .or_else(|| std::env::var_os("NEOMAX_PROFILE").map(PathBuf::from))
        .map(|profile| {
            if is_rooted_but_not_absolute(&profile) {
                return Err(anyhow::anyhow!(
                    "solo-rotate: profile path must not be rooted without an absolute prefix: {}",
                    profile.display()
                ));
            }
            Ok(normalize_profile_path(profile))
        })
        .transpose()?;
    let profile = selected_profile;
    let Some(profile) = profile else {
        if options.json {
            return output::json(&json!({
                "command": "solo-rotate",
                "status": "no-profile",
                "detail": "pass --profile or set NEOMAX_PROFILE to manage solo rotation state",
            }));
        }
        println!("solo-rotate: no profile selected (pass --profile or set NEOMAX_PROFILE)");
        return Ok(());
    };
    if options.disarm && (options.arm || options.claim) {
        return Err(error::usage_error(anyhow::anyhow!(
            "solo-rotate: --disarm cannot be combined with --arm or --claim"
        )));
    }
    let store = ArmedRotateStore::in_paths(&context.paths);
    let (action, record, claim) = if options.disarm {
        let removed = store.clear(&profile)?;
        ("disarm", json!(removed), serde_json::Value::Null)
    } else if options.claim {
        let claim = store.claim(&profile, options.session.as_deref(), context.now)?;
        ("claim", serde_json::Value::Null, json!(claim))
    } else if options.arm {
        let record = store.arm(
            &profile,
            options.threshold,
            options.weekly_threshold,
            &options.prefer,
            options.auto,
            context.now,
        )?;
        ("arm", json!(record), serde_json::Value::Null)
    } else {
        (
            "inspect",
            json!(store.get(&profile)),
            serde_json::Value::Null,
        )
    };
    let result = json!({
        "command": "solo-rotate",
        "action": action,
        "profile": profile,
        "record": record,
        "claim": claim,
    });
    if options.json {
        return output::json(&result);
    }
    match action {
        "arm" => println!("solo-rotate: armed {}", profile.display()),
        "disarm" => println!("solo-rotate: disarmed {}", profile.display()),
        "claim" => println!("solo-rotate: claimed {}", profile.display()),
        _ => println!("solo-rotate: inspected {}", profile.display()),
    }
    Ok(())
}

impl SoloOptions {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self::default();
        let mut index = 0;
        while index < args.len() {
            let current = &args[index];
            let (flag, inline) = current
                .split_once('=')
                .map_or((current.as_str(), None), |(name, value)| {
                    (name, Some(value))
                });
            match flag {
                "--json" => options.json = true,
                "--arm" => options.arm = true,
                "--auto" => options.auto = true,
                "--disarm" => options.disarm = true,
                "--claim" => options.claim = true,
                "--profile" => {
                    options.profile =
                        Some(PathBuf::from(option_value(args, &mut index, flag, inline)?));
                }
                "--session" | "--session-id" => {
                    options.session = Some(option_value(args, &mut index, flag, inline)?);
                }
                "--threshold" => {
                    options.threshold =
                        parse_percent(option_value(args, &mut index, flag, inline)?, flag)?;
                }
                "--weekly-threshold" => {
                    options.weekly_threshold =
                        parse_percent(option_value(args, &mut index, flag, inline)?, flag)?;
                }
                "--prefer" => {
                    let value = option_value(args, &mut index, flag, inline)?;
                    options.prefer.extend(
                        value
                            .split([',', '+'])
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned),
                    );
                }
                value if value.starts_with('-') => bail!("solo-rotate: unknown option {current}"),
                value => bail!("solo-rotate: unexpected argument {value}"),
            }
            index += 1;
        }
        if options.arm && options.claim {
            bail!("solo-rotate: --arm cannot be combined with --claim");
        }
        Ok(options)
    }
}

fn option_value(
    args: &[String],
    index: &mut usize,
    flag: &str,
    inline: Option<&str>,
) -> Result<String> {
    if let Some(value) = inline {
        if value.is_empty() {
            bail!("{flag} requires a value");
        }
        return Ok(value.to_owned());
    }
    let value = args
        .get(*index + 1)
        .with_context(|| format!("{flag} requires a value"))?;
    *index += 1;
    Ok(value.clone())
}

fn parse_percent(value: String, flag: &str) -> Result<f64> {
    let percent = value
        .parse::<f64>()
        .with_context(|| format!("{flag} requires a percentage"))?;
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        bail!("{flag} must be between 0 and 100");
    }
    Ok(percent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::fixture;

    #[test]
    fn arm_and_claim_use_only_local_rotation_state() {
        let fixture = fixture();
        let profile = fixture.context.paths.home.join("solo-profile");
        execute(
            Launcher::Universal,
            &[
                "--profile".into(),
                profile.display().to_string(),
                "--arm".into(),
                "--threshold".into(),
                "95".into(),
                "--json".into(),
            ],
            &fixture.context,
        )
        .unwrap();
        let store = ArmedRotateStore::in_paths(&fixture.context.paths);
        assert_eq!(store.get(&profile).unwrap().threshold, 95.0);
        execute(
            Launcher::Universal,
            &[
                "--profile".into(),
                profile.display().to_string(),
                "--claim".into(),
                "--session".into(),
                "session-1".into(),
                "--json".into(),
            ],
            &fixture.context,
        )
        .unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn rejects_windows_partial_root_profiles_before_normalization() {
        let fixture = fixture();
        for raw in [r"\rooted", r"C:drive-relative"] {
            let error = execute(
                Launcher::Universal,
                &["--profile".into(), raw.into(), "--arm".into()],
                &fixture.context,
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("rooted without an absolute prefix")
            );
        }
        assert!(!fixture.context.paths.armed_rotate.exists());
    }
}
