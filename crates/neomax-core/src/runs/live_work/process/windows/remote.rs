use std::mem::size_of;
use std::ptr;
use std::slice;

use windows_sys::Wdk::System::Threading::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessCommandLineInformation,
    ProcessWow64Information,
};
use windows_sys::Win32::Foundation::{HANDLE, UNICODE_STRING};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Threading::{
    IsWow64Process, PROCESS_NAME_WIN32, QueryFullProcessImageNameW,
};

use super::parsing::environment_end;

const MAX_COMMAND_LINE_BYTES: usize = 128 * 1024;
const MAX_ENVIRONMENT_BYTES: usize = 1024 * 1024;

pub(super) fn process_image_path(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 32 * 1024];
    let mut length = buffer.len() as u32;
    let success = unsafe {
        // SAFETY: the buffer and length describe writable storage owned by this function.
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    } != 0;
    if !success || length == 0 || length as usize > buffer.len() {
        return None;
    }
    String::from_utf16(&buffer[..length as usize]).ok()
}

pub(super) fn process_environment_value(process: HANDLE, key: &str) -> Option<String> {
    let architecture = process_architecture(process)?;
    let environment = match architecture {
        RemoteProcessArchitecture::Native => native_environment_pointer(process)?,
        RemoteProcessArchitecture::Wow64 => wow64_environment_pointer(process)?,
    };
    read_remote_environment(process, environment, key)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RemoteProcessArchitecture {
    Native,
    Wow64,
}

fn process_architecture(process: HANDLE) -> Option<RemoteProcessArchitecture> {
    let mut wow64 = 0;
    let success = unsafe {
        // SAFETY: process is a live handle owned by the caller and wow64 is writable storage.
        IsWow64Process(process, &mut wow64)
    } != 0;
    success.then_some(if wow64 == 0 {
        RemoteProcessArchitecture::Native
    } else {
        RemoteProcessArchitecture::Wow64
    })
}

fn native_environment_pointer(process: HANDLE) -> Option<*mut std::ffi::c_void> {
    let basic = query_basic_information(process)?;
    let peb = read_remote::<RemotePebPrefix>(process, basic.peb_base_address)?;
    let parameters = read_remote::<RemoteProcessParameters>(process, peb.process_parameters)?;
    (!parameters.environment.is_null()).then_some(parameters.environment)
}

fn wow64_environment_pointer(process: HANDLE) -> Option<*mut std::ffi::c_void> {
    let peb_address = query_wow64_peb(process)?;
    let peb = read_remote::<RemotePeb32>(process, peb_address)?;
    let parameters_address = pointer_from_32(peb.process_parameters)?;
    let parameters = read_remote::<RemoteProcessParameters32>(process, parameters_address)?;
    pointer_from_32(parameters.environment)
}

fn read_remote_environment(
    process: HANDLE,
    environment: *mut std::ffi::c_void,
    key: &str,
) -> Option<String> {
    let mut bytes = Vec::new();
    let mut terminated = false;
    for offset in (0..MAX_ENVIRONMENT_BYTES).step_by(4096) {
        let mut chunk = vec![0u8; 4096];
        let address = environment
            .cast::<u8>()
            .cast_const()
            .wrapping_add(offset)
            .cast();
        let Some(read) = read_remote_bytes(process, address, &mut chunk) else {
            return None;
        };
        if read == 0 {
            return None;
        }
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(end) = environment_end(&bytes) {
            bytes.truncate(end);
            terminated = true;
            break;
        }
        if read < chunk.len() {
            return None;
        }
    }
    terminated.then_some(())?;
    environment_value_from_bytes(&bytes, key)
}

fn environment_value_from_bytes(bytes: &[u8], key: &str) -> Option<String> {
    if bytes.is_empty() || bytes.len() % 2 != 0 {
        return None;
    }
    if environment_end(bytes) != Some(bytes.len()) {
        return None;
    }
    let text = String::from_utf16(
        &bytes
            .chunks(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    )
    .ok()?;
    text.split('\0').find_map(|entry| {
        let (name, value) = entry.split_once('=')?;
        (name.eq_ignore_ascii_case(key)
            && !value.is_empty()
            && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
    })
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RemoteProcessBasicInformation {
    _exit_status: i32,
    peb_base_address: *mut std::ffi::c_void,
    _affinity_mask: usize,
    _base_priority: i32,
    _unique_process_id: usize,
    _inherited_from_unique_process_id: usize,
}

impl Default for RemoteProcessBasicInformation {
    fn default() -> Self {
        Self {
            _exit_status: 0,
            peb_base_address: ptr::null_mut(),
            _affinity_mask: 0,
            _base_priority: 0,
            _unique_process_id: 0,
            _inherited_from_unique_process_id: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RemotePebPrefix {
    _reserved1: [u8; 2],
    _being_debugged: u8,
    _reserved2: [u8; 1],
    _reserved3: [*mut std::ffi::c_void; 2],
    _ldr: *mut std::ffi::c_void,
    process_parameters: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RemotePeb32 {
    _reserved1: [u8; 2],
    _being_debugged: u8,
    _reserved2: [u8; 1],
    _reserved3: [u32; 2],
    _ldr: u32,
    process_parameters: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RemoteUnicodeString32 {
    length: u16,
    _maximum_length: u16,
    buffer: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RemoteProcessParameters32 {
    _reserved1: [u8; 16],
    _reserved2: [u32; 10],
    _image_path_name: RemoteUnicodeString32,
    command_line: RemoteUnicodeString32,
    environment: u32,
}

impl Default for RemotePebPrefix {
    fn default() -> Self {
        Self {
            _reserved1: [0; 2],
            _being_debugged: 0,
            _reserved2: [0; 1],
            _reserved3: [ptr::null_mut(); 2],
            _ldr: ptr::null_mut(),
            process_parameters: ptr::null_mut(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RemoteProcessParameters {
    _reserved1: [u8; 16],
    _reserved2: [*mut std::ffi::c_void; 10],
    _image_path_name: UNICODE_STRING,
    _command_line: UNICODE_STRING,
    environment: *mut std::ffi::c_void,
}

impl Default for RemoteProcessParameters {
    fn default() -> Self {
        Self {
            _reserved1: [0; 16],
            _reserved2: [ptr::null_mut(); 10],
            _image_path_name: UNICODE_STRING {
                Length: 0,
                MaximumLength: 0,
                Buffer: ptr::null_mut(),
            },
            _command_line: UNICODE_STRING {
                Length: 0,
                MaximumLength: 0,
                Buffer: ptr::null_mut(),
            },
            environment: ptr::null_mut(),
        }
    }
}

fn query_basic_information(process: HANDLE) -> Option<RemoteProcessBasicInformation> {
    let mut info = RemoteProcessBasicInformation::default();
    let status = unsafe {
        // SAFETY: info is initialized writable storage of the documented basic-information size.
        NtQueryInformationProcess(
            process,
            ProcessBasicInformation,
            (&mut info as *mut RemoteProcessBasicInformation).cast(),
            size_of::<RemoteProcessBasicInformation>() as u32,
            std::ptr::null_mut(),
        )
    };
    (status >= 0 && !info.peb_base_address.is_null()).then_some(info)
}

fn query_wow64_peb(process: HANDLE) -> Option<*mut std::ffi::c_void> {
    let mut peb = 0usize;
    let status = unsafe {
        // SAFETY: peb is writable storage with the documented information size.
        NtQueryInformationProcess(
            process,
            ProcessWow64Information,
            (&mut peb as *mut usize).cast(),
            size_of::<usize>() as u32,
            std::ptr::null_mut(),
        )
    };
    (status >= 0 && peb != 0).then_some(peb as *mut std::ffi::c_void)
}

fn pointer_from_32(address: u32) -> Option<*mut std::ffi::c_void> {
    (address != 0).then_some(address as usize as *mut std::ffi::c_void)
}

fn read_remote<T: Copy + Default>(process: HANDLE, address: *mut std::ffi::c_void) -> Option<T> {
    if address.is_null() {
        return None;
    }
    let mut value = T::default();
    let bytes = unsafe {
        // SAFETY: value is initialized local storage and the slice covers exactly its size.
        std::slice::from_raw_parts_mut((&mut value as *mut T).cast::<u8>(), size_of::<T>())
    };
    let read = read_remote_bytes(process, address.cast_const(), bytes)?;
    (read == size_of::<T>()).then_some(value)
}

fn read_remote_bytes(
    process: HANDLE,
    address: *const std::ffi::c_void,
    destination: &mut [u8],
) -> Option<usize> {
    if destination.is_empty() {
        return None;
    }
    let mut read = 0usize;
    let success = unsafe {
        // SAFETY: destination is owned writable storage; Windows validates the remote address.
        ReadProcessMemory(
            process,
            address,
            destination.as_mut_ptr().cast(),
            destination.len(),
            &mut read,
        )
    } != 0;
    (success && read <= destination.len()).then_some(read)
}

pub(super) fn process_command_line(process: HANDLE) -> Option<String> {
    let architecture = process_architecture(process)?;
    process_command_line_from_peb(process, architecture)
        .or_else(|| process_command_line_information(process))
}

fn process_command_line_from_peb(
    process: HANDLE,
    architecture: RemoteProcessArchitecture,
) -> Option<String> {
    match architecture {
        RemoteProcessArchitecture::Native => {
            let basic = query_basic_information(process)?;
            let peb = read_remote::<RemotePebPrefix>(process, basic.peb_base_address)?;
            let parameters =
                read_remote::<RemoteProcessParameters>(process, peb.process_parameters)?;
            read_remote_unicode_string(
                process,
                parameters._command_line.Length,
                parameters._command_line.Buffer.cast(),
            )
        }
        RemoteProcessArchitecture::Wow64 => {
            let peb_address = query_wow64_peb(process)?;
            let peb = read_remote::<RemotePeb32>(process, peb_address)?;
            let parameters_address = pointer_from_32(peb.process_parameters)?;
            let parameters = read_remote::<RemoteProcessParameters32>(process, parameters_address)?;
            read_remote_unicode_string_32(process, parameters.command_line)
        }
    }
}

fn process_command_line_information(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0u8; size_of::<UNICODE_STRING>() + 1024];
    for _ in 0..4 {
        let mut returned = 0u32;
        let status = unsafe {
            // SAFETY: the process handle is valid and the buffer is writable for its length.
            NtQueryInformationProcess(
                process,
                ProcessCommandLineInformation,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut returned,
            )
        };
        if status >= 0 {
            return command_line_from_buffer(&buffer);
        }
        let requested = returned as usize;
        let next_len = requested.max(buffer.len().saturating_mul(2));
        if next_len <= buffer.len() || next_len > MAX_COMMAND_LINE_BYTES {
            return None;
        }
        buffer.resize(next_len, 0);
    }
    None
}

fn read_remote_unicode_string(
    process: HANDLE,
    length: u16,
    address: *mut std::ffi::c_void,
) -> Option<String> {
    if length == 0 || length % 2 != 0 || length as usize > MAX_COMMAND_LINE_BYTES {
        return None;
    }
    read_remote_utf16(process, address, length as usize)
}

fn read_remote_unicode_string_32(process: HANDLE, value: RemoteUnicodeString32) -> Option<String> {
    read_remote_unicode_string(process, value.length, pointer_from_32(value.buffer)?)
}

fn read_remote_utf16(
    process: HANDLE,
    address: *mut std::ffi::c_void,
    length: usize,
) -> Option<String> {
    if address.is_null() || length == 0 || length % 2 != 0 {
        return None;
    }
    let mut bytes = vec![0u8; length];
    let read = read_remote_bytes(process, address.cast_const(), &mut bytes)?;
    if read != length {
        return None;
    }
    String::from_utf16(
        &bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>(),
    )
    .ok()
}

fn command_line_from_buffer(buffer: &[u8]) -> Option<String> {
    if buffer.len() < size_of::<UNICODE_STRING>() {
        return None;
    }
    let value = unsafe {
        // SAFETY: the size check above permits an unaligned read from initialized bytes.
        ptr::read_unaligned(buffer.as_ptr().cast::<UNICODE_STRING>())
    };
    let length = value.Length as usize;
    if length == 0 || length % 2 != 0 {
        return None;
    }
    let start = buffer.as_ptr() as usize;
    let end = start.checked_add(buffer.len())?;
    let string_start = value.Buffer as usize;
    let string_end = string_start.checked_add(length)?;
    if string_start < start || string_end > end {
        return None;
    }
    let chars = unsafe {
        // SAFETY: the range is proven to be inside the owned byte buffer and UTF-16 is copied.
        slice::from_raw_parts(value.Buffer.cast_const(), length / 2)
    };
    String::from_utf16(chars).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wow64_layouts_match_the_documented_32_bit_prefixes() {
        assert_eq!(size_of::<RemotePeb32>(), 20);
        assert_eq!(size_of::<RemoteUnicodeString32>(), 8);
        assert_eq!(size_of::<RemoteProcessParameters32>(), 76);
    }

    #[test]
    fn wow64_null_pointers_are_rejected() {
        assert!(pointer_from_32(0).is_none());
        assert!(pointer_from_32(1).is_some());
    }

    #[test]
    fn environment_parser_requires_the_bounded_double_nul_terminator() {
        let complete = utf16_bytes("CLAUDE_CONFIG_DIR=/profiles/primary\0\0");
        assert_eq!(
            environment_value_from_bytes(&complete, "CLAUDE_CONFIG_DIR"),
            Some("/profiles/primary".into())
        );

        let truncated = utf16_bytes("CLAUDE_CONFIG_DIR=/profiles/primary\0");
        assert_eq!(
            environment_value_from_bytes(&truncated, "CLAUDE_CONFIG_DIR"),
            None
        );
    }

    fn utf16_bytes(value: &str) -> Vec<u8> {
        value
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    }
}
