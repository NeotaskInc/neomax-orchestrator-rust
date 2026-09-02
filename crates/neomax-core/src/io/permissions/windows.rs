use std::fs::{File, OpenOptions};
use std::io;
use std::mem::size_of;
use std::path::Path;
use std::ptr::{null, null_mut};

use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GetLastError, HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    BuildTrusteeWithSidW, EXPLICIT_ACCESS_W, GRANT_ACCESS, GetSecurityInfo, SE_FILE_OBJECT,
    SetEntriesInAclW, SetSecurityInfo, TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, CONTAINER_INHERIT_ACE, CopySid, DACL_SECURITY_INFORMATION,
    EqualSid, GetAce, GetLengthSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetTokenInformation, INHERITED_ACE, INHERIT_ONLY_ACE, NO_INHERITANCE,
    NO_PROPAGATE_INHERIT_ACE, OBJECT_INHERIT_ACE, PROTECTED_DACL_SECURITY_INFORMATION, PSID,
    SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ALL_ACCESS, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, ReOpenFile,
    READ_CONTROL, WRITE_DAC,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const SYSTEM_SID: windows_sys::core::PCWSTR = windows_sys::core::w!("S-1-5-18");
const PRIVATE_SECURITY_INFORMATION: u32 =
    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;

pub(super) fn set_private_path(path: &Path) -> crate::Result<()> {
    let _path_guard = crate::io::PathGuard::for_path(path).map_err(crate::Error::Io)?;
    let file = open_private_handle(path, true)?;
    let user_sid = current_user_sid()?;
    with_private_acl(&user_sid, |_, acl| {
        let status = unsafe {
            SetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                PRIVATE_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        };
        win32_result("set private path ACL", status)
    })
    .map_err(crate::Error::Io)
}

pub(super) fn set_private_open_path(file: &File, path: &Path) -> crate::Result<()> {
    let _path_guard = crate::io::PathGuard::for_path(path).map_err(crate::Error::Io)?;
    validate_open_handle(file, path, false)?;
    let acl_file = reopen_for_acl(file).map_err(crate::Error::Io)?;
    let user_sid = current_user_sid()?;
    with_private_acl(&user_sid, |_, acl| {
        let status = unsafe {
            SetSecurityInfo(
                acl_file.as_raw_handle(),
                SE_FILE_OBJECT,
                PRIVATE_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut(),
            )
        };
        win32_result("set private open path ACL", status)
    })
    .map_err(crate::Error::Io)
}

fn reopen_for_acl(file: &File) -> io::Result<File> {
    let handle = unsafe {
        ReOpenFile(
            file.as_raw_handle(),
            FILE_READ_ATTRIBUTES | READ_CONTROL | WRITE_DAC,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_FLAG_OPEN_REPARSE_POINT,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error("reopen private path for ACL update"));
    }
    Ok(unsafe { File::from_raw_handle(handle) })
}

pub(super) fn set_private_directory(path: &Path) -> crate::Result<()> {
    set_private_path(path)
}

pub(super) fn verify_private_path(path: &Path) -> crate::Result<()> {
    verify_private_named(path).map_err(crate::Error::Io)
}

fn verify_private_named(path: &Path) -> io::Result<()> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    let file = open_private_handle(path, false)?;
    let descriptor = security_descriptor(file.as_raw_handle())?;
    let result = verify_private_descriptor(descriptor);
    unsafe {
        LocalFree(descriptor as HLOCAL);
    }
    result
}

fn open_private_handle(path: &Path, writable: bool) -> io::Result<File> {
    let _path_guard = crate::io::PathGuard::for_path(path)?;
    crate::io::reject_reparse_components(path)?;
    let mut options = OpenOptions::new();
    options.read(true);
    options
        .access_mode(
            FILE_READ_ATTRIBUTES
                | READ_CONTROL
                | if writable { WRITE_DAC } else { 0 },
        )
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
    let file = options.open(path)?;
    validate_open_handle(&file, path, true)?;
    Ok(file)
}

fn validate_open_handle(file: &File, path: &Path, allow_directory: bool) -> io::Result<()> {
    let metadata = file.metadata()?;
    if (!allow_directory && !metadata.is_file())
        || (allow_directory && !metadata.is_file() && !metadata.is_dir())
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refusing a reparse point or non-file path: {}", path.display()),
        ));
    }
    crate::io::reject_reparse_components(path)
}

fn verify_private_descriptor(
    descriptor: windows_sys::Win32::Security::PSECURITY_DESCRIPTOR,
) -> io::Result<()> {
    let mut control = 0_u16;
    let mut revision = 0_u32;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
        return Err(last_error("read private path security controls"));
    }
    if control & SE_DACL_PROTECTED == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "private Windows ACL permits inherited access",
        ));
    }

    let mut dacl_present = 0;
    let mut dacl_defaulted = 0;
    let mut dacl = null_mut();
    if unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        )
    } == 0
    {
        return Err(last_error("read private path DACL"));
    }
    if dacl_present == 0 || dacl.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "private Windows path has no restrictive DACL",
        ));
    }

    let user_sid = current_user_sid()?;
    let system_sid = system_sid()?;
    let result = verify_private_acl(dacl, &user_sid, system_sid);
    unsafe {
        LocalFree(system_sid as HLOCAL);
    }
    result
}

