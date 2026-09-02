use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::Result;
use crate::io::{
    BoundedIoError, FileSource, LocalFileSource, ReadLimits, read_file, read_file_range,
};

const ATTEMPT_LOG_READ_MAX_BYTES: usize = 16 * 1024 * 1024;
const ATTEMPT_LOG_READ_TIMEOUT: Duration = Duration::from_secs(2);
const STDERR_TAIL_MAX_BYTES: usize = 1024 * 1024;

pub(super) struct AttemptLogFiles {
    pub stdout: File,
    pub stderr: File,
    pub log_path: PathBuf,
    pub stderr_path: PathBuf,
}

impl AttemptLogFiles {
    pub fn open(directory: &Path, run_id: &str, attempt: u32) -> Result<Self> {
        let _directory_guard = crate::io::PathGuard::ensure_directory(directory)?;
        let log_path = directory.join(format!("{run_id}.attempt{attempt}.jsonl"));
        let stderr_path = PathBuf::from(format!("{}.stderr", log_path.to_string_lossy()));
        Ok(Self {
            stdout: append_file(&log_path)?,
            stderr: append_file(&stderr_path)?,
            log_path,
            stderr_path,
        })
    }

    pub fn size(&self) -> u64 {
        self.stdout
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(0)
            .saturating_add(
                self.stderr
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or(0),
            )
    }

    pub fn output(&self) -> Result<Vec<u8>> {
        self.output_with_limits(
            ReadLimits::new(ATTEMPT_LOG_READ_MAX_BYTES, ATTEMPT_LOG_READ_TIMEOUT)
                .expect("attempt log read limits are valid"),
        )
    }

    fn output_with_limits(&self, limits: ReadLimits) -> Result<Vec<u8>> {
        Ok(read_file(&LocalFileSource, &self.log_path, limits)?)
    }

    pub fn stderr_tail(&self, maximum: u64) -> Result<String> {
        read_tail(&self.stderr_path, maximum)
    }

    pub fn sync(&self) -> Result<()> {
        self.stdout.sync_all()?;
        self.stderr.sync_all()?;
        Ok(())
    }
}

fn append_file(path: &Path) -> Result<File> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("refusing a reparse point or non-file path: {}", path.display()),
            )
            .into());
        }
        crate::io::reject_reparse_components(path)?;
    }
    crate::io::set_private_path(path)?;
    Ok(file)
}

fn read_tail(path: &Path, maximum: u64) -> Result<String> {
    let requested = usize::try_from(maximum)
        .map_err(|_| crate::Error::InvalidArgument("stderr tail size is too large".into()))?;
    if requested > STDERR_TAIL_MAX_BYTES {
        return Err(BoundedIoError::Truncated {
            operation: format!("read stderr tail {}", path.display()),
            limit: STDERR_TAIL_MAX_BYTES,
        }
        .into());
    }
    let length = LocalFileSource.metadata(path)?.len;
    let tail_length = requested.min(usize::try_from(length).unwrap_or(requested));
    if tail_length == 0 {
        return Ok(String::new());
    }
    let offset = length.saturating_sub(tail_length as u64);
    let bytes = read_file_range(
        &LocalFileSource,
        path,
        offset,
        tail_length,
        ReadLimits::new(STDERR_TAIL_MAX_BYTES, ATTEMPT_LOG_READ_TIMEOUT)
            .expect("stderr tail read limits are valid"),
    )?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn output_rejects_oversized_log_files() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("run.attempt1.jsonl");
        fs::write(&path, [0_u8; 8]).unwrap();
        let logs = AttemptLogFiles::open(temp.path(), "run", 1).unwrap();
        let limits = ReadLimits::new(4, Duration::from_secs(1)).unwrap();
        let error = logs.output_with_limits(limits).unwrap_err();
        assert!(
            matches!(error, crate::Error::Message(message) if message.contains("exceeded its 4-byte limit"))
        );
    }

    #[test]
    fn stderr_tail_handles_missing_and_invalid_utf8_as_bounded_text() {
        let temp = tempfile::tempdir().unwrap();
        let logs = AttemptLogFiles::open(temp.path(), "run", 1).unwrap();
        fs::write(&logs.stderr_path, [b'a', 0xff, b'b']).unwrap();
        assert_eq!(logs.stderr_tail(3).unwrap(), "a�b");
        fs::remove_file(&logs.stderr_path).unwrap();
        assert!(matches!(
            logs.stderr_tail(1),
            Err(crate::Error::NotFound(_))
        ));
    }

    #[test]
    fn stderr_tail_rejects_requests_above_the_fixed_cap() {
        let temp = tempfile::tempdir().unwrap();
        let logs = AttemptLogFiles::open(temp.path(), "run", 1).unwrap();
        let error = logs
            .stderr_tail((STDERR_TAIL_MAX_BYTES as u64) + 1)
            .unwrap_err();
        assert!(
            matches!(error, crate::Error::Message(message) if message.contains("exceeded its"))
        );
    }
}
