//! What the mouse does while the keyboard is paused, and the per-event decision.
//!
//! Like [`crate::Keyboard`], this runs inside the platform's input hook, so it does no I/O,
//! no allocation and no locking. A button press that was blocked has its release blocked
//! too; a release whose press reached applications always gets through, so no button or
//! drag ever stays stuck.

use serde::{Deserialize, Serialize};

/// How much of the mouse pauses with the keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MouseBlock {
    /// The mouse keeps working.
    #[default]
    Off,
    /// Clicks and scrolling are blocked; the pointer still moves.
    Clicks,
    /// Clicks, scrolling and pointer movement are blocked.
    Cursor,
}

impl MouseBlock {
    /// Order used by the UI's choice control.
    pub const ALL: [Self; 3] = [Self::Off, Self::Clicks, Self::Cursor];
}

/// One physical mouse event, already classified by the platform layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEvent {
    Move,
    /// Vertical or horizontal wheel.
    Wheel,
    /// `button`: 0 left, 1 right, 2 middle, 3 and 4 the side buttons.
    Button {
        button: u8,
        down: bool,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Mouse {
    block: MouseBlock,
    /// Buttons whose press was blocked, one bit each.
    swallowed: u8,
}

impl Mouse {
    pub const fn new() -> Self {
        Self { block: MouseBlock::Off, swallowed: 0 }
    }

    pub fn block(&self) -> MouseBlock {
        self.block
    }
    pub fn set_block(&mut self, block: MouseBlock) {
        self.block = block;
    }

    /// The platform needs to watch the mouse: while paused with blocking on, or while a
    /// blocked press still waits for its release.
    pub fn watching(&self, paused: bool) -> bool {
        (paused && self.block != MouseBlock::Off) || self.swallowed != 0
    }

    /// Forget blocked presses. Call after sleep or session switches, when releases may
    /// have been missed.
    pub fn reset_physical_state(&mut self) {
        self.swallowed = 0;
    }

    /// Whether the event must not reach applications.
    pub fn event(&mut self, paused: bool, event: MouseEvent) -> bool {
        let blocking = paused && self.block != MouseBlock::Off;
        match event {
            MouseEvent::Move => paused && self.block == MouseBlock::Cursor,
            MouseEvent::Wheel => blocking,
            MouseEvent::Button { button, down } => {
                let bit = 1u8.checked_shl(u32::from(button)).unwrap_or(0);
                if down {
                    if blocking {
                        self.swallowed |= bit;
                    }
                    blocking
                } else {
                    let swallowed = self.swallowed & bit != 0;
                    self.swallowed &= !bit;
                    swallowed
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT_DOWN: MouseEvent = MouseEvent::Button { button: 0, down: true };
    const LEFT_UP: MouseEvent = MouseEvent::Button { button: 0, down: false };

    fn mouse(block: MouseBlock) -> Mouse {
        let mut m = Mouse::new();
        m.set_block(block);
        m
    }

    #[test]
    fn off_never_blocks() {
        let mut m = mouse(MouseBlock::Off);
        assert!(!m.watching(true));
        for event in [MouseEvent::Move, MouseEvent::Wheel, LEFT_DOWN, LEFT_UP] {
            assert!(!m.event(true, event));
        }
    }

    #[test]
    fn clicks_blocks_buttons_and_wheel_but_not_movement() {
        let mut m = mouse(MouseBlock::Clicks);
        assert!(m.watching(true));
        assert!(!m.watching(false));
        assert!(!m.event(true, MouseEvent::Move));
        assert!(m.event(true, MouseEvent::Wheel));
        assert!(m.event(true, LEFT_DOWN));
        assert!(m.event(true, LEFT_UP));
        assert!(!m.event(false, MouseEvent::Wheel));
    }

    #[test]
    fn cursor_blocks_movement_too() {
        let mut m = mouse(MouseBlock::Cursor);
        assert!(m.event(true, MouseEvent::Move));
        assert!(!m.event(false, MouseEvent::Move));
    }

    #[test]
    fn press_before_pausing_keeps_its_release() {
        let mut m = mouse(MouseBlock::Clicks);
        assert!(!m.event(false, LEFT_DOWN));
        assert!(!m.event(true, LEFT_UP), "a drag started before pausing can finish");
    }

    #[test]
    fn blocked_press_keeps_its_release_blocked_after_resuming() {
        let mut m = mouse(MouseBlock::Clicks);
        assert!(m.event(true, LEFT_DOWN));
        assert!(m.watching(false), "still waiting for the release");
        assert!(m.event(false, LEFT_UP));
        assert!(!m.watching(false));
        assert!(!m.event(false, LEFT_DOWN), "a fresh click works again");
    }

    #[test]
    fn reset_forgets_blocked_presses() {
        let mut m = mouse(MouseBlock::Clicks);
        m.event(true, MouseEvent::Button { button: 4, down: true });
        m.reset_physical_state();
        assert!(!m.watching(false));
    }
}
