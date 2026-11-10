//! Whether a game controller is connected, for what the frontend shows. The
//! games get it through DOSBox either way (see `keymap`).
//!
//! On Windows, XInput answers at once, as the Web Gamepad API (a pad only
//! shows after a button was pressed in the window) and Windows.Gaming.Input
//! (through gilrs, empty until its events arrived) don't. It sees Xbox
//! controllers and others that act as one, not every pad DOSBox would.
//! Elsewhere, there's no answer yet.

/// Whether a controller is connected now.
#[cfg(windows)]
pub fn connected() -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

    // XInput has four slots.
    (0..4).any(|slot| {
        // SAFETY: XInputGetState only writes into the state we pass.
        let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
        unsafe { XInputGetState(slot, &mut state) == ERROR_SUCCESS }
    })
}

#[cfg(not(windows))]
pub fn connected() -> bool {
    false
}
