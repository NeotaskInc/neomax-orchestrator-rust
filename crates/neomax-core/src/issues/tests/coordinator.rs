use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::issues::{
    CrossRepoIssueCoordinator, CrossRepoIssueInput, IssueMirror, IssueStatus, MirrorDriver,
    MirrorState, RepositoryCatalog, RepositoryTarget,
};
use crate::Result;

struct Catalog(Vec<RepositoryTarget>);

impl RepositoryCatalog for Catalog {
    fn repositories(&self, _project: &str) -> Result<Vec<RepositoryTarget>> {
        Ok(self.0.clone())
    }
}

#[derive(Default)]
struct Driver {
    states: Mutex<BTreeMap<String, MirrorState>>,
    creates: Mutex<usize>,
}

impl MirrorDriver for Driver {
    fn create(
        &self,
        target: &RepositoryTarget,
        _request: &crate::issues::MirrorRequest,
    ) -> Result<IssueMirror> {
        *self.creates.lock().unwrap() += 1;
        let mirror = IssueMirror {
            number: Some(target.name.len().to_string()),
            url: Some(format!("https://example.test/{}/1", target.name)),
            state: MirrorState::Open,
            extra: BTreeMap::new(),
        };
        self.states
            .lock()
            .unwrap()
            .insert(target.name.clone(), MirrorState::Open);
        Ok(mirror)
    }

    fn comment(
        &self,
        _target: &RepositoryTarget,
        _mirror: &IssueMirror,
        _text: &str,
    ) -> Result<()> {
        Ok(())
    }

    fn close(
        &self,
        target: &RepositoryTarget,
        _mirror: &IssueMirror,
        _comment: Option<&str>,
    ) -> Result<()> {
        self.states
            .lock()
            .unwrap()
            .insert(target.name.clone(), MirrorState::Closed);
        Ok(())
    }

    fn state(
        &self,
        target: &RepositoryTarget,
        _mirror: &IssueMirror,
    ) -> Result<Option<MirrorState>> {
        Ok(self.states.lock().unwrap().get(&target.name).cloned())
    }
}

fn coordinator<'a>(
    store: &'a crate::issues::IssueStore,
    catalog: &'a Catalog,
    driver: &'a Driver,
) -> CrossRepoIssueCoordinator<'a, Catalog, Driver> {
    CrossRepoIssueCoordinator::new(store, catalog, driver)
}

#[test]
fn opens_dedupes_and_rejects_unknown_repository_names() {
    let temp = tempfile::tempdir().unwrap();
    let store = crate::issues::IssueStore::new(temp.path());
    let catalog = Catalog(vec![
        RepositoryTarget::new("api", "/tmp/api"),
        RepositoryTarget::new("site", "/tmp/site"),
    ]);
    let driver = Driver::default();
    let coordinator = coordinator(&store, &catalog, &driver);
    let mut input = CrossRepoIssueInput::new("Race in proxy worker pool", "body", "demo", 10);
    let first = coordinator.open(input.clone()).unwrap();
    assert!(first.new_record);
    input.title = "race  in PROXY worker pool!!".into();
    let duplicate = coordinator.open(input).unwrap();
    assert!(!duplicate.new_record);
    assert_eq!(first.key, duplicate.key);
    assert_eq!(*driver.creates.lock().unwrap(), 2);
    let mut bad = CrossRepoIssueInput::new("bad", "", "demo", 10);
    bad.repositories = Some(vec!["missing".into()]);
    assert!(coordinator.open(bad).is_err());
}

#[test]
fn close_and_reconcile_update_mirrors_and_status() {
    let temp = tempfile::tempdir().unwrap();
    let store = crate::issues::IssueStore::new(temp.path());
    let catalog = Catalog(vec![RepositoryTarget::new("api", "/tmp/api")]);
    let driver = Driver::default();
    let coordinator = coordinator(&store, &catalog, &driver);
    let issue = coordinator
        .open(CrossRepoIssueInput::new("drift", "body", "demo", 10))
        .unwrap();
    coordinator.close(&issue.key, Some("fixed"), 11).unwrap();
    assert_eq!(
        store.load(&issue.key).unwrap().unwrap().status,
        IssueStatus::Done
    );
    let mut reopen = store.load(&issue.key).unwrap().unwrap();
    reopen.transition(IssueStatus::Open, 12).unwrap();
    store.save_at(&mut reopen, 12).unwrap();
    driver
        .states
        .lock()
        .unwrap()
        .insert("api".into(), MirrorState::Closed);
    assert_eq!(coordinator.reconcile(Some("demo"), 13).unwrap(), 1);
    assert_eq!(
        store.load(&issue.key).unwrap().unwrap().status,
        IssueStatus::Done
    );
}

#[test]
fn issue_brief_is_self_contained() {
    let temp = tempfile::tempdir().unwrap();
    let store = crate::issues::IssueStore::new(temp.path());
    let catalog = Catalog(vec![RepositoryTarget::new("api", "/tmp/api")]);
    let driver = Driver::default();
    let coordinator = coordinator(&store, &catalog, &driver);
    let issue = coordinator
        .open(CrossRepoIssueInput::new("title", "body", "demo", 10))
        .unwrap();
    let brief = coordinator.issue_brief(&issue);
    assert!(brief.contains(&issue.key));
    assert!(brief.contains("api"));
}
