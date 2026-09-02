use anyhow::{Result, bail};

use crate::http::HttpRequest;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Index,
    Asset {
        name: &'static str,
    },
    Status,
    History {
        limit: usize,
    },
    Modes,
    Usage {
        days: u32,
    },
    Sessions {
        days: u32,
    },
    Subagents {
        days: u32,
    },
    RunDiff {
        id: String,
    },
    Log {
        id: String,
    },
    Projects,
    Plans,
    Issues,
    Worktrees,
    Tasks,
    Queue,
    PrState,
    Connect {
        engine: String,
        account: String,
    },
    Pause {
        engine: String,
        account: String,
        paused: bool,
    },
    RunAction {
        action: String,
        id: String,
    },
    Action,
    NotFound,
}

const SESSION_DEFAULT_DAYS: u32 = 3;

pub fn route(request: &HttpRequest, default_days: u32) -> Result<Route> {
    let days = query_u32(request, "days")?.unwrap_or({
        if matches!(request.path.as_str(), "/api/sessions" | "/api/subagents") {
            SESSION_DEFAULT_DAYS
        } else {
            default_days
        }
    });
    match request.path.as_str() {
        "/" | "/index.html" => Ok(Route::Index),
        "/styles.css" => Ok(Route::Asset { name: "styles.css" }),
        "/app.js" => Ok(Route::Asset { name: "app.js" }),
        "/api.js" => Ok(Route::Asset { name: "api.js" }),
        "/render.js" => Ok(Route::Asset { name: "render.js" }),
        "/format.js" => Ok(Route::Asset { name: "format.js" }),
        "/delivery.js" => Ok(Route::Asset {
            name: "delivery.js",
        }),
        "/api/status" => Ok(Route::Status),
        "/api/history" => Ok(Route::History {
            limit: query_usize(request, "limit")?
                .unwrap_or(60)
                .clamp(1, 10_000),
        }),
        "/api/modes" => Ok(Route::Modes),
        "/api/usage" => Ok(Route::Usage { days }),
        "/api/sessions" => Ok(Route::Sessions {
            days: days.min(3660),
        }),
        "/api/subagents" => Ok(Route::Subagents {
            days: days.min(3660),
        }),
        "/api/projects" => Ok(Route::Projects),
        "/api/plans" => Ok(Route::Plans),
        "/api/issues" => Ok(Route::Issues),
        "/api/worktrees" => Ok(Route::Worktrees),
        "/api/tasks" => Ok(Route::Tasks),
        "/api/queue" => Ok(Route::Queue),
        "/api/prstate" => Ok(Route::PrState),
        "/api/action" => Ok(Route::Action),
        path if path.starts_with("/api/connect/") => {
            let (engine, account) = two_segments(path, "/api/connect/")?;
            Ok(Route::Connect { engine, account })
        }
        path if path.starts_with("/api/pause/") => {
            let (engine, account) = two_segments(path, "/api/pause/")?;
            Ok(Route::Pause {
                engine,
                account,
                paused: true,
            })
        }
        path if path.starts_with("/api/unpause/") => {
            let (engine, account) = two_segments(path, "/api/unpause/")?;
            Ok(Route::Pause {
                engine,
                account,
                paused: false,
            })
        }
        path if path.starts_with("/api/act/") => {
            let (action, id) = two_segments(path, "/api/act/")?;
            Ok(Route::RunAction { action, id })
        }
        path if path.starts_with("/api/rundiff/") => Ok(Route::RunDiff {
            id: path["/api/rundiff/".len()..].to_string(),
        }),
        path if path.starts_with("/api/log/") => Ok(Route::Log {
            id: path["/api/log/".len()..].to_string(),
        }),
        _ => Ok(Route::NotFound),
    }
}

fn two_segments(path: &str, prefix: &str) -> Result<(String, String)> {
    let values = path[prefix.len()..].split('/').collect::<Vec<_>>();
    if values.len() != 2 || values.iter().any(|value| value.is_empty()) {
        bail!("invalid local action path")
    }
    Ok((values[0].to_owned(), values[1].to_owned()))
}

fn query_u32(request: &HttpRequest, key: &str) -> Result<Option<u32>> {
    request
        .query
        .get(key)
        .map(|value| value.parse::<u32>().map_err(Into::into))
        .transpose()
}

fn query_usize(request: &HttpRequest, key: &str) -> Result<Option<usize>> {
    request
        .query
        .get(key)
        .map(|value| value.parse::<usize>().map_err(Into::into))
        .transpose()
}

pub fn validate_days(days: u32) -> Result<u32> {
    if days > 3660 {
        bail!("days must be between 0 and 3660");
    }
    Ok(days)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn request(path: &str) -> HttpRequest {
        HttpRequest {
            method: "GET".into(),
            target: path.into(),
            path: path.split('?').next().unwrap().into(),
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            body: Vec::new(),
        }
    }

    #[test]
    fn maps_every_read_only_reference_endpoint() {
        assert_eq!(route(&request("/"), 30).unwrap(), Route::Index);
        assert_eq!(
            route(&request("/styles.css"), 30).unwrap(),
            Route::Asset { name: "styles.css" }
        );
        assert_eq!(route(&request("/api/status"), 30).unwrap(), Route::Status);
        assert_eq!(route(&request("/api/plans"), 30).unwrap(), Route::Plans);
        assert_eq!(route(&request("/api/issues"), 30).unwrap(), Route::Issues);
        assert_eq!(
            route(&request("/api/worktrees"), 30).unwrap(),
            Route::Worktrees
        );
        assert_eq!(route(&request("/api/prstate"), 30).unwrap(), Route::PrState);
        assert_eq!(
            route(&request("/api/connect/kimi/2"), 30).unwrap(),
            Route::Connect {
                engine: "kimi".into(),
                account: "2".into()
            }
        );
        assert_eq!(
            route(&request("/api/act/kill/run-1"), 30).unwrap(),
            Route::RunAction {
                action: "kill".into(),
                id: "run-1".into()
            }
        );
        assert_eq!(
            route(&request("/api/usage"), 7).unwrap(),
            Route::Usage { days: 7 }
        );
        assert_eq!(
            route(&request("/api/sessions"), 30).unwrap(),
            Route::Sessions { days: 3 }
        );
        assert_eq!(
            route(&request("/api/subagents"), 30).unwrap(),
            Route::Subagents { days: 3 }
        );
        assert_eq!(
            route(&request("/api/log/run-1"), 30).unwrap(),
            Route::Log { id: "run-1".into() }
        );
    }

    #[test]
    fn session_endpoints_honor_an_explicit_window() {
        let mut req = request("/api/sessions");
        req.query.insert("days".into(), "14".into());
        assert_eq!(route(&req, 30).unwrap(), Route::Sessions { days: 14 });

        let mut req = request("/api/subagents");
        req.query.insert("days".into(), "0".into());
        assert_eq!(route(&req, 30).unwrap(), Route::Subagents { days: 0 });
    }

    #[test]
    fn caps_history_and_rejects_invalid_query_numbers() {
        let mut req = request("/api/history");
        req.query.insert("limit".into(), "999999".into());
        assert_eq!(route(&req, 30).unwrap(), Route::History { limit: 10_000 });
        req.query.insert("limit".into(), "bad".into());
        assert!(route(&req, 30).is_err());
    }
}
