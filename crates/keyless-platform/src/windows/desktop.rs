//! Desktop integration: popup window styling and placement, links, system preferences.

use keyless_core::Edge;
use raw_window_handle::RawWindowHandle;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, HWND_TOPMOST, SPI_GETCLIENTAREAANIMATION, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, SystemParametersInfoW, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONERROR, MB_OK, MessageBoxW, SW_HIDE, SW_SHOWNOACTIVATE, SW_SHOWNORMAL, ShowWindow,
};
use windows::core::{HSTRING, w};

use crate::{Anchor, PopupKind};

pub const SUPER_KEY_NAME: &str = "Win";

/// A system font for languages the embedded fonts don't cover and that the automatic
/// fallback gets wrong. Without it, Simplified Chinese falls back to a Japanese font that
/// lacks many Simplified characters.
pub fn font_for_language(language: &str) -> Option<&'static str> {
    match language {
        "zh" => Some("Microsoft YaHei UI"),
        _ => None,
    }
}

/// Windows' "Show animations in Windows" setting. Off means: keep the UI still.
pub fn animations_enabled() -> bool {
    let mut enabled = windows::core::BOOL(1);
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer we pass.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut enabled).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    enabled.as_bool()
}

/// Opens a web page in the default browser.
pub fn open_url(url: &str) {
    let url = HSTRING::from(url);
    // SAFETY: valid NUL-terminated strings for the duration of the call.
    unsafe { ShellExecuteW(None, w!("open"), &url, None, None, SW_SHOWNORMAL) };
}

/// Blocking error dialog, for failures that happen before any window exists.
pub fn show_error(message: &str) {
    let message = HSTRING::from(message);
    // SAFETY: valid NUL-terminated strings for the duration of the call.
    unsafe { MessageBoxW(None, &message, w!("Keyless"), MB_OK | MB_ICONERROR) };
}

fn hwnd(window: RawWindowHandle) -> Option<HWND> {
    match window {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut _)),
        _ => None,
    }
}

/// Hides the window from the taskbar and Alt+Tab, rounds its corners, keeps it on top,
/// and for overlays prevents it from ever taking keyboard focus.
/// Takes effect in the taskbar the next time the window is shown.
pub fn style_popup(window: RawWindowHandle, kind: PopupKind) {
    let Some(hwnd) = hwnd(window) else { return };
    // SAFETY: `hwnd` is a live window owned by this process.
    unsafe {
        let mut ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        ex = (ex | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
        if kind == PopupKind::Overlay {
            ex |= WS_EX_NOACTIVATE.0;
        }
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&raw const corner).cast(),
            size_of_val(&corner) as u32,
        );
    }
}

/// Shows or hides a window without ever activating it, so the app the user is working
/// in keeps keyboard focus.
pub fn set_visible_passive(window: RawWindowHandle, visible: bool) {
    if let Some(hwnd) = hwnd(window) {
        // SAFETY: `hwnd` is a live window owned by this process.
        let _ = unsafe { ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE }) };
    }
}

/// Gives the window keyboard focus. Works right after a tray click.
pub fn bring_to_front(window: RawWindowHandle) {
    if let Some(hwnd) = hwnd(window) {
        // SAFETY: `hwnd` is a live window owned by this process.
        let _ = unsafe { SetForegroundWindow(hwnd) };
    }
}

/// Work area and scale factor of a monitor.
fn monitor_area(monitor: HMONITOR) -> Option<(RECT, f32)> {
    if monitor.is_invalid() {
        return None;
    }
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
    // SAFETY: plain Win32 queries with valid out-pointers.
    unsafe {
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
        Some((info.rcWork, dpi_x as f32 / 96.0))
    }
}

fn physical(logical_size: (f32, f32), scale: f32) -> (i32, i32) {
    ((logical_size.0 * scale).round() as i32, (logical_size.1 * scale).round() as i32)
}

fn clamp(v: i32, lo: i32, hi: i32) -> i32 {
    v.max(lo).min(hi.max(lo))
}

/// Physical top-left position for a window of `logical_size` on the monitor under the pointer.
pub fn place(anchor: Anchor, logical_size: (f32, f32)) -> Option<(i32, i32)> {
    let mut pointer = POINT::default();
    // SAFETY: valid out-pointer.
    unsafe { GetCursorPos(&mut pointer) }.ok()?;
    // SAFETY: plain Win32 query.
    let (work, scale) = monitor_area(unsafe { MonitorFromPoint(pointer, MONITOR_DEFAULTTONEAREST) })?;
    let (w, h) = physical(logical_size, scale);
    let margin = (12.0 * scale).round() as i32;
    let center_x = work.left + (work.right - work.left - w) / 2;
    let center_y = work.top + (work.bottom - work.top - h) / 2;
    Some(match anchor {
        Anchor::Pointer => {
            let x = clamp(pointer.x - w / 2, work.left + margin, work.right - w - margin);
            // The taskbar is on the side of the screen the pointer is on.
            let y = if pointer.y > (work.top + work.bottom) / 2 {
                work.bottom - h - margin
            } else {
                work.top + margin
            };
            (x, y)
        }
        Anchor::Edge(Edge::Top) => (center_x, work.top + margin),
        Anchor::Edge(Edge::Bottom) => (center_x, work.bottom - h - (32.0 * scale).round() as i32),
        Anchor::Edge(Edge::Left) => (work.left + margin, center_y),
        Anchor::Edge(Edge::Right) => (work.right - w - margin, center_y),
    })
}

/// Returns `spot` (a physical top-left position) moved fully inside the work area of the
/// monitor it is on, or `None` when that monitor is gone, for example after unplugging it.
pub fn fit(spot: (i32, i32), logical_size: (f32, f32)) -> Option<(i32, i32)> {
    let probe = POINT { x: spot.0 + 8, y: spot.1 + 8 };
    // SAFETY: plain Win32 query.
    let (work, scale) = monitor_area(unsafe { MonitorFromPoint(probe, MONITOR_DEFAULTTONULL) })?;
    let (w, h) = physical(logical_size, scale);
    Some((clamp(spot.0, work.left, work.right - w), clamp(spot.1, work.top, work.bottom - h)))
}
