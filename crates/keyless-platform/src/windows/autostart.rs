//! Start at sign-in through the per-user `Run` registry key. No admin rights needed.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{HSTRING, w};

use crate::Error;

const RUN_KEY: windows::core::PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: windows::core::PCWSTR = w!("Keyless");

/// True when the sign-in entry starts this executable (not an older copy elsewhere).
pub fn is_enabled() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let mut buffer = [0u16; 2048];
    let mut size = size_of_val(&buffer) as u32;
    // SAFETY: `buffer` is writable for `size` bytes.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if result != ERROR_SUCCESS {
        return false;
    }
    let len = (size as usize / 2).saturating_sub(1);
    let command = String::from_utf16_lossy(&buffer[..len]).to_lowercase();
    command.contains(&exe.display().to_string().to_lowercase())
}

/// Enabling points the entry at the running executable, started in the background.
pub fn set_enabled(enabled: bool) -> Result<(), Error> {
    let result = if enabled {
        let exe = std::env::current_exe().map_err(|e| Error::Os(e.to_string()))?;
        let command = HSTRING::from(format!("\"{}\" --background", exe.display()));
        let bytes = (command.len() + 1) * 2;
        // SAFETY: `command` is a NUL-terminated UTF-16 buffer of `bytes` bytes.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                bytes as u32,
            )
        }
    } else {
        // SAFETY: plain registry call.
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) } {
            ERROR_FILE_NOT_FOUND => ERROR_SUCCESS,
            other => other,
        }
    };
    if result == ERROR_SUCCESS { Ok(()) } else { Err(Error::Os(format!("registry error {}", result.0))) }
}
