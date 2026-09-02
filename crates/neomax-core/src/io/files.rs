use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::clock::{Clock, SystemClock};
use super::error::{BoundedIoError, Result};
use super::reader::{ReadLimits, consume_reader, read_exact_with_clock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMetadata {
    pub len: u64,
    pub regular: bool,
}

pub trait ReadSeek: Read + Seek + Send {}

impl<T> ReadSeek for T where T: Read + Seek + Send {}

pub trait FileSource: Send + Sync {
    fn metadata(&self, path: &Path) -> Result<FileMetadata>;
    fn open(&self, path: &Path) -> Result<Box<dyn Read + Send>>;
    fn open_seekable(&self, path: &Path) -> Result<Box<dyn ReadSeek>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileSource;

impl FileSource for LocalFileSource {
    fn metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                BoundedIoError::NotFound {
                    path: path.to_path_buf(),
                }
            } else {
                BoundedIoError::io(format!("metadata {}", path.display()), error)
            }
        })?;
        Ok(FileMetadata {
            len: metadata.len(),
            regular: metadata.file_type().is_file(),
        })
    }

    fn open(&self, path: &Path) -> Result<Box<dyn Read + Send>> {
        open_regular_no_follow(path)
            .map(|file| Box::new(file) as Box<dyn Read + Send>)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    BoundedIoError::NotFound {
                        path: path.to_path_buf(),
                    }
                } else {
                    BoundedIoError::io(format!("open {}", path.display()), error)
                }
            })
    }

    fn open_seekable(&self, path: &Path) -> Result<Box<dyn ReadSeek>> {
        open_regular_no_follow(path)
            .map(|file| Box::new(file) as Box<dyn ReadSeek>)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    BoundedIoError::NotFound {
                        path: path.to_path_buf(),
                    }
                } else {
                    BoundedIoError::io(format!("open {}", path.display()), error)
                }
            })
    }
}

pub(crate) fn open_regular_no_follow(path: &Path) -> std::io::Result<File> {
    let _path_guard = crate::io::PathGuard::for_existing_parent(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NOFOLLOW);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        reject_reparse_components(path)?;
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
            ));
        }
        reject_reparse_components(path)?;
    }
    Ok(file)
}

#[cfg(windows)]
pub(crate) fn reject_reparse_components(path: &Path) -> std::io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

    let mut current = Some(path);
    while let Some(candidate) = current {
        match fs::symlink_metadata(candidate) {
            Ok(metadata)
                if metadata.file_type().is_symlink()
                    || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 =>
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("path contains a reparse point: {}", candidate.display()),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        current = candidate.parent().filter(|parent| *parent != candidate);
    }
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn reject_reparse_components(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn read_file<S: FileSource + ?Sized>(
    source: &S,
    path: &Path,
    limits: ReadLimits,
) -> Result<Vec<u8>> {
    read_file_with_clock(source, path, limits, &SystemClock)
}

pub fn read_file_with_clock<S: FileSource + ?Sized, C: Clock + ?Sized>(
    source: &S,
    path: &Path,
    limits: ReadLimits,
    clock: &C,
) -> Result<Vec<u8>> {
    let metadata = source.metadata(path)?;
    ensure_regular(path, metadata)?;
    let expected = usize::try_from(metadata.len).map_err(|_| BoundedIoError::Truncated {
        operation: format!("read {}", path.display()),
        limit: limits.max_bytes,
    })?;
    if expected > limits.max_bytes {
        return Err(BoundedIoError::Truncated {
            operation: format!("read {}", path.display()),
            limit: limits.max_bytes,
        });
    }
    let reader = source.open(path)?;
    let bytes = super::reader::read_reader_with_clock(reader, limits, clock)?;
    if bytes.len() != expected {
        return Err(BoundedIoError::Corrupt {
            path: path.to_path_buf(),
            message: format!("metadata reported {expected} bytes, read {}", bytes.len()),
        });
    }
    Ok(bytes)
}

pub fn read_file_range(
    source: &dyn FileSource,
    path: &Path,
    offset: u64,
    length: usize,
    limits: ReadLimits,
) -> Result<Vec<u8>> {
    read_file_range_with_clock(source, path, offset, length, limits, &SystemClock)
}

pub fn read_file_range_with_clock<C: Clock + ?Sized>(
    source: &dyn FileSource,
    path: &Path,
    offset: u64,
    length: usize,
    limits: ReadLimits,
    clock: &C,
) -> Result<Vec<u8>> {
    let metadata = source.metadata(path)?;
    ensure_regular(path, metadata)?;
    let end = offset
        .checked_add(length as u64)
        .ok_or_else(|| BoundedIoError::Corrupt {
            path: path.to_path_buf(),
            message: "requested range overflows the file address space".into(),
        })?;
    if end > metadata.len {
        return Err(BoundedIoError::Corrupt {
            path: path.to_path_buf(),
            message: format!(
                "requested range {offset}..{end} exceeds file length {}",
                metadata.len
            ),
        });
    }
    if length > limits.max_bytes {
        return Err(BoundedIoError::Truncated {
            operation: format!("read range {}", path.display()),
            limit: limits.max_bytes,
        });
    }
    let mut reader = source.open_seekable(path)?;
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| BoundedIoError::io(format!("seek {}", path.display()), error))?;
    let bytes = read_exact_with_clock(
        &mut *reader,
        length,
        limits,
        clock,
        &format!("read range {}", path.display()),
    )
    .map_err(|error| match error {
        BoundedIoError::Corrupt { message, .. } => BoundedIoError::Corrupt {
            path: path.to_path_buf(),
            message,
        },
        other => other,
    })?;
    Ok(bytes)
}

pub fn hash_file<S: FileSource + ?Sized>(
    source: &S,
    path: &Path,
    limits: ReadLimits,
) -> Result<String> {
    hash_file_with_clock(source, path, limits, &SystemClock)
}

pub fn hash_file_with_clock<S: FileSource + ?Sized, C: Clock + ?Sized>(
    source: &S,
    path: &Path,
    limits: ReadLimits,
    clock: &C,
) -> Result<String> {
    let metadata = source.metadata(path)?;
    ensure_regular(path, metadata)?;
    let expected = usize::try_from(metadata.len).map_err(|_| BoundedIoError::Truncated {
        operation: format!("hash {}", path.display()),
        limit: limits.max_bytes,
    })?;
    if expected > limits.max_bytes {
        return Err(BoundedIoError::Truncated {
            operation: format!("hash {}", path.display()),
            limit: limits.max_bytes,
        });
    }
    let mut reader = source.open(path)?;
    let started = clock.now();
    let mut digest = Sha256::new();
    let count = consume_reader(
        &mut *reader,
        limits,
        clock,
        started,
        &format!("hash {}", path.display()),
        |chunk| {
            digest.update(chunk);
            Ok(())
        },
    )?;
    if count != expected {
        return Err(BoundedIoError::Corrupt {
            path: path.to_path_buf(),
            message: format!("metadata reported {expected} bytes, hashed {count}"),
        });
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn ensure_regular(path: &Path, metadata: FileMetadata) -> Result<()> {
    if metadata.regular {
        return Ok(());
    }
    Err(BoundedIoError::Corrupt {
        path: PathBuf::from(path),
        message: "expected a regular file".into(),
    })
}
