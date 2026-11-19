//! Keyboard blocking with a low-level keyboard hook (`WH_KEYBOARD_LL`), and optional mouse
//! blocking with a low-level mouse hook (`WH_MOUSE_LL`).
//!
//! The hook and all keyboard state live on one dedicated thread with its own message loop,
//! so a busy UI can never delay key events. The hook does bounded in-memory work only.
//! A hidden window on the same thread receives screen-lock, logoff and sleep notifications
//! and owns the hotkey registration used to detect shortcut conflicts.
//!
//! Synthetic input (`LLKHF_INJECTED`, e.g. on-screen keyboards, voice typing or automation
//! tools) is passed through untouched: Keyless pauses the physical keyboard only.
//!
//! The mouse hook exists only while the mouse is paused (and until blocked buttons are
//! released), so the mouse pays nothing the rest of the time. Injected mouse input passes
//! through too.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use keyless_core::{Key, KeyEvent, Keyboard, Modifiers, Mouse, MouseBlock, MouseEvent, Shortcut};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, HC_ACTION,
    HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW,
    PostThreadMessageW, RegisterClassW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_POWERBROADCAST, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_WTSSESSION_CHANGE,
    WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW,
};
use windows::core::w;

use crate::{BlockerEvent, BlockerOptions, Error};

const WM_COMMAND_READY: u32 = WM_APP + 1;
const NOTIFY_FOR_THIS_SESSION: u32 = 0;
const WTS_SESSION_LOGOFF: usize = 6;
const WTS_SESSION_LOCK: usize = 7;
const PBT_APMSUSPEND: usize = 4;

enum Command {
    Locked(bool),
    Shortcut(Option<Shortcut>, Sender<Result<(), Error>>),
    ShortcutUnlocks(bool),
    Recording(bool),
    MouseBlock(MouseBlock),
}

pub struct Blocker {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
    commands: Sender<Command>,
    locked: Arc<AtomicBool>,
}

impl Blocker {
    /// Installs the hook on a new thread. Fails if Windows refuses the hook.
    /// The shortcut in `options` is applied without conflict checks; call
    /// [`Blocker::set_shortcut`] afterwards to reserve it.
    pub fn start(
        options: BlockerOptions,
        on_event: impl Fn(BlockerEvent) + Send + 'static,
    ) -> Result<Self, Error> {
        let (commands, receiver) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let locked = Arc::new(AtomicBool::new(false));
        let thread_locked = locked.clone();
        let thread = thread::Builder::new()
            .name("keyless-input".into())
            .spawn(move || run(options, Box::new(on_event), thread_locked, receiver, ready_tx))
            .map_err(|e| Error::Os(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self { thread_id, thread: Some(thread), commands, locked }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(Error::Os("input thread exited".into())),
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Acquire)
    }

    pub fn set_locked(&self, locked: bool) {
        self.send(Command::Locked(locked));
    }

    /// Reserves the shortcut with Windows and starts listening for it.
    /// On conflict the previous shortcut stays active.
    pub fn set_shortcut(&self, shortcut: Option<Shortcut>) -> Result<(), Error> {
        let (reply, response) = mpsc::channel();
        self.send(Command::Shortcut(shortcut, reply));
        response.recv().unwrap_or_else(|_| Err(Error::Os("input thread exited".into())))
    }

    pub fn set_shortcut_unlocks(&self, unlocks: bool) {
        self.send(Command::ShortcutUnlocks(unlocks));
    }

    pub fn set_recording(&self, recording: bool) {
        self.send(Command::Recording(recording));
    }

    /// How much of the mouse pauses with the keyboard. Applies at once, also while paused.
    pub fn set_mouse_block(&self, block: MouseBlock) {
        self.send(Command::MouseBlock(block));
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_ok() {
            // SAFETY: plain Win32 call with a thread id we own.
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_COMMAND_READY, WPARAM(0), LPARAM(0)) };
        }
    }
}

impl Drop for Blocker {
    fn drop(&mut self) {
        // SAFETY: plain Win32 call with a thread id we own.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct State {
    keyboard: Keyboard,
    mouse: Mouse,
    on_event: Box<dyn Fn(BlockerEvent) + Send>,
    locked: Arc<AtomicBool>,
    instance: HINSTANCE,
    window: HWND,
    hotkey_id: Option<i32>,
    mouse_hook: Option<HHOOK>,
}

impl State {
    fn publish_lock(&mut self) {
        let locked = self.keyboard.locked();
        self.update_mouse_hook();
        if self.locked.swap(locked, Ordering::AcqRel) != locked {
            (self.on_event)(BlockerEvent::LockChanged(locked));
        }
    }

