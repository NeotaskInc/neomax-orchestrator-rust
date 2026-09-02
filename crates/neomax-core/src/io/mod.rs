mod clock;
mod error;
pub(crate) mod event_partition;
mod files;
mod permissions;
mod process;
pub mod process_group;
mod reader;
pub mod text;
mod windows_paths;

pub use clock::{Clock, SystemClock};
pub use error::{BoundedIoError, Result};
pub use files::{
    FileMetadata, FileSource, LocalFileSource, ReadSeek, hash_file, hash_file_with_clock,
    read_file, read_file_range, read_file_range_with_clock, read_file_with_clock,
};
pub(crate) use files::{open_regular_no_follow, reject_reparse_components};
pub use windows_paths::{PathGuard, is_rooted_but_not_absolute};
pub use permissions::{
    enforce_private_path, ensure_private_directory, set_private_directory, set_private_open_path,
    set_private_path, verify_private_path,
};
pub use process::{
    DEFAULT_PROCESS_TIMEOUT, DEFAULT_TERMINATE_GRACE, LocalProcessRunner, ProcessOutput,
    ProcessRequest, ProcessRunner,
};
pub use process_group::{
    ChildContainment, DEFAULT_DETACHED_TERMINATE_GRACE, DetachedProcessControl, ProcessControl,
    SystemDetachedProcessControl, SystemProcessControl, spawn_detached, spawn_managed,
    terminate_detached, terminate_detached_with, terminate_supervisor, terminate_worker,
};
pub use reader::{
    ReadLimits, hash_reader, hash_reader_with_clock, read_lines, read_lines_with_clock,
    read_reader, read_reader_with_clock,
};
pub use text::{os_str_to_utf8, path_to_string, path_to_utf8};

#[cfg(test)]
mod tests;
