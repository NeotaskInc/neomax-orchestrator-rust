use std::ffi::c_void;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::Result;

const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_SUSPENDED: u32 = 0x0000_0004;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: u32 = 9;
const ERROR_INVALID_PARAMETER: i32 = 87;
const ERROR_BAD_LENGTH: i32 = 24;
const MAX_SNAPSHOT_ATTEMPTS: usize = 3;
const PROCESS_TERMINATE: u32 = 0x0001;
const PROCESS_SET_QUOTA: u32 = 0x0100;
const SYNCHRONIZE: u32 = 0x0010_0000;
const THREAD_SUSPEND_RESUME: u32 = 0x0002;
const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
const TASKKILL_TIMEOUT: Duration = Duration::from_secs(5);
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 258;
const SYSTEM_DIRECTORY_BUFFER_LEN: usize = 32_768;
const INVALID_HANDLE_VALUE: Handle = -1isize as *mut c_void;

type Handle = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BasicLimitInformation {
    per_process_user_time: i64,
    per_job_user_time: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ExtendedLimitInformation {
    basic_limit_information: BasicLimitInformation,
    io_info: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(
        job: Handle,
        information_class: u32,
        information: *const c_void,
        information_length: u32,
    ) -> i32;
    fn OpenProcess(access: u32, inherit_handle: i32, process_id: u32) -> Handle;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
    fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry32) -> i32;
    fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry32) -> i32;
    fn OpenThread(access: u32, inherit_handle: i32, thread_id: u32) -> Handle;
    fn ResumeThread(thread: Handle) -> u32;
    fn CloseHandle(handle: Handle) -> i32;
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ThreadEntry32 {
    size: u32,
    usage: u32,
    thread_id: u32,
    owner_process_id: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}

#[derive(Debug)]
pub struct ChildContainment {
    job: Handle,
}

// SAFETY: a job handle is a process-wide kernel reference. Moving or sharing
// it does not expose Rust-owned memory, and the Windows job APIs support
// concurrent access to the same handle.
unsafe impl Send for ChildContainment {}
unsafe impl Sync for ChildContainment {}

impl Drop for ChildContainment {
    fn drop(&mut self) {
        if !self.job.is_null() {
            // SAFETY: the handle was returned by CreateJobObjectW and is owned here.
            unsafe {
                CloseHandle(self.job);
            }
        }
    }
}

pub(super) fn configure_detached(command: &mut Command) {
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
}

pub(super) fn spawn_managed(command: &mut Command) -> Result<(Child, ChildContainment)> {
    let job = create_job()?;
    command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_SUSPENDED);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            close_job(job);
            return Err(error.into());
        }
    };

    if let Err(error) = assign_process(job, child.id()) {
        abort_start(&mut child, job);
        return Err(error);
    }
    if let Err(error) = resume_suspended_process(child.id()) {
        abort_start(&mut child, job);
        return Err(error);
    }
    Ok((child, ChildContainment { job }))
}

pub(super) fn terminate(child: &mut Child, containment: &ChildContainment, grace: Duration) {
    terminate_job(containment);
    if wait_for_exit(child, grace).ok().flatten().is_some() {
        return;
    }
    terminate_job(containment);
    let _ = kill_root_if_running(child);
    let _ = wait_for_exit(child, grace);
}

pub(super) fn terminate_residual(containment: &ChildContainment, _grace: Duration) {
    terminate_job(containment);
}

pub(super) fn terminate_detached(child: &mut Child, grace: Duration) -> std::io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    match run_taskkill(child.id()) {
        Ok(status) if status.success() => {}
        Ok(_) | Err(_) => kill_root_if_running(child)?,
    }
    if wait_for_exit(child, grace)?.is_some() {
        return Ok(());
    }
    let _ = run_taskkill(child.id());
    kill_root_if_running(child)?;
    if wait_for_exit(child, grace)?.is_none() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "Windows process tree did not exit after forced termination",
        ));
    }
    Ok(())
}

