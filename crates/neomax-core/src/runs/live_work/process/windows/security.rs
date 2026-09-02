use std::mem::size_of;
use std::ptr;
use std::slice;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::handles::OwnedHandle;

const MAX_SID_BYTES: usize = 4096;

pub(super) fn current_user_sid() -> Option<Vec<u8>> {
    let token = unsafe {
        // SAFETY: GetCurrentProcess returns a pseudo-handle valid for this process. The token
        // handle is returned through a valid out pointer and owned by OwnedHandle below.
        let mut token = std::ptr::null_mut();
        (OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0).then_some(token)
    }?;
    let token = OwnedHandle::new(token)?;
    token_user_sid(token.raw())
}

pub(super) fn token_user_sid(token: HANDLE) -> Option<Vec<u8>> {
    let mut required = 0u32;
    unsafe {
        // SAFETY: the zero-length probe is the documented way to obtain the required size.
        let _ = GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut required);
    }
    if required == 0 || required as usize > MAX_SID_BYTES * 4 {
        return None;
    }
    let mut storage = vec![0u8; required as usize];
    let success = unsafe {
        // SAFETY: storage has the exact probed size and is writable for the token record.
        GetTokenInformation(
            token,
            TokenUser,
            storage.as_mut_ptr().cast(),
            storage.len() as u32,
            &mut required,
        )
    } != 0;
    if !success || storage.len() < size_of::<TOKEN_USER>() {
        return None;
    }
    let user = unsafe {
        // SAFETY: TOKEN_USER is read unaligned from the initialized API-owned byte record.
        ptr::read_unaligned(storage.as_ptr().cast::<TOKEN_USER>())
    };
    if user.User.Sid.is_null() {
        return None;
    }
    let sid_length = unsafe {
        // SAFETY: the pointer is supplied by GetTokenInformation and checked for null.
        GetLengthSid(user.User.Sid) as usize
    };
    if sid_length == 0 || sid_length > MAX_SID_BYTES {
        return None;
    }
    let sid_end = (user.User.Sid as usize).checked_add(sid_length)?;
    let storage_start = storage.as_ptr() as usize;
    let storage_end = storage_start.checked_add(storage.len())?;
    if (user.User.Sid as usize) < storage_start || sid_end > storage_end {
        return None;
    }
    let sid = unsafe {
        // SAFETY: the SID range is proven to lie inside storage and is copied before return.
        slice::from_raw_parts(user.User.Sid.cast_const().cast::<u8>(), sid_length)
    };
    Some(sid.to_vec())
}
