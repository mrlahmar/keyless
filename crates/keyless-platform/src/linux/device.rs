//! One input device under `/dev/input`, and the virtual copy that stands in for it while
//! Keyless holds it.

use std::io;
use std::path::{Path, PathBuf};

use evdev::raw_stream::RawDevice;
use evdev::uinput::VirtualDevice;
use evdev::{AbsoluteAxisCode, InputEvent, KeyCode, PropType, RelativeAxisCode, UinputAbsSetup};
use keyless_core::APP_NAME;

use super::keys::KEY_COUNT;

/// Room for every key code, one bit each.
#[derive(Clone, Default)]
pub struct KeyBits([u64; KEY_COUNT as usize / 64]);

impl KeyBits {
    pub fn get(&self, code: u16) -> bool {
        code < KEY_COUNT && self.0[usize::from(code) / 64] & (1 << (code % 64)) != 0
    }
    pub fn set(&mut self, code: u16, value: bool) {
        if code >= KEY_COUNT {
            return;
        }
        let word = &mut self.0[usize::from(code) / 64];
        if value { *word |= 1 << (code % 64) } else { *word &= !(1 << (code % 64)) }
    }
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&word| word == 0)
    }
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        (0..KEY_COUNT).filter(|&code| self.get(code))
    }
}

/// A keyboard or a pointing device, opened by Keyless.
pub struct Device {
    pub path: PathBuf,
    pub device: RawDevice,
    /// Has letter keys. Keyboards are held for as long as Keyless runs.
    pub keyboard: bool,
    /// A mouse, trackpoint or touchpad. Held only while the mouse is paused.
    pub pointer: bool,
    /// Stands in for the device while it is grabbed. Created on the first grab.
    pub copy: Option<VirtualDevice>,
    /// Events of the frame being read, up to the next `SYN_REPORT`.
    pub frame: Vec<InputEvent>,
    /// Events of the frame that pass, reused between frames.
    pub out: Vec<InputEvent>,
    /// The kernel dropped events: ignore everything up to the next `SYN_REPORT`.
    pub dropping: bool,
    /// Keys and buttons physically down, as Keyless has seen them.
    pub pressed: KeyBits,
    /// Keys and buttons the copy has pressed. Their releases always go to the copy.
    pub held: KeyBits,
}

impl Device {
    /// Opens a keyboard or pointing device. `Ok(None)` for every other device, and for
    /// virtual ones: Keyless leaves software-sent input alone, its own copies included.
    pub fn open(path: &Path) -> io::Result<Option<Self>> {
        if is_virtual(path) {
            return Ok(None);
        }
        let device = RawDevice::open(path)?;
        let keys = device.supported_keys();
        let has_key = |key| keys.is_some_and(|keys| keys.contains(key));
        let keyboard = has_key(KeyCode::KEY_A) && has_key(KeyCode::KEY_Z);
        let rel = device.supported_relative_axes();
        let abs = device.supported_absolute_axes();
        let mouse = rel
            .is_some_and(|r| r.contains(RelativeAxisCode::REL_X) && r.contains(RelativeAxisCode::REL_Y))
            && has_key(KeyCode::BTN_LEFT);
        // Touchpads, and mice that report where the pointer is rather than how far it
        // moved: virtual machines (VirtualBox, VMware, QEMU tablets) and KVM switches.
        // Touchscreens and drawing tablets are not mice: leave them alone.
        let absolute = abs
            .is_some_and(|a| a.contains(AbsoluteAxisCode::ABS_X) && a.contains(AbsoluteAxisCode::ABS_Y))
            && (has_key(KeyCode::BTN_TOOL_FINGER) || has_key(KeyCode::BTN_LEFT))
            && !has_key(KeyCode::BTN_TOOL_PEN)
            && !has_key(KeyCode::BTN_STYLUS)
            && !device.properties().contains(PropType::DIRECT);
        let pointer = mouse || absolute;
        if !keyboard && !pointer {
            return Ok(None);
        }
        Ok(Some(Self {
            path: path.to_owned(),
            device,
            keyboard,
            pointer,
            copy: None,
            frame: Vec::new(),
            out: Vec::new(),
            dropping: false,
            pressed: KeyBits::default(),
            held: KeyBits::default(),
        }))
    }

    pub fn grabbed(&self) -> bool {
        self.device.is_grabbed()
    }

    /// Takes the device over: from now on its events reach Keyless only, and those that
    /// pass come out of the copy. Returns `false` while keys or buttons are held, because
    /// apps saw those presses from the device itself and would never see their release.
    /// Call again after the next events.
    pub fn grab(&mut self) -> io::Result<bool> {
        if self.copy.is_none() {
            self.copy = Some(make_copy(&self.device)?);
        }
        self.device.grab()?;
        // Checked after grabbing, so no press can slip in between.
        if self.device.get_key_state()?.iter().next().is_some() {
            self.device.ungrab()?;
            return Ok(false);
        }
        self.frame.clear();
        self.dropping = false;
        self.pressed = KeyBits::default();
        Ok(true)
    }
}

/// Devices created by software (uinput), such as on-screen keyboards, remappers and
/// Keyless's own copies, live under `/sys/devices/virtual`.
fn is_virtual(path: &Path) -> bool {
    let Some(name) = path.file_name() else { return true };
    std::fs::canonicalize(Path::new("/sys/class/input").join(name))
        .map(|sys| sys.components().any(|part| part.as_os_str() == "virtual"))
        .unwrap_or(false)
}

/// A virtual device with the same keys, axes and properties, so the desktop treats it
/// like the original (keymap, touchpad gestures, scrolling).
fn make_copy(device: &RawDevice) -> io::Result<VirtualDevice> {
    let mut name = format!("{APP_NAME} {}", device.name().unwrap_or("input"));
    // uinput names hold at most 79 bytes.
    while name.len() > 70 {
        name.pop();
    }
    let mut builder = VirtualDevice::builder()?.name(&name).input_id(device.input_id());
    if let Some(keys) = device.supported_keys() {
        builder = builder.with_keys(keys)?;
    }
    if let Some(axes) = device.supported_relative_axes() {
        builder = builder.with_relative_axes(axes)?;
    }
    if device.supported_absolute_axes().is_some() {
        for (axis, info) in device.get_absinfo()? {
            builder = builder.with_absolute_axis(&UinputAbsSetup::new(axis, info))?;
        }
    }
    if let Some(misc) = device.misc_properties() {
        builder = builder.with_msc(misc)?;
    }
    if let Some(switches) = device.supported_switches() {
        builder = builder.with_switches(switches)?;
    }
    builder.with_properties(device.properties())?.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_bits() {
        let mut bits = KeyBits::default();
        assert!(bits.is_empty());
        bits.set(30, true);
        bits.set(0x110, true);
        bits.set(KEY_COUNT, true); // out of range: ignored
        assert!(bits.get(30) && bits.get(0x110) && !bits.get(31));
        assert_eq!(bits.iter().collect::<Vec<_>>(), [30, 0x110]);
        bits.set(30, false);
        bits.set(0x110, false);
        assert!(bits.is_empty());
    }
}
