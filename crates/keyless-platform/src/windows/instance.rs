//! One Keyless per user session. Starting it again opens the running copy instead.

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
};
use windows::core::w;

use crate::Error;

/// Keeps this process registered as the running instance until dropped.
pub struct Guard {
    mutex: HANDLE,
}

impl Drop for Guard {
    fn drop(&mut self) {
        // SAFETY: we own the handle.
        let _ = unsafe { CloseHandle(self.mutex) };
    }
}

/// Returns `Ok(None)` if another instance is running; that instance is asked to activate.
/// Otherwise `on_activate` runs (on a background thread) whenever a later launch asks.
pub fn acquire(on_activate: impl Fn() + Send + 'static) -> Result<Option<Guard>, Error> {
    let os = |e: windows::core::Error| Error::Os(e.message());
    // SAFETY: named kernel objects; handles are owned below.
    unsafe {
        let mutex = CreateMutexW(None, false, w!("Local\\Keyless.Instance")).map_err(os)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, w!("Local\\Keyless.Activate")) {
                let _ = SetEvent(event);
                let _ = CloseHandle(event);
            }
            return Ok(None);
        }
        let event = CreateEventW(None, false, false, w!("Local\\Keyless.Activate")).map_err(os)?;
        let raw = event.0 as usize;
        std::thread::Builder::new()
            .name("keyless-instance".into())
            .spawn(move || {
                let event = HANDLE(raw as *mut _);
                // SAFETY: the event lives for the rest of the process.
                while WaitForSingleObject(event, INFINITE) == WAIT_OBJECT_0 {
                    on_activate();
                }
            })
            .map_err(|e| Error::Os(e.to_string()))?;
        Ok(Some(Guard { mutex }))
    }
}
