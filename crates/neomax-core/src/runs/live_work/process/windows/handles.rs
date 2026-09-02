use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};

pub(super) struct OwnedHandle(HANDLE);

impl OwnedHandle {
    pub(super) fn new(handle: HANDLE) -> Option<Self> {
        (!handle.is_null() && handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
    }

    pub(super) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: this handle was returned by a Windows handle-producing API and is owned here.
            let _ = CloseHandle(self.0);
        }
    }
}
