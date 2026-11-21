//! Keyboard blocking with exclusive evdev grabs (`EVIOCGRAB`) and uinput copies.
//!
//! Why this approach: Linux has no global keyboard hook. On Wayland, apps can't see or stop
//! other apps' input at all, and on X11 a keyboard grab breaks the focused window. Below
//! the display server, every keyboard is a device in `/dev/input`, the same on X11, Wayland
//! and the console. Keyless grabs each keyboard, so only Keyless reads it, and replays the
//! events that should pass through a virtual copy of that keyboard (uinput). Like the hook
//! on Windows, this sees every key before any app and can drop any single event.
//!
//! The keyboard is held for as long as Keyless runs, not only while paused, so every press
//! and its release come from the same device. Grabbing only while paused would strand the
//! releases of keys held at that moment: the desktop ignores a release from a device that
//! never pressed the key. For the same reason a device is grabbed only when no key on it is
//! down (the Enter that started Keyless, for example).
//!
//! Mice and touchpads are grabbed only while the mouse is paused and until blocked buttons
//! are released. The rest of the time Keyless doesn't even open them.
//!
//! This needs read and write access to `/dev/input/event*` and `/dev/uinput`. The packages
//! install a udev rule that gives it to the user at the screen (`uaccess`).
//!
//! Software-sent input (uinput devices, such as on-screen keyboards or remappers) is never
//! grabbed. If Keyless ends, the kernel releases the grabs and removes the copies, releasing
//! any key they held, so the keyboard can't stay paused.
//!
//! Screen lock, sleep and switching to another user come from logind ([`super::session`]).

use std::collections::HashSet;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use evdev::InputEvent;
use keyless_core::{KeyEvent, Keyboard, MAX_KEY_CODE, Mouse, MouseBlock, MouseEvent, Shortcut};

use super::device::Device;
use super::keys::{self, EV_ABS, EV_KEY, EV_MSC, EV_REL, EV_SYN, SYN_DROPPED, SYN_REPORT};
use super::session::{self, SessionEvent};
use crate::{BlockerEvent, BlockerOptions, Error};

const INPUT_DIR: &str = "/dev/input";
const SETUP_HELP: &str =
    concat!("Install the Keyless udev rule: see ", env!("CARGO_PKG_REPOSITORY"), "/blob/main/docs/LINUX.md");

enum Command {
    Locked(bool),
    Shortcut(Option<Shortcut>),
    ShortcutUnlocks(bool),
    Recording(bool),
    MouseBlock(MouseBlock),
    Session(SessionEvent),
    Quit,
}

/// Sends commands to the input thread and wakes it up.
struct Commands {
    sender: Sender<Command>,
    wake: UnixStream,
}

impl Commands {
    fn send(&self, command: Command) {
        if self.sender.send(command).is_ok() {
            // Nonblocking: a full socket already means "wake up".
            let _ = (&self.wake).write(&[1]);
        }
    }
}

pub struct Blocker {
    thread: Option<JoinHandle<()>>,
    commands: Arc<Commands>,
    locked: Arc<AtomicBool>,
}

impl Blocker {
    /// Grabs the keyboards on a new thread. Fails without access to `/dev/input` or
    /// `/dev/uinput`. There is no global shortcut registry on Linux, so the shortcut in
    /// `options` and from [`Blocker::set_shortcut`] is never reported as taken.
    pub fn start(
        options: BlockerOptions,
        on_event: impl Fn(BlockerEvent) + Send + 'static,
    ) -> Result<Self, Error> {
        let os = |e: io::Error| Error::Os(e.to_string());
        let (sender, receiver) = mpsc::channel();
        let (wake, woken) = UnixStream::pair().map_err(os)?;
        wake.set_nonblocking(true).map_err(os)?;
        woken.set_nonblocking(true).map_err(os)?;
        let commands = Arc::new(Commands { sender, wake });
        let locked = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread_locked = locked.clone();
        let thread = thread::Builder::new()
            .name("keyless-input".into())
            .spawn(move || run(options, Box::new(on_event), thread_locked, receiver, woken, ready_tx))
            .map_err(|e| Error::Os(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = thread.join();
                return Err(e);
            }
            Err(_) => return Err(Error::Os("input thread exited".into())),
        }
        let session_commands = commands.clone();
        session::watch(move |event| session_commands.send(Command::Session(event)));
        Ok(Self { thread: Some(thread), commands, locked })
    }

    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Acquire)
    }

    pub fn set_locked(&self, locked: bool) {
        self.commands.send(Command::Locked(locked));
    }

    /// Starts listening for the shortcut. Keyless sees keys before the desktop does and
    /// consumes the shortcut, so nothing else can take it.
    pub fn set_shortcut(&self, shortcut: Option<Shortcut>) -> Result<(), Error> {
        self.commands.send(Command::Shortcut(shortcut));
        Ok(())
    }

    pub fn set_shortcut_unlocks(&self, unlocks: bool) {
        self.commands.send(Command::ShortcutUnlocks(unlocks));
    }

    pub fn set_recording(&self, recording: bool) {
        self.commands.send(Command::Recording(recording));
    }

    /// How much of the mouse pauses with the keyboard. Applies at once, also while paused.
    pub fn set_mouse_block(&self, block: MouseBlock) {
        self.commands.send(Command::MouseBlock(block));
    }
}

