use std::fs;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

use super::super::{
    enforce_private_path, ensure_private_directory, set_private_open_path, verify_private_path,
};
use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HLOCAL, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
    SetNamedSecurityInfoW, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, DACL_SECURITY_INFORMATION, GetAce, GetSecurityDescriptorDacl,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;

#[test]
fn private_acl_has_no_inherited_or_broad_principal() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("private");
    ensure_private_directory(&directory).unwrap();
    let file = directory.join("secret.json");
    fs::write(&file, b"fixture").unwrap();
    enforce_private_path(&file).unwrap();
    verify_private_path(&file).unwrap();

    let descriptor = security_descriptor(&file);
    assert!(
        descriptor.contains("D:P"),
        "ACL is not protected: {descriptor}"
    );
    assert!(
        descriptor.contains(";;;SY"),
        "SYSTEM ACE is missing: {descriptor}"
    );
    assert!(
        descriptor.matches("(A;;").count() >= 2,
        "current-user ACE is missing: {descriptor}"
    );
    assert!(
        !descriptor.contains(";;;WD"),
        "Everyone ACE leaked: {descriptor}"
    );
    assert!(
        !descriptor.contains(";;;BU"),
        "Users ACE leaked: {descriptor}"
    );
    assert!(
        !descriptor.contains(";;;AU"),
        "Authenticated Users ACE leaked: {descriptor}"
    );
}

#[test]
fn private_acl_can_be_applied_to_an_open_named_temp_path() {
    let temp = tempfile::tempdir().unwrap();
    let mut file = tempfile::NamedTempFile::new_in(temp.path()).unwrap();
    file.as_file_mut().write_all(b"fixture").unwrap();
    file.as_file().sync_all().unwrap();
    set_private_open_path(file.as_file(), file.path()).unwrap();
    verify_private_path(file.path()).unwrap();
    assert_eq!(fs::read(file.path()).unwrap(), b"fixture");
    assert!(security_descriptor(file.path()).contains("D:P"));
}

#[test]
fn private_acl_rejects_a_narrowed_access_mask() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("secret.json");
    fs::write(&file, b"fixture").unwrap();
    enforce_private_path(&file).unwrap();

    let mut wide = file.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, ERROR_SUCCESS);

    let mut dacl_present = 0;
    let mut dacl_defaulted = 0;
    let mut dacl = null_mut();
    let success = unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        )
    };
    assert_ne!(success, 0);
    assert_ne!(dacl_present, 0);

    let mut raw_ace = null_mut();
    let success = unsafe { GetAce(dacl, 0, &mut raw_ace) };
    assert_ne!(success, 0);
    let allowed = raw_ace.cast::<ACCESS_ALLOWED_ACE>();
    unsafe {
        (*allowed).Mask = FILE_GENERIC_READ;
    }
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            dacl,
            null_mut(),
        )
    };
    assert_eq!(status, ERROR_SUCCESS);
    unsafe {
        LocalFree(descriptor as HLOCAL);
    }

    let error = verify_private_path(&file).unwrap_err();
    assert!(error.to_string().contains("access mask"));
}

fn security_descriptor(path: &std::path::Path) -> String {
    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, ERROR_SUCCESS);
    let mut string = null_mut();
    let mut length = 0_u32;
    let success = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            1,
            DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION,
            &mut string,
            &mut length,
        )
    };
    assert_ne!(success, 0);
    let units = unsafe { std::slice::from_raw_parts(string, length as usize) };
    let value = String::from_utf16_lossy(units);
    unsafe {
        LocalFree(string as HLOCAL);
        LocalFree(descriptor as HLOCAL);
    }
    value
}