pub(super) fn terminate_process_group(pid: u32) -> std::io::Result<()> {
    let Some(process) = open_process_identity(pid)? else {
        return Ok(());
    };
    let termination = run_taskkill(pid);
    match termination {
        Ok(status) if status.success() => Ok(()),
        result => {
            if process.has_exited()? {
                Ok(())
            } else {
                match result {
                    Err(error) => Err(error),
                    Ok(_) => Err(std::io::Error::other(format!(
                        "taskkill failed for process group {pid}"
                    ))),
                }
            }
        }
    }
}

struct ProcessIdentity(Handle);

impl ProcessIdentity {
    fn has_exited(&self) -> std::io::Result<bool> {
        // SAFETY: the handle is an owned process identity opened with SYNCHRONIZE access.
        match unsafe { WaitForSingleObject(self.0, 0) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err(std::io::Error::last_os_error()),
        }
    }
}

impl Drop for ProcessIdentity {
    fn drop(&mut self) {
        // SAFETY: this is the handle returned by OpenProcess and is owned here.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn open_process_identity(pid: u32) -> std::io::Result<Option<ProcessIdentity>> {
    // SAFETY: the process id is a validated worker or supervisor id.
    let process = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if !process.is_null() {
        return Ok(Some(ProcessIdentity(process)));
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER) {
        Ok(None)
    } else {
        Err(error)
    }
}

fn run_taskkill(pid: u32) -> std::io::Result<ExitStatus> {
    let executable = system_binary("taskkill.exe")?;
    let mut command = Command::new(executable);
    command
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    bounded_status(&mut command, TASKKILL_TIMEOUT)
}

fn bounded_status(command: &mut Command, timeout: Duration) -> std::io::Result<ExitStatus> {
    let mut child = command.spawn()?;
    if let Some(status) = wait_for_exit(&mut child, timeout)? {
        return Ok(status);
    }
    let _ = child.kill();
    let _ = wait_for_exit(&mut child, Duration::from_millis(250));
    Err(std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        "Windows system command exceeded its termination deadline",
    ))
}

fn kill_root_if_running(child: &mut Child) -> std::io::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }
    match child.kill() {
        Ok(()) => Ok(()),
        Err(_) if child.try_wait()?.is_some() => Ok(()),
        Err(error) => Err(error),
    }
}

fn system_binary(name: &str) -> std::io::Result<PathBuf> {
    if name.is_empty() || Path::new(name).components().count() != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Windows system executable name is invalid",
        ));
    }
    let system32 = system_directory()?;
    ensure_directory(&system32)?;
    let executable = system32.join(name);
    let metadata = fs::symlink_metadata(&executable)?;
    let linked = has_linked_component(&executable)?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || linked
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Windows system executable is not a trusted regular file",
        ));
    }
    Ok(executable)
}

fn system_directory() -> std::io::Result<PathBuf> {
    let mut buffer = [0u16; SYSTEM_DIRECTORY_BUFFER_LEN];
    // SAFETY: the buffer is writable UTF-16 storage and its bounded length is
    // provided to the Win32 API in the required element count.
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 {
        return Err(std::io::Error::last_os_error());
    }
    system_directory_from_utf16(&buffer, length)
}

fn system_directory_from_utf16(buffer: &[u16], length: u32) -> std::io::Result<PathBuf> {
    let length = usize::try_from(length).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Windows system directory length is invalid",
        )
    })?;
    if length == 0 || length >= buffer.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Windows system directory output exceeds its bounded buffer",
        ));
    }
    let units = &buffer[..length];
    if units.contains(&0) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Windows system directory contains an embedded NUL",
        ));
    }
    let text = String::from_utf16(units).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Windows system directory is not valid UTF-16",
        )
    })?;
    if text.chars().any(char::is_control) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Windows system directory contains control characters",
        ));
    }
    let path = PathBuf::from(text);
    if !is_drive_absolute_windows(&path) || has_unsafe_component(&path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Windows system directory must be an absolute normalized drive path",
        ));
    }
    Ok(path)
}