impl Drop for Blocker {
    fn drop(&mut self) {
        self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The decisions, apart from the devices so both can be borrowed at once.
struct Input {
    keyboard: Keyboard,
    mouse: Mouse,
    on_event: Box<dyn Fn(BlockerEvent) + Send>,
    /// The shortcut paused or resumed while reading events.
    toggled: bool,
}

impl Input {
    fn paused(&self) -> bool {
        self.keyboard.locked()
    }

    /// Whether a key event passes. Updates which keys the device and its copy hold.
    fn key(&mut self, device: &mut Device, code: u16, value: i32, pointer_frozen: bool) -> bool {
        let down = value != 0;
        device.pressed.set(code, down);
        let passes = if let Some(button) = keys::mouse_button(code) {
            !self.mouse.event(self.paused(), MouseEvent::Button { button, down })
        } else if device.keyboard && code < MAX_KEY_CODE {
            let decision = self.keyboard.event(KeyEvent {
                code,
                down,
                modifier: keys::modifier(code),
                key: keys::shortcut_key(code),
            });
            self.toggled |= decision.toggled;
            if let Some(capture) = decision.captured {
                (self.on_event)(BlockerEvent::Captured(capture));
            }
            !decision.consume
        } else {
            // Touch and tool keys of a touchpad go with the pointer.
            !pointer_frozen
        };
        // Whatever was decided, the copy releases exactly what it pressed, so no key or
        // button can stay down in apps.
        let held = device.held.get(code);
        match value {
            0 => {
                device.held.set(code, false);
                held
            }
            1 => {
                device.held.set(code, held || passes);
                passes
            }
            _ => passes && held, // auto-repeat
        }
    }

    /// Filters one complete frame of `device` into its copy.
    fn frame(&mut self, device: &mut Device) {
        let mut frame = std::mem::take(&mut device.frame);
        let mut out = std::mem::take(&mut device.out);
        out.clear();
        let pointer_frozen = self.mouse.event(self.paused(), MouseEvent::Move);
        let mut scan_codes = false;
        let mut others = false;
        for event in &frame {
            let (kind, code) = (event.event_type().0, event.code());
            let passes = match kind {
                EV_MSC => {
                    scan_codes = true;
                    continue;
                }
                EV_KEY => self.key(device, code, event.value(), pointer_frozen),
                EV_REL | EV_ABS => !self.mouse.event(self.paused(), keys::axis_event(kind, code)),
                _ => true,
            };
            others = true;
            if passes {
                out.push(*event);
            }
        }
        // Scan codes (MSC_SCAN) go with the keys they describe.
        if scan_codes && (!out.is_empty() || !others) {
            out = frame.iter().filter(|e| e.event_type().0 == EV_MSC).chain(out.iter()).copied().collect();
        }
        if !out.is_empty()
            && let Some(copy) = device.copy.as_mut()
        {
            let _ = copy.emit(&out);
        }
        frame.clear();
        device.frame = frame;
        device.out = out;
    }

    /// Feeds the releases of everything `device` held, for example when it was unplugged.
    fn release_all(&mut self, device: &Device) {
        for code in device.pressed.iter() {
            if let Some(button) = keys::mouse_button(code) {
                self.mouse.event(self.paused(), MouseEvent::Button { button, down: false });
            } else if device.keyboard && code < MAX_KEY_CODE {
                self.keyboard.event(KeyEvent {
                    code,
                    down: false,
                    modifier: keys::modifier(code),
                    key: None,
                });
            }
        }
    }
}

struct State {
    input: Input,
    devices: Vec<Device>,
    locked: Arc<AtomicBool>,
    /// Pointing devices are open (the mouse is paused, or a blocked button is still down).
    pointers: bool,
    /// The user's session is in front. Another user's session must get the keyboard.
    active: bool,
}

impl State {
    fn publish_lock(&mut self) {
        let locked = self.input.keyboard.locked();
        self.update_pointers();
        if self.locked.swap(locked, Ordering::AcqRel) != locked {
            (self.input.on_event)(BlockerEvent::LockChanged(locked));
        }
    }

    /// Opens and grabs mice and touchpads only while their events may be blocked.
    fn update_pointers(&mut self) {
        let needed = self.active && self.input.mouse.watching(self.input.keyboard.locked());
        if needed {
            if !self.pointers {
                self.pointers = true;
                self.scan();
            }
            for device in self.devices.iter_mut().filter(|d| d.pointer && !d.grabbed()) {
                let _ = device.grab();
            }
        } else {
            self.pointers = false;
            // A device whose copy still holds a button stays until it is released.
            self.devices.retain(|d| d.keyboard || !d.held.is_empty());
        }
    }

    /// Opens every device not open yet that Keyless needs now.
    /// Returns how many devices could not be opened for lack of permission.
    fn scan(&mut self) -> usize {
        let Ok(entries) = std::fs::read_dir(INPUT_DIR) else { return 0 };
        let mut denied = 0;
        let mut paths: Vec<PathBuf> =
            entries.flatten().map(|entry| entry.path()).filter(|path| is_event_node(path)).collect();
        paths.sort();
        for path in paths {
            if let Err(e) = self.add(&path)
                && e.kind() == io::ErrorKind::PermissionDenied
            {
                denied += 1;
            }
        }
        denied
    }

    fn add(&mut self, path: &Path) -> io::Result<()> {
        if !self.active || self.devices.iter().any(|d| d.path == path) {
            return Ok(());
        }
        let Some(mut device) = Device::open(path)? else { return Ok(()) };
        if device.keyboard || self.pointers {
            device.grab()?;
            self.devices.push(device);
        }
        Ok(())
    }

    fn remove(&mut self, index: usize) {
        let device = self.devices.remove(index);
        self.input.release_all(&device);
        // Dropping the copy makes the kernel release whatever it held.
        drop(device);
        self.update_pointers();
    }

    /// Reads what one device sent. `Err` means the device is gone.
    fn read(&mut self, index: usize) -> io::Result<()> {
        let device = &mut self.devices[index];
        if !device.grabbed() {
            // Waiting for held keys to be released: they still reach apps directly.
            device.device.fetch_events()?.for_each(drop);
            device.grab()?;
            return Ok(());
        }
        let events: Vec<InputEvent> = device.device.fetch_events()?.collect();
        for event in events {
            match (event.event_type().0, event.code()) {
                (EV_SYN, SYN_REPORT) if device.dropping => device.dropping = false,
                (EV_SYN, SYN_REPORT) => self.input.frame(device),
                (EV_SYN, SYN_DROPPED) => {
                    // The kernel's buffer overflowed. Rare; resynchronizing is not worth it.
                    device.frame.clear();
                    device.dropping = true;
                }
                (EV_SYN, _) => {}
                _ if !device.dropping => device.frame.push(event),
                _ => {}
            }
        }
        if std::mem::take(&mut self.input.toggled) {
            self.publish_lock();
        } else if !self.input.mouse.watching(self.input.keyboard.locked())
            && self.devices.iter().any(|d| !d.keyboard)
        {
            // The last blocked button was released: let go of the mouse.
            self.update_pointers();
        }
        Ok(())
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Locked(locked) => {
                self.input.keyboard.set_locked(locked);
                self.publish_lock();
            }
            Command::Shortcut(shortcut) => self.input.keyboard.set_shortcut(shortcut),
            Command::ShortcutUnlocks(unlocks) => self.input.keyboard.set_shortcut_unlocks(unlocks),
            Command::Recording(recording) => self.input.keyboard.set_recording(recording),
            Command::MouseBlock(block) => {
                self.input.mouse.set_block(block);
                self.update_pointers();
            }
            Command::Session(SessionEvent::Interrupt) => self.system_interrupt(),
            Command::Session(SessionEvent::Active(active)) if active != self.active => {
                self.system_interrupt();
                self.active = active;
                if active {
                    self.scan();
                } else {
                    // Another user is at the screen: hand the keyboard back entirely.
                    self.devices.clear();
                    self.pointers = false;
                }
            }
            Command::Session(SessionEvent::Active(_)) | Command::Quit => {}
        }
    }

    /// Screen lock, logout or sleep: resume and forget held keys, because their releases
    /// may never arrive.
    fn system_interrupt(&mut self) {
        self.input.keyboard.set_recording(false);
        self.input.keyboard.reset_physical_state();
        self.input.mouse.reset_physical_state();
        for device in &mut self.devices {
            device.pressed = Default::default();
        }
        self.input.keyboard.set_locked(false);
        self.publish_lock();
    }
}

fn is_event_node(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("event"))
}

