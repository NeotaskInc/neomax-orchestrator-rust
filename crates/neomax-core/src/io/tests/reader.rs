use std::io::{self, Read};
use std::time::Duration;

use super::super::read_lines;
use super::super::{BoundedIoError, ReadLimits, hash_reader, read_reader};

struct PartialReader {
    chunks: Vec<Vec<u8>>,
}

impl Read for PartialReader {
    fn read(&mut self, target: &mut [u8]) -> io::Result<usize> {
        let Some(mut chunk) = self.chunks.first_mut().map(std::mem::take) else {
            return Ok(0);
        };
        let count = chunk.len().min(target.len());
        target[..count].copy_from_slice(&chunk[..count]);
        if count < chunk.len() {
            chunk.drain(..count);
            self.chunks[0] = chunk;
        } else {
            self.chunks.remove(0);
        }
        Ok(count)
    }
}

struct SlowReader;

impl Read for SlowReader {
    fn read(&mut self, _target: &mut [u8]) -> io::Result<usize> {
        std::thread::sleep(Duration::from_millis(40));
        Ok(1)
    }
}

#[test]
fn partial_reads_are_reassembled_and_hashed() {
    let limits = ReadLimits::new(64, Duration::from_secs(1)).unwrap();
    let bytes = read_reader(
        PartialReader {
            chunks: vec![b"ne".to_vec(), b"om".to_vec(), b"ax".to_vec()],
        },
        limits,
    )
    .unwrap();
    assert_eq!(bytes, b"neomax");
    assert_eq!(
        hash_reader(
            PartialReader {
                chunks: vec![b"ne".to_vec(), b"om".to_vec(), b"ax".to_vec()],
            },
            limits,
        )
        .unwrap(),
        "41f59ce2fbf8c1a65aed992eaeb5c09fead1d381d87952dd91b31cd5926af620"
    );
}

#[test]
fn reader_rejects_output_over_limit() {
    let error = read_reader(
        io::Cursor::new(b"0123456789".to_vec()),
        ReadLimits::new(4, Duration::from_secs(1)).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(error, BoundedIoError::Truncated { limit: 4, .. }));
}

#[test]
fn slow_reader_is_timed_out() {
    let error = read_reader(
        SlowReader,
        ReadLimits::new(32, Duration::from_millis(5)).unwrap(),
    )
    .unwrap_err();
    assert!(matches!(error, BoundedIoError::Timeout { .. }));
}

#[test]
fn limits_reject_zero_timeout_and_zero_bytes() {
    assert!(ReadLimits::new(0, Duration::from_secs(1)).is_err());
    assert!(ReadLimits::new(1, Duration::ZERO).is_err());
}

#[test]
fn line_reader_streams_lines_and_strips_crlf() {
    let mut lines = Vec::new();
    let count = read_lines(
        io::Cursor::new(b"first\r\nsecond\nlast".to_vec()),
        ReadLimits::new(64, Duration::from_secs(1)).unwrap(),
        16,
        |line| {
            lines.push(line.to_vec());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(count, 3);
    assert_eq!(
        lines,
        vec![b"first".to_vec(), b"second".to_vec(), b"last".to_vec()]
    );
}

#[test]
fn line_reader_stops_before_an_oversized_line_can_grow_unbounded() {
    let error = read_lines(
        io::Cursor::new(b"123456789\nvalid\n".to_vec()),
        ReadLimits::new(64, Duration::from_secs(1)).unwrap(),
        8,
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(matches!(error, BoundedIoError::Truncated { limit: 8, .. }));
}
