mod runs;
mod sessions;
mod status;
mod usage;

pub use status::build_status;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FilesystemPortalSource;

    #[test]
    fn aggregate_modules_are_composed_through_the_status_entrypoint() {
        let temp = tempfile::tempdir().unwrap();
        let source = FilesystemPortalSource::new(temp.path(), temp.path().join("state"));
        let snapshot = build_status(&source, 1_800_000_000, 3).unwrap();
        assert_eq!(snapshot.now, 1_800_000_000);
        assert_eq!(snapshot.inbox, 0);
    }
}