/// Watches `/dev/input` for plugged-in devices (and for permission changes, which arrive
/// right after a new device appears).
fn watch_input_dir() -> io::Result<OwnedFd> {
    // SAFETY: plain syscalls; the descriptor is owned right away.
    unsafe {
        let fd = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = OwnedFd::from_raw_fd(fd);
        let dir = c"/dev/input";
        if libc::inotify_add_watch(fd.as_raw_fd(), dir.as_ptr(), libc::IN_CREATE | libc::IN_ATTRIB) < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(fd)
    }
}

/// Names of the event nodes reported by the inotify descriptor.
fn read_input_dir_changes(fd: RawFd) -> HashSet<PathBuf> {
    let mut names = HashSet::new();
    let mut buffer = [0u8; 4096];
    loop {
        // SAFETY: reads into a buffer we own.
        let len = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
        let Ok(len) = usize::try_from(len) else { break };
        if len == 0 {
            break;
        }
        let mut offset = 0;
        let header = size_of::<libc::inotify_event>();
        while offset + header <= len {
            // SAFETY: the kernel wrote a whole inotify_event at `offset`.
            let event = unsafe { buffer.as_ptr().add(offset).cast::<libc::inotify_event>().read_unaligned() };
            let name = &buffer[offset + header..(offset + header + event.len as usize).min(len)];
            let name = name.split(|&b| b == 0).next().unwrap_or_default();
            if let Ok(name) = std::str::from_utf8(name) {
                let path = Path::new(INPUT_DIR).join(name);
                if is_event_node(&path) {
                    names.insert(path);
                }
            }
            offset += header + event.len as usize;
        }
    }
    names
}