fn ensure_directory(path: &Path) -> std::io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    let linked = has_linked_component(path)?;
    if !metadata.file_type().is_dir()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || linked
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Windows system directory is not trusted",
        ));
    }
    Ok(())
}

fn is_drive_absolute_windows(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.as_bytes().get(1) == Some(&b':')
        && text
            .as_bytes()
            .get(2)
            .is_some_and(|separator| *separator == b'/' || *separator == b'\\')
}

fn has_unsafe_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & 0x400 != 0
}

fn has_linked_component(path: &Path) -> std::io::Result<bool> {
    for ancestor in path.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn create_job() -> Result<Handle> {
    // SAFETY: null security attributes and name request an unnamed job owned by this process.
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut limits = ExtendedLimitInformation {
        basic_limit_information: BasicLimitInformation {
            per_process_user_time: 0,
            per_job_user_time: 0,
            limit_flags: JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            minimum_working_set_size: 0,
            maximum_working_set_size: 0,
            active_process_limit: 0,
            affinity: 0,
            priority_class: 0,
            scheduling_class: 0,
        },
        io_info: IoCounters {
            read_operation_count: 0,
            write_operation_count: 0,
            other_operation_count: 0,
            read_transfer_count: 0,
            write_transfer_count: 0,
            other_transfer_count: 0,
        },
        process_memory_limit: 0,
        job_memory_limit: 0,
        peak_process_memory_used: 0,
        peak_job_memory_used: 0,
    };
    // SAFETY: limits is a correctly laid out JOB_OBJECT_EXTENDED_LIMIT_INFORMATION value.
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            (&mut limits as *mut ExtendedLimitInformation).cast(),
            std::mem::size_of::<ExtendedLimitInformation>() as u32,
        )
    } != 0;
    if !configured {
        close_job(job);
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(job)
}

fn assign_process(job: Handle, pid: u32) -> Result<()> {
    let process = {
        // SAFETY: OpenProcess receives a process id returned by Child::id.
        unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SET_QUOTA, 0, pid) }
    };
    if process.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: both handles are valid and owned for this operation.
    let assigned = unsafe { AssignProcessToJobObject(job, process) } != 0;
    // SAFETY: process is the handle returned by OpenProcess and is not retained.
    unsafe {
        CloseHandle(process);
    }
    if assigned {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

fn resume_suspended_process(pid: u32) -> Result<()> {
    let snapshot = thread_snapshot()?;
    let mut entry = ThreadEntry32 {
        size: std::mem::size_of::<ThreadEntry32>() as u32,
        usage: 0,
        thread_id: 0,
        owner_process_id: 0,
        base_priority: 0,
        delta_priority: 0,
        flags: 0,
    };
    let mut thread = None;
    let mut found = unsafe { Thread32First(snapshot, &mut entry) } != 0;
    while found {
        if entry.owner_process_id == pid {
            let handle = {
                // SAFETY: the thread id came from the system-owned snapshot.
                unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.thread_id) }
            };
            if handle.is_null() {
                let error = std::io::Error::last_os_error();
                // SAFETY: snapshot is a valid handle until closed below.
                unsafe {
                    CloseHandle(snapshot);
                }
                return Err(error.into());
            }
            thread = Some(handle);
            break;
        }
        found = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
    }
    // SAFETY: snapshot is a valid handle returned by CreateToolhelp32Snapshot.
    unsafe {
        CloseHandle(snapshot);
    }
    let thread =
        thread.ok_or_else(|| std::io::Error::other("suspended process thread not found"))?;
    // SAFETY: thread is a valid handle with THREAD_SUSPEND_RESUME access.
    let resumed = unsafe { ResumeThread(thread) } != u32::MAX;
    // SAFETY: thread is the handle returned by OpenThread and is not retained.
    unsafe {
        CloseHandle(thread);
    }
    if resumed {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error().into())
    }
}