    /// Installs the mouse hook only while it has work to do, and removes it right after.
    fn update_mouse_hook(&mut self) {
        let needed = self.mouse.watching(self.keyboard.locked());
        if needed && self.mouse_hook.is_none() {
            // SAFETY: installed and removed on this thread, which runs a message loop.
            self.mouse_hook =
                unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(self.instance), 0) }.ok();
        } else if !needed && let Some(hook) = self.mouse_hook.take() {
            // SAFETY: a hook this thread installed.
            let _ = unsafe { UnhookWindowsHookEx(hook) };
        }
    }

    fn mouse(&mut self, event: MouseEvent) -> bool {
        let consume = self.mouse.event(self.keyboard.locked(), event);
        if !self.mouse.watching(self.keyboard.locked()) {
            self.update_mouse_hook();
        }
        consume
    }

    fn key(&mut self, info: &KBDLLHOOKSTRUCT, down: bool) -> bool {
        let Ok(code) = u16::try_from(info.vkCode) else { return self.keyboard.locked() };
        let decision =
            self.keyboard.event(KeyEvent { code, down, modifier: modifier(code), key: shortcut_key(code) });
        if decision.toggled {
            self.publish_lock();
        }
        if let Some(capture) = decision.captured {
            (self.on_event)(BlockerEvent::Captured(capture));
        }
        decision.consume
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Locked(locked) => {
                self.keyboard.set_locked(locked);
                self.publish_lock();
            }
            Command::Shortcut(shortcut, reply) => {
                let _ = reply.send(self.reserve(shortcut));
            }
            Command::ShortcutUnlocks(unlocks) => self.keyboard.set_shortcut_unlocks(unlocks),
            Command::Recording(recording) => self.keyboard.set_recording(recording),
            Command::MouseBlock(block) => {
                self.mouse.set_block(block);
                self.update_mouse_hook();
            }
        }
    }

    /// Registers the new hotkey before releasing the old one, so a conflict never leaves
    /// the user without a working shortcut. The hook does the actual toggling: the
    /// registration only claims the combination and detects conflicts.
    fn reserve(&mut self, shortcut: Option<Shortcut>) -> Result<(), Error> {
        let next_id = if self.hotkey_id == Some(1) { 2 } else { 1 };
        if let Some(s) = shortcut {
            let modifiers = HOT_KEY_MODIFIERS(u32::from(s.modifiers.bits()) | MOD_NOREPEAT.0);
            // SAFETY: the window belongs to this thread.
            unsafe { RegisterHotKey(Some(self.window), next_id, modifiers, virtual_key(s.key)) }
                .map_err(|_| Error::ShortcutTaken)?;
        }
        if let Some(old) = self.hotkey_id.take() {
            // SAFETY: the window belongs to this thread.
            let _ = unsafe { UnregisterHotKey(Some(self.window), old) };
        }
        self.hotkey_id = shortcut.map(|_| next_id);
        self.keyboard.set_shortcut(shortcut);
        Ok(())
    }

    /// Screen lock, logoff or sleep: resume and forget held keys, because their key-up
    /// events may never arrive.
    fn system_interrupt(&mut self) {
        self.keyboard.set_recording(false);
        self.keyboard.reset_physical_state();
        self.mouse.reset_physical_state();
        self.keyboard.set_locked(false);
        self.publish_lock();
    }
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(f))
}

fn run(
    options: BlockerOptions,
    on_event: Box<dyn Fn(BlockerEvent) + Send>,
    locked: Arc<AtomicBool>,
    commands: Receiver<Command>,
    ready: Sender<Result<u32, Error>>,
) {
    let os = |e: windows::core::Error| Error::Os(e.message());
    // SAFETY: all handles created here are used and destroyed on this thread only.
    let setup = (|| unsafe {
        let mut msg = MSG::default();
        // Create this thread's message queue before anyone posts to it.
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        let instance: HINSTANCE = GetModuleHandleW(None).map_err(os)?.into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: w!("KeylessInputSink"),
            ..Default::default()
        };
        RegisterClassW(&class);
        // A hidden top-level window: message-only windows do not get power broadcasts.
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("KeylessInputSink"),
            w!("Keyless input"),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
        .map_err(os)?;
        let _ = WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);
        let mut keyboard = Keyboard::new();
        keyboard.set_shortcut(options.shortcut);
        keyboard.set_shortcut_unlocks(options.shortcut_unlocks);
        STATE.with(|cell| {
            *cell.borrow_mut() = Some(State {
                keyboard,
                mouse: Mouse::new(),
                on_event,
                locked,
                instance,
                window,
                hotkey_id: None,
                mouse_hook: None,
            });
        });
        match SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), Some(instance), 0) {
            Ok(hook) => Ok((window, hook)),
            Err(e) => {
                let _ = DestroyWindow(window);
                Err(os(e))
            }
        }
    })();
    let (window, hook): (HWND, HHOOK) = match setup {
        Ok(handles) => handles,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    // SAFETY: plain Win32 call.
    let _ = ready.send(Ok(unsafe { GetCurrentThreadId() }));

    let mut msg = MSG::default();
    // SAFETY: standard message loop on this thread's queue.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.hwnd.is_invalid() && msg.message == WM_COMMAND_READY {
            while let Ok(command) = commands.try_recv() {
                with_state(|s| s.command(command));
            }
            continue;
        }
        // SAFETY: dispatching a message we just received.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    // SAFETY: tearing down handles created above on this thread.
    unsafe {
        let _ = UnhookWindowsHookEx(hook);
        with_state(|s| {
            if let Some(mouse_hook) = s.mouse_hook.take() {
                let _ = UnhookWindowsHookEx(mouse_hook);
            }
            if let Some(id) = s.hotkey_id.take() {
                let _ = UnregisterHotKey(Some(window), id);
            }
            s.keyboard.set_locked(false);
            s.locked.store(false, Ordering::Release);
        });
        let _ = WTSUnRegisterSessionNotification(window);
        let _ = DestroyWindow(window);
    }
    STATE.with(|cell| cell.borrow_mut().take());
}

