use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use crate::atomic::with_exclusive_lock;
use crate::{Error, Result};

use super::super::fingerprint::issue_fingerprint;
use super::super::store::IssueStore;
use super::super::types::{Issue, IssueEvent, IssueMirror, IssueStatus, MirrorState};
use super::driver::MirrorDriver;
use super::types::{CrossRepoIssueInput, MirrorRequest, RepositoryCatalog, RepositoryTarget};

pub struct CrossRepoIssueCoordinator<'a, C, D> {
    pub(super) store: &'a IssueStore,
    pub(super) catalog: &'a C,
    pub(super) driver: &'a D,
    pub(super) issue_label: String,
}

impl<'a, C, D> CrossRepoIssueCoordinator<'a, C, D>
where
    C: RepositoryCatalog,
    D: MirrorDriver,
{
    pub fn new(store: &'a IssueStore, catalog: &'a C, driver: &'a D) -> Self {
        Self {
            store,
            catalog,
            driver,
            issue_label: "neomax-issue".into(),
        }
    }

    pub fn with_issue_label(mut self, label: impl Into<String>) -> Self {
        self.issue_label = label.into();
        self
    }

    pub fn open(&self, input: CrossRepoIssueInput) -> Result<Issue> {
        let all_targets = self.catalog.repositories(&input.project)?;
        let targets = select_targets(&all_targets, input.repositories.as_deref())?;
        let target_names = targets
            .iter()
            .map(|target| target.name.clone())
            .collect::<Vec<_>>();
        let fingerprint = input.fingerprint.clone().unwrap_or_else(|| {
            issue_fingerprint(&input.title, Some(&input.project), &target_names)
        });
        let key = input
            .key
            .clone()
            .unwrap_or_else(|| generated_key(input.now));
        validate_issue_key(&key)?;
        let labels = normalize_labels(&self.issue_label, &input.labels);
        let request = MirrorRequest {
            key: key.clone(),
            title: input.title.clone(),
            body: input.body.clone(),
            project: input.project.clone(),
            labels: labels.clone(),
        };
        let lock = self.store.directory().join(".open.lock");
        let issue = with_exclusive_lock(&lock, || {
            if !input.force_new {
                if let Some(mut duplicate) = self
                    .store
                    .find_open_duplicate(&fingerprint, Some(&input.project))?
                {
                    duplicate.new_record = false;
                    return Ok(duplicate);
                }
            }
            let mut issue = Issue::new(&key, &input.title, &input.project, input.now);
            issue.body = input.body.clone();
            issue.severity = input.severity.clone();
            issue.labels = labels.clone();
            issue.fingerprint = Some(fingerprint.clone());
            for target in &targets {
                let mirror = self
                    .driver
                    .create(target, &request)
                    .unwrap_or_else(|_| IssueMirror::local());
                issue.repos.insert(target.name.clone(), mirror);
            }
            issue.history.push(IssueEvent {
                ts: input.now,
                event: "opened".into(),
                extra: BTreeMap::from([(
                    "repos".into(),
                    serde_json::Value::Array(
                        target_names
                            .iter()
                            .cloned()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                )]),
            });
            self.store.save_at(&mut issue, input.now)?;
            issue.new_record = true;
            Ok(issue)
        })?;
        if issue.new_record {
            notify_siblings(self, &all_targets, &issue);
        }
        Ok(issue)
    }

    pub fn comment_all(&self, issue: &Issue, text: &str) -> Result<usize> {
        let targets = self.catalog.repositories(&issue.project)?;
        let by_name = targets
            .into_iter()
            .map(|target| (target.name.clone(), target))
            .collect::<BTreeMap<_, _>>();
        let mut posted = 0;
        for (name, mirror) in &issue.repos {
            let Some(target) = by_name.get(name) else {
                continue;
            };
            if (mirror.number.is_some() || mirror.url.is_some())
                && self.driver.comment(target, mirror, text).is_ok()
            {
                posted += 1;
            }
        }
        Ok(posted)
    }

    pub fn close(&self, key: &str, comment: Option<&str>, now: i64) -> Result<Option<Issue>> {
        let Some(issue) = self.store.load(key)? else {
            return Ok(None);
        };
        let targets = self.catalog.repositories(&issue.project)?;
        let by_name = targets
            .into_iter()
            .map(|target| (target.name.clone(), target))
            .collect::<BTreeMap<_, _>>();
        for (name, mirror) in &issue.repos {
            let Some(target) = by_name.get(name) else {
                continue;
            };
            if mirror.number.is_some() || mirror.url.is_some() {
                let _ = self.driver.close(target, mirror, comment);
            }
        }
        self.store.update_at(key, now, |issue| {
            for mirror in issue.repos.values_mut() {
                if mirror.number.is_some() || mirror.url.is_some() {
                    mirror.state = MirrorState::Closed;
                }
            }
            issue.transition(IssueStatus::Done, now)?;
            issue.history.push(IssueEvent {
                ts: now,
                event: "closed".into(),
                extra: BTreeMap::new(),
            });
            Ok(())
        })
    }

    pub fn reconcile(&self, project: Option<&str>, now: i64) -> Result<usize> {
        let issues = self.store.list(project, None)?;
        let mut changed = 0;
        for issue in issues {
            if issue.status.is_terminal() {
                continue;
            }
            let targets = self.catalog.repositories(&issue.project)?;
            let by_name = targets
                .into_iter()
                .map(|target| (target.name.clone(), target))
                .collect::<BTreeMap<_, _>>();
            let mut states = BTreeMap::new();
            for (name, mirror) in &issue.repos {
                let Some(target) = by_name.get(name) else {
                    continue;
                };
                let Some(state) = self.driver.state(target, mirror)? else {
                    continue;
                };
                states.insert(name.clone(), state);
            }
            if states.is_empty() {
                continue;
            }
            let all_closed = states
                .values()
                .all(|state| matches!(state, MirrorState::Closed));
            let needs_update = states.iter().any(|(name, state)| {
                issue
                    .repos
                    .get(name)
                    .is_some_and(|mirror| mirror.state != *state)
            }) || all_closed && !issue.status.is_terminal();
            if !needs_update {
                continue;
            }
            self.store.update_at(&issue.key, now, |current| {
                for (name, state) in &states {
                    if let Some(mirror) = current.repos.get_mut(name) {
                        mirror.state = state.clone();
                    }
                }
                if all_closed {
                    current.transition(IssueStatus::Done, now)?;
                    current.history.push(IssueEvent {
                        ts: now,
                        event: "auto-closed (all mirrors closed)".into(),
                        extra: BTreeMap::new(),
                    });
                }
                Ok(())
            })?;
            changed += 1;
        }
        Ok(changed)
    }
}

fn notify_siblings<C, D>(
    coordinator: &CrossRepoIssueCoordinator<'_, C, D>,
    targets: &[RepositoryTarget],
    issue: &Issue,
) where
    C: RepositoryCatalog,
    D: MirrorDriver,
{
    let targets_by_name = targets
        .iter()
        .cloned()
        .map(|target| (target.name.clone(), target))
        .collect::<BTreeMap<_, _>>();
    let sibling_urls = issue
        .repos
        .iter()
        .filter_map(|(name, mirror)| mirror.url.as_ref().map(|url| (name.clone(), url.clone())))
        .collect::<Vec<_>>();
    if sibling_urls.len() < 2 {
        return;
    }
    for (name, mirror) in &issue.repos {
        if mirror.url.is_none() {
            continue;
        }
        let Some(target) = targets_by_name.get(name) else {
            continue;
        };
        let siblings = sibling_urls
            .iter()
            .filter(|(sibling, _)| sibling != name)
            .map(|(sibling, sibling_url)| format!("- {sibling}: {sibling_url}"))
            .collect::<Vec<_>>()
            .join("\n");
        let text = format!(
            "Synced cross-repo issue `{}`. Sibling issues:\n{}",
            issue.key, siblings
        );
        let _ = coordinator.driver.comment(target, mirror, &text);
    }
}

fn select_targets(
    all: &[RepositoryTarget],
    requested: Option<&[String]>,
) -> Result<Vec<RepositoryTarget>> {
    let available = all
        .iter()
        .map(|target| target.name.as_str())
        .collect::<BTreeSet<_>>();
    let names = requested.unwrap_or_default();
    if names.is_empty() {
        return Ok(all.to_vec());
    }
    let unknown = names
        .iter()
        .filter(|name| !available.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(Error::InvalidArgument(format!(
            "unknown repository name(s) {:?}; valid repositories: {}",
            unknown,
            available.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    let wanted = names.iter().collect::<BTreeSet<_>>();
    Ok(all
        .iter()
        .filter(|target| wanted.contains(&target.name))
        .cloned()
        .collect())
}

fn normalize_labels(required: &str, labels: &[String]) -> Vec<String> {
    let mut labels = labels
        .iter()
        .cloned()
        .chain([required.to_string()])
        .collect::<BTreeSet<_>>();
    labels.retain(|value| !value.trim().is_empty());
    labels.into_iter().collect()
}

fn generated_key(now: i64) -> String {
    format!("iss-{now}-{}", Uuid::new_v4().simple())
}

fn validate_issue_key(key: &str) -> Result<()> {
    if key.is_empty()
        || !key
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(Error::InvalidArgument(format!("invalid issue key {key:?}")));
    }
    Ok(())
}