fn verify_private_acl(
    dacl: *const windows_sys::Win32::Security::ACL,
    user_sid: &[u8],
    system_sid: PSID,
) -> io::Result<()> {
    let ace_count = unsafe { (*dacl).AceCount };
    let mut user_present = false;
    let mut system_present = false;
    let inheritance_flags = u8::try_from(
        CONTAINER_INHERIT_ACE
            | INHERITED_ACE
            | INHERIT_ONLY_ACE
            | NO_PROPAGATE_INHERIT_ACE
            | OBJECT_INHERIT_ACE,
    )
    .map_err(|_| io::Error::other("Windows ACE inheritance flags exceed one byte"))?;

    for index in 0..u32::from(ace_count) {
        let mut raw_ace = null_mut();
        if unsafe { GetAce(dacl, index, &mut raw_ace) } == 0 || raw_ace.is_null() {
            return Err(last_error("read private path ACE"));
        }
        let header = unsafe { &*raw_ace.cast::<ACE_HEADER>() };
        if header.AceType != 0 || usize::from(header.AceSize) < size_of::<ACCESS_ALLOWED_ACE>() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private Windows ACL contains a non-allow or malformed ACE",
            ));
        }
        if header.AceFlags & inheritance_flags != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private Windows ACL contains inherited or inheritable access",
            ));
        }

        let allowed = raw_ace.cast::<ACCESS_ALLOWED_ACE>();
        if unsafe { (*allowed).Mask } != FILE_ALL_ACCESS {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private Windows ACL contains an unexpected access mask",
            ));
        }
        let principal = unsafe { std::ptr::addr_of_mut!((*allowed).SidStart).cast() };
        let is_user = unsafe { EqualSid(principal, user_sid.as_ptr() as PSID) } != 0;
        let is_system = unsafe { EqualSid(principal, system_sid) } != 0;
        if !is_user && !is_system {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private Windows ACL contains an unapproved principal",
            ));
        }
        if is_user && user_present || is_system && system_present {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "private Windows ACL contains a duplicate principal",
            ));
        }
        user_present |= is_user;
        system_present |= is_system;
    }
    if !user_present || !system_present {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "private Windows ACL is missing the current user or SYSTEM",
        ));
    }
    Ok(())
}

fn security_descriptor(
    handle: HANDLE,
) -> io::Result<windows_sys::Win32::Security::PSECURITY_DESCRIPTOR> {
    let mut descriptor = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(win32_error("read private path ACL", status));
    }
    if descriptor.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Windows returned an empty security descriptor",
        ));
    }
    Ok(descriptor)
}

fn with_private_acl<T>(
    user_sid: &[u8],
    apply: impl FnOnce(PSID, *const windows_sys::Win32::Security::ACL) -> io::Result<T>,
) -> io::Result<T> {
    let system_sid = system_sid()?;
    let acl = match private_acl(user_sid, system_sid) {
        Ok(acl) => acl,
        Err(error) => {
            unsafe {
                LocalFree(system_sid as HLOCAL);
            }
            return Err(error);
        }
    };
    let result = apply(user_sid.as_ptr() as PSID, acl);
    unsafe {
        LocalFree(acl as HLOCAL);
        LocalFree(system_sid as HLOCAL);
    }
    result
}

fn private_acl(
    user_sid: &[u8],
    system_sid: PSID,
) -> io::Result<*mut windows_sys::Win32::Security::ACL> {
    let mut user_trustee = TRUSTEE_W::default();
    let mut system_trustee = TRUSTEE_W::default();
    unsafe {
        BuildTrusteeWithSidW(&mut user_trustee, user_sid.as_ptr() as PSID);
        BuildTrusteeWithSidW(&mut system_trustee, system_sid);
    }
    let entries = [
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: user_trustee,
        },
        EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_ALL_ACCESS,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: system_trustee,
        },
    ];
    let entry_count = u32::try_from(entries.len())
        .map_err(|_| io::Error::other("private ACL has too many entries"))?;
    let mut acl = null_mut();
    let status = unsafe { SetEntriesInAclW(entry_count, entries.as_ptr(), null(), &mut acl) };
    if status != ERROR_SUCCESS {
        return Err(win32_error("build private ACL", status));
    }
    if acl.is_null() {
        return Err(io::Error::other("Windows returned an empty private ACL"));
    }
    Ok(acl)
}

fn current_user_sid() -> io::Result<Vec<u8>> {
    let mut token = null_mut();
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if opened == 0 {
        return Err(last_error("open current user token"));
    }
    let result = current_user_sid_from_token(token);
    unsafe {
        CloseHandle(token);
    }
    result
}

fn current_user_sid_from_token(token: HANDLE) -> io::Result<Vec<u8>> {
    let mut required = 0_u32;
    let first = unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut required) };
    if first != 0 || required == 0 {
        return Err(last_error("query current user token size"));
    }
    let mut buffer = vec![0_u8; required as usize];
    let success = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    };
    if success == 0 {
        return Err(last_error("query current user token"));
    }
    let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    let source_sid = token_user.User.Sid;
    if source_sid.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "current user token has no SID",
        ));
    }
    let length = unsafe { GetLengthSid(source_sid) };
    if length == 0 {
        return Err(last_error("read current user SID length"));
    }
    let mut sid = vec![0_u8; length as usize];
    if unsafe { CopySid(length, sid.as_mut_ptr().cast(), source_sid) } == 0 {
        return Err(last_error("copy current user SID"));
    }
    Ok(sid)
}

fn system_sid() -> io::Result<PSID> {
    let mut sid = null_mut();
    let success = unsafe {
        windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW(SYSTEM_SID, &mut sid)
    };
    if success == 0 || sid.is_null() {
        return Err(last_error("resolve Windows SYSTEM SID"));
    }
    Ok(sid)
}

fn win32_result(operation: &str, status: u32) -> io::Result<()> {
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(win32_error(operation, status))
    }
}

fn win32_error(operation: &str, status: u32) -> io::Error {
    io::Error::other(format!(
        "{operation}: {}",
        io::Error::from_raw_os_error(status as i32)
    ))
}

fn last_error(operation: &str) -> io::Error {
    win32_error(operation, unsafe { GetLastError() })
}