fn run(
    options: BlockerOptions,
    on_event: Box<dyn Fn(BlockerEvent) + Send>,
    locked: Arc<AtomicBool>,
    commands: Receiver<Command>,
    mut woken: UnixStream,
    ready: Sender<Result<(), Error>>,
) {
    // Without uinput, Keyless could only drop keys, never let them through.
    if let Err(e) = evdev::uinput::VirtualDevice::builder() {
        let _ = ready.send(Err(Error::Os(format!("no access to /dev/uinput ({e}). {SETUP_HELP}"))));
        return;
    }
    let inotify = match watch_input_dir() {
        Ok(fd) => fd,
        Err(e) => {
            let _ = ready.send(Err(Error::Os(format!("cannot watch /dev/input ({e})"))));
            return;
        }
    };
    let mut keyboard = Keyboard::new();
    keyboard.set_shortcut(options.shortcut);
    keyboard.set_shortcut_unlocks(options.shortcut_unlocks);
    let mut state = State {
        input: Input { keyboard, mouse: Mouse::new(), on_event, toggled: false },
        devices: Vec::new(),
        locked,
        pointers: false,
        active: true,
    };
    let denied = state.scan();
    if denied > 0 && !state.devices.iter().any(|d| d.keyboard) {
        let _ = ready.send(Err(Error::Os(format!("no access to the keyboards in /dev/input. {SETUP_HELP}"))));
        return;
    }
    let _ = ready.send(Ok(()));

    let mut fds: Vec<libc::pollfd> = Vec::new();
    let mut polled: Vec<PathBuf> = Vec::new();
    'run: loop {
        fds.clear();
        polled.clear();
        let watch = |fd: RawFd| libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        fds.push(watch(woken.as_raw_fd()));
        fds.push(watch(inotify.as_raw_fd()));
        for device in &state.devices {
            fds.push(watch(device.device.as_raw_fd()));
            polled.push(device.path.clone());
        }
        // SAFETY: `fds` is a valid array of pollfd for the duration of the call.
        let count = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if count < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }

        if fds[0].revents != 0 {
            let _ = woken.read(&mut [0u8; 64]);
            loop {
                match commands.try_recv() {
                    Ok(Command::Quit) | Err(mpsc::TryRecvError::Disconnected) => break 'run,
                    Ok(command) => state.command(command),
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }
        }
        for (fd, path) in fds[2..].iter().zip(&polled) {
            if fd.revents == 0 {
                continue;
            }
            let Some(index) = state.devices.iter().position(|d| &d.path == path) else { continue };
            let gone = fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0;
            if gone || state.read(index).is_err() {
                state.remove(index);
            }
        }
        if fds[1].revents != 0 {
            for path in read_input_dir_changes(inotify.as_raw_fd()) {
                let _ = state.add(&path);
            }
        }
    }

    state.input.keyboard.set_locked(false);
    state.locked.store(false, Ordering::Release);
    // Closing the devices ends the grabs; removing the copies releases what they held.
    state.devices.clear();
}
