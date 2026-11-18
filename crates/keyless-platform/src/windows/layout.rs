//! What each key does under the active keyboard layout, for the drawn keyboard.

use keyless_core::{KeyCap, LAYOUT_KEYS};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, HKL, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::blocker::shortcut_key;

/// Set 1 scan codes of [`LAYOUT_KEYS`], in the same order.
const SCAN_CODES: [u16; LAYOUT_KEYS.len()] = [
    0x29, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, //
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x2B, //
    0x1E, 0x1F, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, //
    0x2C, 0x2D, 0x2E, 0x2F, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35,
];

/// Identifies the active layout. Cheap: compare it to tell when [`key_caps`] would change.
pub fn active_layout() -> usize {
    foreground_layout().0 as usize
}

/// One [`KeyCap`] per entry of [`LAYOUT_KEYS`], for the active layout.
pub fn key_caps() -> Vec<KeyCap> {
    let layout = foreground_layout();
    SCAN_CODES
        .iter()
        .map(|&scan| {
            // SAFETY: pure lookups in the layout tables; no keyboard state changes.
            let vk = unsafe { MapVirtualKeyExW(u32::from(scan), MAPVK_VSC_TO_VK_EX, Some(layout)) };
            let vk = u16::try_from(vk).unwrap_or(0);
            match shortcut_key(vk) {
                Some(key) => KeyCap::shortcut(key),
                None => {
                    // SAFETY: as above. Bit 31 marks a dead key; the low word is the character.
                    let character =
                        unsafe { MapVirtualKeyExW(u32::from(vk), MAPVK_VK_TO_CHAR, Some(layout)) } & 0xFFFF;
                    KeyCap::character(if vk == 0 { None } else { char::from_u32(character) })
                }
            }
        })
        .collect()
}

/// The layout of the app in front. The hook sees keys through that same layout, so the
/// drawn keyboard matches what the shortcut reacts to.
fn foreground_layout() -> HKL {
    // SAFETY: plain queries; a null window gives thread 0, the calling thread.
    unsafe {
        let window = GetForegroundWindow();
        let thread = if window.is_invalid() { 0 } else { GetWindowThreadProcessId(window, None) };
        GetKeyboardLayout(thread)
    }
}
