//! One Keyless per user session. Starting it again opens the running copy instead.

use keyless_core::APP_NAME;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent, WaitForSingleObject,
};
use windows::core::HSTRING;

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
    // Named after the app, so a dev build and the installed app don't see each other.
    let mutex_name = HSTRING::from(format!("Local\\{APP_NAME}.Instance"));
    let event_name = HSTRING::from(format!("Local\\{APP_NAME}.Activate"));
    // SAFETY: named kernel objects; handles are owned below.
    unsafe {
        let mutex = CreateMutexW(None, false, &mutex_name).map_err(os)?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, &event_name) {
                let _ = SetEvent(event);
                let _ = CloseHandle(event);
            }
            return Ok(None);
        }
        let event = CreateEventW(None, false, false, &event_name).map_err(os)?;
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