unsafe extern "system" fn hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT owned by the system.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if info.flags.0 & LLKHF_INJECTED.0 == 0 {
            let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            if with_state(|s| s.key(info, down)).unwrap_or(false) {
                return LRESULT(1);
            }
        }
    }
    // SAFETY: forwarding the unchanged arguments to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT owned by the system.
        let info = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if info.flags & LLMHF_INJECTED == 0
            && let Some(event) = mouse_event(wparam.0 as u32, info.mouseData)
            && with_state(|s| s.mouse(event)).unwrap_or(false)
        {
            return LRESULT(1);
        }
    }
    // SAFETY: forwarding the unchanged arguments to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn mouse_event(message: u32, data: u32) -> Option<MouseEvent> {
    let button = |button, down| Some(MouseEvent::Button { button, down });
    // The high word of `data` says which side button: 1 or 2.
    let side = 2 + (data >> 16).clamp(1, 2) as u8;
    match message {
        WM_MOUSEMOVE => Some(MouseEvent::Move),
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => Some(MouseEvent::Wheel),
        WM_LBUTTONDOWN => button(0, true),
        WM_LBUTTONUP => button(0, false),
        WM_RBUTTONDOWN => button(1, true),
        WM_RBUTTONUP => button(1, false),
        WM_MBUTTONDOWN => button(2, true),
        WM_MBUTTONUP => button(2, false),
        WM_XBUTTONDOWN => button(side, true),
        WM_XBUTTONUP => button(side, false),
        _ => None,
    }
}

unsafe extern "system" fn window_proc(window: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let interrupt = match msg {
        WM_WTSSESSION_CHANGE => matches!(wparam.0, WTS_SESSION_LOCK | WTS_SESSION_LOGOFF),
        WM_POWERBROADCAST => wparam.0 == PBT_APMSUSPEND,
        _ => false,
    };
    if interrupt {
        with_state(State::system_interrupt);
    }
    // SAFETY: default handling for our own window.
    unsafe { DefWindowProcW(window, msg, wparam, lparam) }
}

fn modifier(vk: u16) -> Option<Modifiers> {
    match vk {
        0x10 | 0xA0 | 0xA1 => Some(Modifiers::SHIFT),
        0x11 | 0xA2 | 0xA3 => Some(Modifiers::CTRL),
        0x12 | 0xA4 | 0xA5 => Some(Modifiers::ALT),
        0x5B | 0x5C => Some(Modifiers::SUPER),
        _ => None,
    }
}

pub(super) fn shortcut_key(vk: u16) -> Option<Key> {
    match vk {
        0x30..=0x39 => Some(Key::Digit((vk - 0x30) as u8)),
        0x41..=0x5A => Some(Key::Letter(vk as u8 as char)),
        0x70..=0x87 => Some(Key::Function((vk - 0x6F) as u8)),
        _ => None,
    }
}

fn virtual_key(key: Key) -> u32 {
    match key {
        Key::Letter(c) => c as u32,
        Key::Digit(d) => 0x30 + u32::from(d),
        Key::Function(n) => 0x6F + u32::from(n),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_keys_round_trip() {
        for vk in (0x30..=0x39).chain(0x41..=0x5A).chain(0x70..=0x87) {
            let key = shortcut_key(vk).unwrap();
            assert_eq!(virtual_key(key), u32::from(vk));
        }
        assert_eq!(shortcut_key(0x1B), None);
    }
}
