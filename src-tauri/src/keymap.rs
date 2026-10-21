//! Letter keys on non-US keyboard layouts. Keen and most DOS games read raw
//! scancodes, so DOSBox's `keyboard_layout` doesn't reach them: on a German
//! keyboard the key labelled Y arrives as Z. A DOSBox mapper file that binds
//! each emulated letter to the host key that types it fixes that.
//!
//! A mapper file replaces all of DOSBox's default bindings, so it repeats
//! them (DOSBox Staging 0.83, `CreateDefaultBinds` in `gui/mapper.cpp`) and
//! only moves the letters. Only written when the layout moves a letter, so
//! US layouts keep DOSBox's own defaults.

use std::path::Path;

use crate::dosconf::write_if_changed;
use crate::error::Result;

/// The mapper file's name, next to the options layer that names it.
pub const MAPPERFILE: &str = "keyboard.map";

/// DOSBox's emulated keys and their default host key (SDL scancode).
#[rustfmt::skip]
const KEYS: &[(&str, u16)] = &[
    ("f1", 58), ("f2", 59), ("f3", 60), ("f4", 61), ("f5", 62), ("f6", 63),
    ("f7", 64), ("f8", 65), ("f9", 66), ("f10", 67), ("f11", 68), ("f12", 69),
    ("1", 30), ("2", 31), ("3", 32), ("4", 33), ("5", 34), ("6", 35),
    ("7", 36), ("8", 37), ("9", 38), ("0", 39),
    ("a", 4), ("b", 5), ("c", 6), ("d", 7), ("e", 8), ("f", 9), ("g", 10),
    ("h", 11), ("i", 12), ("j", 13), ("k", 14), ("l", 15), ("m", 16),
    ("n", 17), ("o", 18), ("p", 19), ("q", 20), ("r", 21), ("s", 22),
    ("t", 23), ("u", 24), ("v", 25), ("w", 26), ("x", 27), ("y", 28),
    ("z", 29),
    ("space", 44), ("esc", 41), ("equals", 46), ("grave", 53), ("tab", 43),
    ("enter", 40), ("bspace", 42), ("lbracket", 47), ("rbracket", 48),
    ("minus", 45), ("capslock", 57), ("semicolon", 51), ("quote", 52),
    ("backslash", 49), ("lshift", 225), ("rshift", 229), ("lalt", 226),
    ("ralt", 230), ("lctrl", 224), ("rctrl", 228), ("lgui", 227),
    ("rgui", 231), ("comma", 54), ("period", 55), ("slash", 56),
    ("printscreen", 70), ("scrolllock", 71), ("pause", 72), ("pagedown", 78),
    ("pageup", 75), ("insert", 73), ("home", 74), ("delete", 76), ("end", 77),
    ("up", 82), ("left", 80), ("down", 81), ("right", 79),
    ("kp_1", 89), ("kp_2", 90), ("kp_3", 91), ("kp_4", 92), ("kp_5", 93),
    ("kp_6", 94), ("kp_7", 95), ("kp_8", 96), ("kp_9", 97), ("kp_0", 98),
    ("numlock", 83), ("kp_divide", 84), ("kp_multiply", 85), ("kp_minus", 86),
    ("kp_plus", 87), ("kp_period", 99), ("kp_enter", 88),
    ("oem102", 100), ("abnt1", 135),
];

/// Everything after the keys, as DOSBox Staging 0.83 binds it on Windows and
/// Linux. Hotkeys of machines BloogBase doesn't emulate are left out.
const REST: &str = "\
mod_1 \"key 228\" \"key 224\"
mod_2 \"key 230\" \"key 226\"
mod_3 \"key 231\" \"key 227\"
hand_mapper \"key 58 mod1\"
hand_shutdown \"key 66 mod1\"
hand_fullscr \"key 40 mod2\"
hand_restart \"key 74 mod1 mod2\"
hand_capmouse \"key 67 mod1\"
hand_pause \"key 72 mod2\"
hand_mute \"key 65 mod1\"
hand_speedlock \"key 69 mod2\"
hand_cycledown \"key 68 mod1\"
hand_cycleup \"key 69 mod1\"
hand_reloadshader \"key 59 mod1\"
hand_previmageadj \"key 66 mod3\"
hand_nextimageadj \"key 67 mod3\"
hand_decimageadj \"key 68 mod3\"
hand_incimageadj \"key 69 mod3\"
hand_recwave \"key 63 mod1\"
hand_caprawmidi \"key 63 mod1 mod2\"
hand_scrshot \"key 62 mod1\"
hand_rendshot \"key 62 mod2\"
hand_video \"key 64 mod1\"
hand_swapimg \"key 61 mod1\"
jbutton_0_0 \"stick_0 button 0\"
jbutton_0_1 \"stick_0 button 1\"
jbutton_0_2 \"stick_0 button 2\"
jbutton_0_3 \"stick_0 button 3\"
jbutton_0_4 \"stick_0 button 4\"
jbutton_0_5 \"stick_0 button 5\"
jbutton_1_0 \"stick_1 button 0\"
jbutton_1_1 \"stick_1 button 1\"
jaxis_0_0- \"stick_0 axis 0 0\"
jaxis_0_0+ \"stick_0 axis 0 1\"
jaxis_0_1- \"stick_0 axis 1 0\"
jaxis_0_1+ \"stick_0 axis 1 1\"
jaxis_0_2- \"stick_0 axis 2 0\"
jaxis_0_2+ \"stick_0 axis 2 1\"
jaxis_0_3- \"stick_0 axis 3 0\"
jaxis_0_3+ \"stick_0 axis 3 1\"
jaxis_1_0- \"stick_1 axis 0 0\"
jaxis_1_0+ \"stick_1 axis 0 1\"
jaxis_1_1- \"stick_1 axis 1 0\"
jaxis_1_1+ \"stick_1 axis 1 1\"
jhat_0_0_0 \"stick_0 hat 0 1\"
jhat_0_0_1 \"stick_0 hat 0 2\"
jhat_0_0_2 \"stick_0 hat 0 4\"
jhat_0_0_3 \"stick_0 hat 0 8\"
";