fn thread_snapshot() -> std::io::Result<Handle> {
    for attempt in 0..MAX_SNAPSHOT_ATTEMPTS {
        let snapshot = {
            // SAFETY: the snapshot has no Rust-owned memory and only requests thread metadata.
            unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
        };
        if snapshot != INVALID_HANDLE_VALUE {
            return Ok(snapshot);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(ERROR_BAD_LENGTH) && attempt + 1 < MAX_SNAPSHOT_ATTEMPTS {
            thread::yield_now();
            continue;
        }
        return Err(error);
    }
    unreachable!("bounded Toolhelp snapshot retry loop must return")
}

fn abort_start(child: &mut Child, job: Handle) {
    // SAFETY: job is owned by this start transaction and remains open until cleanup finishes.
    unsafe {
        let _ = TerminateJobObject(job, 1);
    }
    let _ = child.kill();
    let _ = child.wait();
    close_job(job);
}

fn close_job(job: Handle) {
    if !job.is_null() {
        // SAFETY: callers only pass handles created by CreateJobObjectW.
        unsafe {
            CloseHandle(job);
        }
    }
}

fn terminate_job(containment: &ChildContainment) {
    // SAFETY: the job remains owned by the containment while termination runs.
    unsafe {
        let _ = TerminateJobObject(containment.job, 1);
    }
}

fn wait_for_exit(child: &mut Child, grace: Duration) -> std::io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(20).min(deadline.duration_since(now)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Stdio;

    const FIXTURE_ENV: &str = "NEOMAX_PROCESS_GROUP_FIXTURE";
    const FIXTURE_PARENT: &str = "parent";
    const FIXTURE_DESCENDANT: &str = "descendant";
    const FIXTURE_DETACHED: &str = "detached";
    const FIXTURE_EXITED: &str = "exited";

    #[test]
    fn managed_job_terminates_a_descendant() {
        run_descendant_termination_check("managed");
    }

    #[test]
    fn early_spawn_is_contained_before_resume() {
        run_descendant_termination_check("early");
    }

    #[test]
    fn detached_termination_is_bounded() {
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some(FIXTURE_DETACHED) {
            thread::sleep(Duration::from_secs(30));
            return;
        }
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "io::process_group::windows::tests::detached_termination_is_bounded",
                "--nocapture",
            ])
            .env(FIXTURE_ENV, FIXTURE_DETACHED)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_detached(&mut command);
        let mut child = command.spawn().expect("spawn detached fixture");
        let started = Instant::now();
        terminate_detached(&mut child, Duration::from_millis(100))
            .expect("terminate detached fixture");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(child.try_wait().expect("inspect detached fixture").is_some());
    }

    #[test]
    fn system_command_status_is_bounded() {
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some("system-command") {
            thread::sleep(Duration::from_secs(30));
            return;
        }
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "io::process_group::windows::tests::system_command_status_is_bounded",
                "--nocapture",
            ])
            .env(FIXTURE_ENV, "system-command")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let started = Instant::now();
        let error = bounded_status(&mut command, Duration::from_millis(100))
            .expect_err("hanging system command must time out");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn pid_termination_is_idempotent_after_exit() {
        if std::env::var(FIXTURE_ENV).ok().as_deref() == Some(FIXTURE_EXITED) {
            return;
        }
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "io::process_group::windows::tests::pid_termination_is_idempotent_after_exit",
                "--nocapture",
            ])
            .env(FIXTURE_ENV, FIXTURE_EXITED)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_detached(&mut command);
        let mut child = command.spawn().expect("spawn exited fixture");
        let pid = child.id();
        assert!(child.wait().expect("wait for exited fixture").success());
        drop(child);
        terminate_process_group(pid).expect("already exited process is terminated");
    }

    #[test]
    fn system_binary_trust_check_fails_closed_when_metadata_is_missing() {
        let path = std::env::temp_dir().join(format!(
            "neomax-missing-system-component-{}",
            std::process::id()
        ));
        assert!(has_linked_component(&path).is_err());
    }

    #[test]
    fn system_directory_decoder_rejects_untrusted_native_results() {
        let valid = "C:\\Windows\\System32";
        let mut buffer = [0u16; 64];
        let units = valid.encode_utf16().collect::<Vec<_>>();
        buffer[..units.len()].copy_from_slice(&units);
        assert_eq!(
            system_directory_from_utf16(&buffer, units.len() as u32).expect("valid path"),
            PathBuf::from(valid)
        );

        assert!(system_directory_from_utf16(&buffer, 0).is_err());
        assert!(system_directory_from_utf16(&buffer, buffer.len() as u32).is_err());

        let mut nul = buffer;
        nul[2] = 0;
        assert!(system_directory_from_utf16(&nul, units.len() as u32).is_err());

        let mut invalid_utf16 = buffer;
        invalid_utf16[0] = 0xD800;
        assert!(system_directory_from_utf16(&invalid_utf16, units.len() as u32).is_err());

        for value in [
            "Windows\\System32",
            "\\Windows\\System32",
            "C:Windows\\System32",
            "C:\\Windows\\..\\System32",
        ] {
            let units = value.encode_utf16().collect::<Vec<_>>();
            let mut candidate = [0u16; 64];
            candidate[..units.len()].copy_from_slice(&units);
            assert!(
                system_directory_from_utf16(&candidate, units.len() as u32).is_err(),
                "path should be rejected: {value}"
            );
        }
    }

    #[test]
    fn descendant_fixture() {
        if std::env::var(FIXTURE_ENV).ok().as_deref() != Some(FIXTURE_DESCENDANT) {
            return;
        }
        let marker = PathBuf::from(
            std::env::var_os("NEOMAX_PROCESS_GROUP_FINISHED").expect("descendant marker path"),
        );
        thread::sleep(Duration::from_millis(700));
        fs::write(marker, b"finished").expect("write descendant marker");
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn parent_fixture() {
        if std::env::var(FIXTURE_ENV).ok().as_deref() != Some(FIXTURE_PARENT) {
            return;
        }
        let started = PathBuf::from(
            std::env::var_os("NEOMAX_PROCESS_GROUP_STARTED").expect("parent started marker path"),
        );
        let finished = PathBuf::from(
            std::env::var_os("NEOMAX_PROCESS_GROUP_FINISHED").expect("parent finished marker path"),
        );
        let executable = std::env::current_exe().expect("test executable");
        let mut descendant = Command::new(executable)
            .args([
                "--exact",
                "io::process_group::windows::tests::descendant_fixture",
                "--nocapture",
            ])
            .env(FIXTURE_ENV, FIXTURE_DESCENDANT)
            .env("NEOMAX_PROCESS_GROUP_FINISHED", finished)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn descendant fixture");
        fs::write(started, b"started").expect("write parent started marker");
        thread::sleep(Duration::from_secs(30));
        let _ = descendant.kill();
        let _ = descendant.wait();
    }

    fn run_descendant_termination_check(label: &str) {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let started = temporary.path().join(format!("{label}-started"));
        let finished = temporary.path().join(format!("{label}-finished"));
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "io::process_group::windows::tests::parent_fixture",
                "--nocapture",
            ])
            .env(FIXTURE_ENV, FIXTURE_PARENT)
            .env("NEOMAX_PROCESS_GROUP_STARTED", &started)
            .env("NEOMAX_PROCESS_GROUP_FINISHED", &finished)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let (mut child, containment) = spawn_managed(&mut command).expect("spawn managed fixture");
        wait_for_marker(&started);
        let terminated = Instant::now();
        terminate(&mut child, &containment, Duration::from_millis(100));
        assert!(terminated.elapsed() < Duration::from_secs(5));
        assert!(child.try_wait().expect("inspect managed fixture").is_some());
        thread::sleep(Duration::from_millis(1_000));
        assert!(!finished.exists(), "descendant escaped managed job");
    }

    fn wait_for_marker(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(path.exists(), "fixture did not reach its spawn handshake");
    }
}
