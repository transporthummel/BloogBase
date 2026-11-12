//! Which game controller is connected, for what the frontend shows and the
//! layout the DOSBox mapper file binds (see `keymap`).
//!
//! On Windows, XInput answers at once, as the Web Gamepad API (a pad only
//! shows after a button was pressed in the window) and Windows.Gaming.Input
//! (through gilrs, empty until its events arrived) don't. It sees Xbox
//! controllers and others that act as one. PlayStation controllers are found
//! by their HID vendor and product IDs through Raw Input. Other pads count
//! as none. Elsewhere, there's no answer yet.
//!
//! The player can set the layout in the settings, for pads found wrongly or
//! not at all; it then counts for every connected controller.

use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::Result;
use crate::settings;

/// A controller's button layout, as DOSBox's SDL2 numbers its inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Controller {
    /// Xbox controllers and pads that act as one (XInput).
    Xbox,
    /// DualShock 4 and DualSense, which SDL2 reads itself (HIDAPI).
    Playstation,
}

/// The layout the player chose; unset means found by itself.
const CHOSEN: &str = "controller.layout";

/// The layout the player chose, or none to find it.
pub fn chosen(db: &Db) -> Result<Option<Controller>> {
    Ok(match settings::get(db, CHOSEN)?.as_deref() {
        Some("xbox") => Some(Controller::Xbox),
        Some("playstation") => Some(Controller::Playstation),
        _ => None,
    })
}

pub fn set_chosen(db: &Db, layout: Option<Controller>) -> Result<()> {
    match layout {
        Some(Controller::Xbox) => settings::set(db, CHOSEN, "xbox"),
        Some(Controller::Playstation) => settings::set(db, CHOSEN, "playstation"),
        None => settings::delete(db, CHOSEN),
    }
}

/// The connected controller's layout as the player chose it or as found;
/// none without a controller (as far as can be told).
pub fn connected_as_chosen(db: &Db) -> Option<Controller> {
    let found = connected()?;
    Some(chosen(db).ok().flatten().unwrap_or(found))
}

/// The layout a game's mapper file binds: the chosen one, else the found
/// one, else an Xbox controller's (where nothing can be found).
pub fn for_game(db: &Db) -> Controller {
    match chosen(db) {
        Ok(Some(layout)) => layout,
        _ => connected().unwrap_or(Controller::Xbox),
    }
}

/// Sony's USB vendor ID.
#[cfg_attr(not(windows), allow(dead_code))]
const SONY: u32 = 0x054C;

/// DualShock 4 (both versions and the wireless adapter), DualSense and
/// DualSense Edge: the pads SDL2's HIDAPI drivers take on by default.
#[cfg_attr(not(windows), allow(dead_code))]
const PLAYSTATION: &[u32] = &[0x05C4, 0x09CC, 0x0BA0, 0x0CE6, 0x0DF2];

/// The connected controller DOSBox takes as its first: SDL2 lists the pads
/// it reads through HIDAPI before XInput's, so a PlayStation controller
/// comes first.
#[cfg(windows)]
pub fn connected() -> Option<Controller> {
    if playstation() {
        Some(Controller::Playstation)
    } else if xinput() {
        Some(Controller::Xbox)
    } else {
        None
    }
}

#[cfg(not(windows))]
pub fn connected() -> Option<Controller> {
    None
}

#[cfg(windows)]
fn xinput() -> bool {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::UI::Input::XboxController::{XINPUT_STATE, XInputGetState};

    // XInput has four slots.
    (0..4).any(|slot| {
        // SAFETY: XInputGetState only writes into the state we pass.
        let mut state: XINPUT_STATE = unsafe { std::mem::zeroed() };
        unsafe { XInputGetState(slot, &mut state) == ERROR_SUCCESS }
    })
}

/// Whether Raw Input lists a PlayStation controller.
#[cfg(windows)]
fn playstation() -> bool {
    use windows_sys::Win32::UI::Input::{
        GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
        RIDI_DEVICEINFO, RIM_TYPEHID,
    };

    let entry = size_of::<RAWINPUTDEVICELIST>() as u32;
    let mut count = 0u32;
    // SAFETY: a null list asks only for the count.
    if unsafe { GetRawInputDeviceList(std::ptr::null_mut(), &mut count, entry) } == u32::MAX {
        return false;
    }
    // Room for devices plugged in between the two calls.
    let mut devices = vec![RAWINPUTDEVICELIST::default(); count as usize + 4];
    let mut room = devices.len() as u32;
    // SAFETY: the list has `room` entries of `entry` bytes.
    let found = unsafe { GetRawInputDeviceList(devices.as_mut_ptr(), &mut room, entry) };
    if found == u32::MAX {
        return false;
    }
    devices.truncate(found as usize);

    devices.iter().filter(|d| d.dwType == RIM_TYPEHID).any(|d| {
        let mut info = RID_DEVICE_INFO {
            cbSize: size_of::<RID_DEVICE_INFO>() as u32,
            ..Default::default()
        };
        let mut size = info.cbSize;
        // SAFETY: `info` is as large as `size` says, with cbSize set.
        let read = unsafe {
            GetRawInputDeviceInfoW(
                d.hDevice,
                RIDI_DEVICEINFO,
                (&mut info as *mut RID_DEVICE_INFO).cast(),
                &mut size,
            )
        };
        if read == u32::MAX || info.dwType != RIM_TYPEHID {
            return false;
        }
        // SAFETY: a HID device's info fills the `hid` variant.
        let hid = unsafe { info.Anonymous.hid };
        hid.dwVendorId == SONY && PLAYSTATION.contains(&hid.dwProductId)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chosen_layout_is_kept_and_cleared() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(chosen(&db).unwrap(), None);
        set_chosen(&db, Some(Controller::Playstation)).unwrap();
        assert_eq!(chosen(&db).unwrap(), Some(Controller::Playstation));
        assert_eq!(for_game(&db), Controller::Playstation);
        set_chosen(&db, None).unwrap();
        assert_eq!(chosen(&db).unwrap(), None);
    }

    /// Run with a controller connected: prints what `connected` finds.
    #[test]
    #[ignore]
    fn print_connected() {
        println!("{:?}", super::connected());
    }
}