/// The mapper file for a layout, given the host key (SDL scancode) of each
/// letter that has one; `None` if no letter moves. Each letter is swapped
/// onto its host key, so the key it pushes out takes the letter's old place
/// and every host key still sends exactly one emulated key.
pub fn mapper(letters: &[(char, u16)]) -> Option<String> {
    let mut keys: Vec<(&str, u16)> = KEYS.to_vec();
    let mut moved = false;
    for &(letter, host) in letters {
        let name = letter.to_ascii_lowercase().to_string();
        let Some(i) = keys.iter().position(|(n, _)| *n == name) else {
            continue;
        };
        let Some(j) = keys.iter().position(|(_, h)| *h == host) else {
            continue;
        };
        if i != j {
            let (a, b) = (keys[i].1, keys[j].1);
            keys[i].1 = b;
            keys[j].1 = a;
            moved = true;
        }
    }
    if !moved {
        return None;
    }
    let mut out = String::new();
    for (name, host) in keys {
        out.push_str(&format!("key_{name} \"key {host}\"\n"));
    }
    out.push_str(REST);
    Some(out)
}

/// Writes the mapper file for the host's keyboard layout into `dir`, or
/// removes it if the layout needs none. Returns whether DOSBox should use it.
pub fn ensure(dir: &Path) -> Result<bool> {
    let path = dir.join(MAPPERFILE);
    match host_letters().and_then(|l| mapper(&l)) {
        Some(content) => {
            write_if_changed(&path, &content)?;
            Ok(true)
        }
        None => {
            let _ = std::fs::remove_file(&path);
            Ok(false)
        }
    }
}

/// The host key of each letter on the active keyboard layout.
#[cfg(windows)]
fn host_letters() -> Option<Vec<(char, u16)>> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MAPVK_VK_TO_VSC, MapVirtualKeyExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    // SAFETY: plain queries without pointers we own; a null window gives
    // thread 0, whose layout is the default one.
    let layout = unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        GetKeyboardLayout(thread)
    };
    let letters: Vec<(char, u16)> = ('A'..='Z')
        .filter_map(|c| {
            // Letter virtual keys are their uppercase ASCII codes.
            let scan = unsafe { MapVirtualKeyExW(c as u32, MAPVK_VK_TO_VSC, layout) };
            sdl_scancode(scan).map(|s| (c, s))
        })
        .collect();
    Some(letters)
}

#[cfg(not(windows))]
fn host_letters() -> Option<Vec<(char, u16)>> {
    None
}

/// The SDL scancode of a PC (set 1) scancode in the typing area.
#[cfg_attr(not(windows), allow(dead_code))]
fn sdl_scancode(pc: u32) -> Option<u16> {
    const ROWS: &[(u32, &[u16])] = &[
        (0x02, &[30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 45, 46]),
        (0x10, &[20, 26, 8, 21, 23, 28, 24, 12, 18, 19, 47, 48]),
        (0x1E, &[4, 22, 7, 9, 10, 11, 13, 14, 15, 51, 52, 53]),
        (0x2B, &[49, 29, 27, 6, 25, 5, 17, 16, 54, 55, 56]),
        (0x56, &[100]),
    ];
    ROWS.iter().find_map(|(start, keys)| {
        pc.checked_sub(*start)
            .and_then(|i| keys.get(i as usize).copied())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding<'a>(map: &'a str, event: &str) -> &'a str {
        map.lines()
            .find_map(|l| l.strip_prefix(&format!("{event} ")))
            .unwrap()
    }

    #[test]
    fn us_layout_needs_no_mapper() {
        let us: Vec<(char, u16)> = ('a'..='z').zip(4..).collect();
        assert_eq!(mapper(&us), None);
    }

    #[test]
    fn german_layout_swaps_y_and_z() {
        let map = mapper(&[('Y', 29), ('Z', 28), ('A', 4)]).unwrap();
        assert_eq!(binding(&map, "key_y"), "\"key 29\"");
        assert_eq!(binding(&map, "key_z"), "\"key 28\"");
        assert_eq!(binding(&map, "key_a"), "\"key 4\"");
        assert_eq!(binding(&map, "hand_shutdown"), "\"key 66 mod1\"");
    }

    #[test]
    fn french_layout_keeps_every_host_key_bound_once() {
        // AZERTY: A and Q, Z and W swap; M sits where US has the semicolon.
        let map = mapper(&[('A', 20), ('Q', 4), ('Z', 26), ('W', 29), ('M', 51)]).unwrap();
        assert_eq!(binding(&map, "key_m"), "\"key 51\"");
        assert_eq!(binding(&map, "key_semicolon"), "\"key 16\"");
        let hosts: Vec<&str> = map
            .lines()
            .filter(|l| l.starts_with("key_"))
            .map(|l| l.split_once(' ').unwrap().1)
            .collect();
        let mut unique = hosts.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), hosts.len());
    }

    #[test]
    fn pc_scancodes_map_to_sdl() {
        assert_eq!(sdl_scancode(0x15), Some(28)); // Y
        assert_eq!(sdl_scancode(0x2C), Some(29)); // Z
        assert_eq!(sdl_scancode(0x27), Some(51)); // semicolon
        assert_eq!(sdl_scancode(0x29), Some(53)); // grave
        assert_eq!(sdl_scancode(0x01), None);
    }
}

