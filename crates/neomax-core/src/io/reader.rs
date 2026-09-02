use std::io::{ErrorKind, Read};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::clock::{Clock, SystemClock};
use super::error::{BoundedIoError, Result};

const READ_CHUNK: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadLimits {
    pub max_bytes: usize,
    pub timeout: Duration,
}

impl ReadLimits {
    pub fn new(max_bytes: usize, timeout: Duration) -> Result<Self> {
        if max_bytes == 0 || max_bytes == usize::MAX {
            return Err(BoundedIoError::InvalidLimit(
                "max_bytes must be between 1 and usize::MAX - 1".into(),
            ));
        }
        if timeout.is_zero() {
            return Err(BoundedIoError::InvalidLimit(
                "timeout must be greater than zero".into(),
            ));
        }
        Ok(Self { max_bytes, timeout })
    }
}

/// Reads an interruptible or local regular-file reader within the limits.
///
/// The synchronous Read contract permits an implementation to block inside
/// one call, so the deadline is checked before and after each completed read.
/// Use a regular file, a nonblocking reader, or a reader with its own
/// cancellation mechanism when a hard wall-clock bound is required.
pub fn read_reader<R: Read>(reader: R, limits: ReadLimits) -> Result<Vec<u8>> {
    read_reader_with_clock(reader, limits, &SystemClock)
}

pub fn read_reader_with_clock<R: Read, C: Clock + ?Sized>(
    mut reader: R,
    limits: ReadLimits,
    clock: &C,
) -> Result<Vec<u8>> {
    let started = clock.now();
    let mut bytes = Vec::with_capacity(limits.max_bytes.min(READ_CHUNK));
    consume_reader(&mut reader, limits, clock, started, "read", |chunk| {
        bytes.extend_from_slice(chunk);
        Ok(())
    })?;
    Ok(bytes)
}

/// Feeds newline-delimited records to a callback without buffering the whole
/// source. The source and each individual line are bounded independently.
pub fn read_lines<R: Read, F>(
    reader: R,
    limits: ReadLimits,
    max_line_bytes: usize,
    consume_line: F,
) -> Result<usize>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    read_lines_with_clock(reader, limits, max_line_bytes, &SystemClock, consume_line)
}

pub fn read_lines_with_clock<R: Read, C: Clock + ?Sized, F>(
    mut reader: R,
    limits: ReadLimits,
    max_line_bytes: usize,
    clock: &C,
    mut consume_line: F,
) -> Result<usize>
where
    F: FnMut(&[u8]) -> Result<()>,
{
    if max_line_bytes == 0 {
        return Err(BoundedIoError::InvalidLimit(
            "max_line_bytes must be greater than zero".into(),
        ));
    }
    if max_line_bytes > limits.max_bytes {
        return Err(BoundedIoError::InvalidLimit(
            "max_line_bytes cannot exceed max_bytes".into(),
        ));
    }

    let started = clock.now();
    let mut pending = Vec::with_capacity(max_line_bytes.min(READ_CHUNK));
    let mut lines = 0usize;
    consume_reader(&mut reader, limits, clock, started, "read lines", |chunk| {
        let mut remaining = chunk;
        while let Some(newline) = remaining.iter().position(|byte| *byte == b'\n') {
            pending.extend_from_slice(&remaining[..newline]);
            if pending.len() > max_line_bytes {
                return Err(BoundedIoError::Truncated {
                    operation: "read line".into(),
                    limit: max_line_bytes,
                });
            }
            trim_line_ending(&mut pending);
            consume_line(&pending)?;
            lines = lines.saturating_add(1);
            pending.clear();
            remaining = &remaining[newline + 1..];
        }
        pending.extend_from_slice(remaining);
        if pending.len() > max_line_bytes {
            return Err(BoundedIoError::Truncated {
                operation: "read line".into(),
                limit: max_line_bytes,
            });
        }
        Ok(())
    })?;
    if !pending.is_empty() {
        trim_line_ending(&mut pending);
        consume_line(&pending)?;
        lines = lines.saturating_add(1);
    }
    Ok(lines)
}

fn trim_line_ending(line: &mut Vec<u8>) {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
}

/// Hashes an interruptible or local regular-file reader within the limits.
///
/// The timeout has the same completed-read scope as read_reader.
pub fn hash_reader<R: Read>(reader: R, limits: ReadLimits) -> Result<String> {
    hash_reader_with_clock(reader, limits, &SystemClock)
}

pub fn hash_reader_with_clock<R: Read, C: Clock + ?Sized>(
    mut reader: R,
    limits: ReadLimits,
    clock: &C,
) -> Result<String> {
    let started = clock.now();
    let mut digest = Sha256::new();
    consume_reader(&mut reader, limits, clock, started, "hash", |chunk| {
        digest.update(chunk);
        Ok(())
    })?;
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn consume_reader<R, C, F>(
    reader: &mut R,
    limits: ReadLimits,
    clock: &C,
    started: std::time::Instant,
    operation: &str,
    mut consume: F,
) -> Result<usize>
where
    R: Read + ?Sized,
    C: Clock + ?Sized,
    F: FnMut(&[u8]) -> Result<()>,
{
    ReadLimits::new(limits.max_bytes, limits.timeout)?;
    let mut total = 0usize;
    let mut buffer = vec![0u8; READ_CHUNK];
    loop {
        if clock.now().duration_since(started) >= limits.timeout {
            return Err(BoundedIoError::timeout(operation, limits.timeout));
        }
        let remaining = limits.max_bytes.saturating_sub(total);
        let request = remaining.saturating_add(1).min(READ_CHUNK);
        let read = match reader.read(&mut buffer[..request]) {
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(BoundedIoError::io(operation, error)),
        };
        if clock.now().duration_since(started) >= limits.timeout {
            return Err(BoundedIoError::timeout(operation, limits.timeout));
        }
        if read == 0 {
            return Ok(total);
        }
        if read > remaining {
            return Err(BoundedIoError::Truncated {
                operation: operation.into(),
                limit: limits.max_bytes,
            });
        }
        consume(&buffer[..read])?;
        total += read;
    }
}

pub(crate) fn read_exact_with_clock<R, C>(
    reader: &mut R,
    expected: usize,
    limits: ReadLimits,
    clock: &C,
    operation: &str,
) -> Result<Vec<u8>>
where
    R: Read + ?Sized,
    C: Clock + ?Sized,
{
    if expected > limits.max_bytes {
        return Err(BoundedIoError::Truncated {
            operation: operation.into(),
            limit: limits.max_bytes,
        });
    }
    let started = clock.now();
    let mut bytes = Vec::with_capacity(expected);
    let mut buffer = vec![0u8; READ_CHUNK.min(expected.max(1))];
    while bytes.len() < expected {
        if clock.now().duration_since(started) >= limits.timeout {
            return Err(BoundedIoError::timeout(operation, limits.timeout));
        }
        let remaining = expected - bytes.len();
        let request = remaining.min(buffer.len());
        let read = match reader.read(&mut buffer[..request]) {
            Ok(read) => read,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(error) => return Err(BoundedIoError::io(operation, error)),
        };
        if clock.now().duration_since(started) >= limits.timeout {
            return Err(BoundedIoError::timeout(operation, limits.timeout));
        }
        if read == 0 {
            return Err(BoundedIoError::Corrupt {
                path: "<stream>".into(),
                message: format!("expected {expected} bytes, received {}", bytes.len()),
            });
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
    Ok(bytes)
}
