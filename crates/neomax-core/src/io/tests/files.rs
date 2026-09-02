use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::time::Duration;

use super::super::{
    BoundedIoError, FileMetadata, FileSource, LocalFileSource, ReadLimits, hash_file, read_file,
    read_file_range,
};

#[test]
fn file_reads_are_bounded_and_ranges_are_exact() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("payload.bin");
    fs::write(&path, b"0123456789").unwrap();
    let limits = ReadLimits::new(32, Duration::from_secs(1)).unwrap();
    assert_eq!(
        read_file(&LocalFileSource, &path, limits).unwrap(),
        b"0123456789"
    );
    assert_eq!(
        read_file_range(&LocalFileSource, &path, 3, 4, limits).unwrap(),
        b"3456"
    );
    assert_eq!(
        hash_file(&LocalFileSource, &path, limits).unwrap(),
        "84d89877f0d4041efb6bf91a16f0248f2fd573e6af05c19f96bedb9f882f7882"
    );
}

#[test]
fn missing_and_oversized_files_are_distinguished() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let not_found = read_file(
        &LocalFileSource,
        &missing,
        ReadLimits::new(8, Duration::from_secs(1)).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(not_found, BoundedIoError::NotFound { .. }));

    let path = temp.path().join("large");
    fs::write(&path, b"0123456789").unwrap();
    let oversized = read_file(
        &LocalFileSource,
        &path,
        ReadLimits::new(4, Duration::from_secs(1)).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(
        oversized,
        BoundedIoError::Truncated { limit: 4, .. }
    ));
}

struct ShortSource;

impl FileSource for ShortSource {
    fn metadata(&self, _path: &Path) -> super::super::Result<FileMetadata> {
        Ok(FileMetadata {
            len: 8,
            regular: true,
        })
    }

    fn open(&self, _path: &Path) -> super::super::Result<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(Cursor::new(b"short".to_vec())))
    }

    fn open_seekable(&self, _path: &Path) -> super::super::Result<Box<dyn super::super::ReadSeek>> {
        Ok(Box::new(Cursor::new(b"short".to_vec())))
    }
}

#[test]
fn short_file_is_corruption_not_silent_success() {
    let error = read_file(
        &ShortSource,
        Path::new("fixture"),
        ReadLimits::new(32, Duration::from_secs(1)).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(error, BoundedIoError::Corrupt { .. }));
}

#[test]
fn directories_are_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let error = read_file(
        &LocalFileSource,
        temp.path(),
        ReadLimits::new(32, Duration::from_secs(1)).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(error, BoundedIoError::Corrupt { .. }));
}

#[cfg(unix)]
#[test]
fn local_file_source_does_not_follow_symlink_swaps() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let link = temp.path().join("link");
    fs::write(&target, b"secret").unwrap();
    symlink(&target, &link).unwrap();

    assert!(LocalFileSource.open(&link).is_err());
    assert!(LocalFileSource.open_seekable(&link).is_err());
}

#[cfg(windows)]
#[test]
fn local_file_source_does_not_follow_reparse_files() {
    use std::os::windows::fs::symlink_file;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    let link = temp.path().join("link");
    fs::write(&target, b"secret").unwrap();
    if symlink_file(&target, &link).is_err() {
        return;
    }

    assert!(LocalFileSource.open(&link).is_err());
    assert!(LocalFileSource.open_seekable(&link).is_err());
}
