//! Letter keys on non-US keyboard layouts. Keen and most DOS games read raw
//! scancodes, so DOSBox's `keyboard_layout` doesn't reach them: on a German
//! keyboard the key labelled Y arrives as Z. A DOSBox mapper file that binds
//! each emulated letter to the host key that types it fixes that.
//!
//! A mapper file replaces all of DOSBox's default bindings, so it repeats
//! them (DOSBox Staging 0.83, `CreateDefaultBinds` in `gui/mapper.cpp`) and
//! only moves the letters.
//!
//! It also puts an Xbox controller on the keys the game plays with, so games
//! need no joystick setup or calibration, and on its menu keys while View
//! is held. Those keys are the game's own (its settings file, with the
//! global settings), so each profile has a mapper file of its own, written
//! at every start.

use crate::catalog::Engine;
use crate::dosconf::write_if_changed;
use crate::error::Result;
use crate::game_settings::ActionKeys;
use crate::paths::AppPaths;

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

/// Controller inputs added to DOSBox's events, as SDL numbers an Xbox
/// controller over Bluetooth: buttons A 0, B 1, X 2, Y 3, LB 4, RB 5,
/// View 6, Menu 7, left stick click 8, right stick click 9; axes left stick
/// 0 and 1, right stick 2 and 3, triggers 4 and 5 (at rest at the negative
/// end); the D-pad is hat 0 (up 1, right 2, down 4, left 8).
///
/// Each menu key has a button of its own: Keen answers a question with the
/// last key pressed, so a button pressing two keys only gives the second.
type Pad = &'static [(&'static str, &'static [&'static str])];

/// Menus in Keen 1-3, with View held.
const MENU_VORTICONS: Pad = &[
    // View+A: confirm; View+B: the title screen.
    ("key_enter", &["stick_0 button 0 mod3"]),
    ("key_t", &["stick_0 button 1 mod3"]),
    // View+X: no; View+Y: yes.
    ("key_n", &["stick_0 button 2 mod3"]),
    ("key_y", &["stick_0 button 3 mod3"]),
    // View+Menu: quit to DOS.
    ("key_d", &["stick_0 button 7 mod3"]),
    // View+RB: save; View+LB: sound.
    ("key_f5", &["stick_0 button 5 mod3"]),
    ("key_f2", &["stick_0 button 4 mod3"]),
    // Save slots. View+right stick up, right, down, left: 1-4; View+D-pad:
    // 5-8; View+right stick click: 9.
    ("key_1", &["stick_0 axis 3 0 mod3"]),
    ("key_2", &["stick_0 axis 2 1 mod3"]),
    ("key_3", &["stick_0 axis 3 1 mod3"]),
    ("key_4", &["stick_0 axis 2 0 mod3"]),
    ("key_5", &["stick_0 hat 0 1 mod3"]),
    ("key_6", &["stick_0 hat 0 2 mod3"]),
    ("key_7", &["stick_0 hat 0 4 mod3"]),
    ("key_8", &["stick_0 hat 0 8 mod3"]),
    ("key_9", &["stick_0 button 9 mod3"]),
];

/// Menus in Keen 4-6 and Keen Dreams, with View held.
const MENU_GALAXY: Pad = &[
    // View+A: confirm; View+B: back.
    ("key_enter", &["stick_0 button 0 mod3"]),
    ("key_esc", &["stick_0 button 1 mod3"]),
    // View+X: no; View+Y: yes.
    ("key_n", &["stick_0 button 2 mod3"]),
    ("key_y", &["stick_0 button 3 mod3"]),
    // Letters for naming a save. View+LB is a second layer: LB pressed with
    // View held is DOSBox's first modifier (and not pogo).
    ("mod_1", &["stick_0 button 4 mod3"]),
    // View+D-pad up, right, down, left: A-D; View+LB: O-R.
    ("key_a", &["stick_0 hat 0 1 mod3"]),
    ("key_b", &["stick_0 hat 0 2 mod3"]),
    ("key_c", &["stick_0 hat 0 4 mod3"]),
    ("key_d", &["stick_0 hat 0 8 mod3"]),
    ("key_o", &["stick_0 hat 0 1 mod1 mod3"]),
    ("key_p", &["stick_0 hat 0 2 mod1 mod3"]),
    ("key_q", &["stick_0 hat 0 4 mod1 mod3"]),
    ("key_r", &["stick_0 hat 0 8 mod1 mod3"]),
    // View+right stick up, right, down, left, click: E-I; View+LB: S-W.
    ("key_e", &["stick_0 axis 3 0 mod3"]),
    ("key_f", &["stick_0 axis 2 1 mod3"]),
    ("key_g", &["stick_0 axis 3 1 mod3"]),
    ("key_h", &["stick_0 axis 2 0 mod3"]),
    ("key_i", &["stick_0 button 9 mod3"]),
    ("key_s", &["stick_0 axis 3 0 mod1 mod3"]),
    ("key_t", &["stick_0 axis 2 1 mod1 mod3"]),
    ("key_u", &["stick_0 axis 3 1 mod1 mod3"]),
    ("key_v", &["stick_0 axis 2 0 mod1 mod3"]),
    ("key_w", &["stick_0 button 9 mod1 mod3"]),
    // View+left stick up, right, down, left, click: J-N; View+LB: X, Y, Z,
    // space, backspace.
    ("key_j", &["stick_0 axis 1 0 mod3"]),
    ("key_k", &["stick_0 axis 0 1 mod3"]),
    ("key_l", &["stick_0 axis 1 1 mod3"]),
    ("key_m", &["stick_0 axis 0 0 mod3"]),
    ("key_n", &["stick_0 button 8 mod3"]),
    ("key_x", &["stick_0 axis 1 0 mod1 mod3"]),
    ("key_y", &["stick_0 axis 0 1 mod1 mod3"]),
    ("key_z", &["stick_0 axis 1 1 mod1 mod3"]),
    ("key_space", &["stick_0 axis 0 0 mod1 mod3"]),
    ("key_bspace", &["stick_0 button 8 mod1 mod3"]),
];

/// The emulated key of each DOS scancode a game can be set to. The arrows
/// are the gray ones, the other keys of the number pad its own.
#[rustfmt::skip]
const SCANCODES: &[(u8, &str)] = &[
    (0x01, "esc"), (0x02, "1"), (0x03, "2"), (0x04, "3"), (0x05, "4"),
    (0x06, "5"), (0x07, "6"), (0x08, "7"), (0x09, "8"), (0x0A, "9"),
    (0x0B, "0"), (0x0C, "minus"), (0x0D, "equals"), (0x0E, "bspace"),
    (0x0F, "tab"), (0x10, "q"), (0x11, "w"), (0x12, "e"), (0x13, "r"),
    (0x14, "t"), (0x15, "y"), (0x16, "u"), (0x17, "i"), (0x18, "o"),
    (0x19, "p"), (0x1A, "lbracket"), (0x1B, "rbracket"), (0x1C, "enter"),
    (0x1D, "lctrl"), (0x1E, "a"), (0x1F, "s"), (0x20, "d"), (0x21, "f"),
    (0x22, "g"), (0x23, "h"), (0x24, "j"), (0x25, "k"), (0x26, "l"),
    (0x27, "semicolon"), (0x28, "quote"), (0x29, "grave"), (0x2A, "lshift"),
    (0x2B, "backslash"), (0x2C, "z"), (0x2D, "x"), (0x2E, "c"), (0x2F, "v"),
    (0x30, "b"), (0x31, "n"), (0x32, "m"), (0x33, "comma"), (0x34, "period"),
    (0x35, "slash"), (0x36, "rshift"), (0x37, "kp_multiply"), (0x38, "lalt"),
    (0x39, "space"), (0x3A, "capslock"), (0x3B, "f1"), (0x3C, "f2"),
    (0x3D, "f3"), (0x3E, "f4"), (0x3F, "f5"), (0x40, "f6"), (0x41, "f7"),
    (0x42, "f8"), (0x43, "f9"), (0x44, "f10"), (0x45, "numlock"),
    (0x46, "scrolllock"), (0x47, "kp_7"), (0x48, "up"), (0x49, "kp_9"),
    (0x4A, "kp_minus"), (0x4B, "left"), (0x4C, "kp_5"), (0x4D, "right"),
    (0x4E, "kp_plus"), (0x4F, "kp_1"), (0x50, "down"), (0x51, "kp_3"),
    (0x52, "kp_0"), (0x53, "kp_period"), (0x56, "oem102"), (0x57, "f11"),
    (0x58, "f12"),
];

/// DOSBox's event for the emulated key of a DOS scancode.
fn event(scancode: u8) -> Option<String> {
    SCANCODES
        .iter()
        .find(|(code, _)| *code == scancode)
        .map(|(_, name)| format!("key_{name}"))
}

const SPACE: u8 = 0x39;
const ENTER: u8 = 0x1C;

/// What the controller presses while playing, as DOS scancodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Controls {
    pub up: u8,
    pub down: u8,
    pub left: u8,
    pub right: u8,
    pub jump: u8,
    pub pogo: Option<u8>,
    /// Pressed in this order.
    pub fire: Vec<u8>,
    pub status: u8,
}

/// The controls of a game playing with `keys`. `firing`: Keen 1-3's fire
/// key, where one-button firing is on or built in.
pub fn controls(engine: Engine, keys: &ActionKeys, firing: Option<u8>) -> Controls {
    let (fire, status) = match engine {
        // Without one-button firing, Jump and Pogo together shoot, and Space
        // opens the status window.
        Engine::Vorticons => match firing {
            Some(key) => (vec![key], ENTER),
            None => ([keys.jump].into_iter().chain(keys.pogo).collect(), SPACE),
        },
        // Keen 4's special demo has no fire key and fires with Space.
        Engine::Galaxy => (vec![keys.fire.unwrap_or(SPACE)], ENTER),
        Engine::Dreams => (vec![keys.fire.unwrap_or(0x38)], SPACE),
    };
    Controls {
        up: keys.up,
        down: keys.down,
        left: keys.left,
        right: keys.right,
        jump: keys.jump,
        pogo: keys.pogo,
        fire,
        status,
    }
}

/// A key's event, or that of its default where DOSBox has no key for it.
fn key(scancode: u8, default: u8) -> String {
    event(scancode)
        .or_else(|| event(default))
        .expect("defaults have keys")
}

/// The lines for playing, in file order: the order an input bound to several
/// keys presses them.
fn play(engine: Engine, c: &Controls) -> Vec<(String, Vec<&'static str>)> {
    let defaults = ActionKeys::defaults(engine);
    let mut lines = Vec::new();
    if engine == Engine::Dreams {
        // A: 5, then the jump key.
        lines.push(("key_5".to_string(), vec!["stick_0 button 0"]));
    }
    // A: jump.
    lines.push((key(c.jump, defaults.jump), vec!["stick_0 button 0"]));
    // Menu: main menu.
    lines.push(("key_esc".to_string(), vec!["stick_0 button 7"]));
    // Left stick and D-pad: move.
    for (code, default, binds) in [
        (c.up, defaults.up, ["stick_0 hat 0 1", "stick_0 axis 1 0"]),
        (
            c.right,
            defaults.right,
            ["stick_0 hat 0 2", "stick_0 axis 0 1"],
        ),
        (
            c.down,
            defaults.down,
            ["stick_0 hat 0 4", "stick_0 axis 1 1"],
        ),
        (
            c.left,
            defaults.left,
            ["stick_0 hat 0 8", "stick_0 axis 0 0"],
        ),
    ] {
        lines.push((key(code, default), binds.to_vec()));
    }
    // View held is DOSBox's third modifier and switches to the menu keys. A
    // binding with the modifier wins over the same input without it, so the
    // game doesn't see both.
    lines.push(("mod_3".to_string(), vec!["stick_0 button 6"]));
    // B, LB, RB: pogo.
    if let Some(pogo) = c.pogo {
        let default = defaults.pogo.unwrap_or(0x38);
        lines.push((
            key(pogo, default),
            vec!["stick_0 button 1", "stick_0 button 4", "stick_0 button 5"],
        ));
    }
    // X, LT, RT: fire, throw in Keen Dreams.
    for &fire in &c.fire {
        lines.push((
            key(fire, SPACE),
            vec!["stick_0 button 2", "stick_0 axis 4 1", "stick_0 axis 5 1"],
        ));
    }
    // Y: status window.
    lines.push((key(c.status, ENTER), vec!["stick_0 button 3"]));
    lines
}

/// The menu keys of an engine, with View held.
fn menu(engine: Engine) -> Pad {
    match engine {
        Engine::Vorticons => MENU_VORTICONS,
        Engine::Galaxy | Engine::Dreams => MENU_GALAXY,
    }
}

/// DOSBox's own hotkeys on the controller, with View held, in every engine.
/// View+RT: window or full screen (Alt+Enter).
const WINDOW: Pad = &[("hand_fullscr", &["stick_0 axis 5 1 mod3"])];

/// The mapper file for a layout, engine and the game's controls, given the
/// host key (SDL scancode) of each letter that has one. Each letter is swapped onto its
/// host key, so the key it pushes out takes the letter's old place and every
/// host key still sends exactly one emulated key.
pub fn mapper(letters: &[(char, u16)], engine: Engine, controls: &Controls) -> String {
    let mut keys: Vec<(&str, u16)> = KEYS.to_vec();
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
        }
    }
    let mut out = String::new();
    for (name, host) in keys {
        out.push_str(&format!("key_{name} \"key {host}\"\n"));
    }
    // The controller only presses keys: as DOSBox's emulated joystick too,
    // its buttons would also select menu entries in Keen Dreams.
    for line in REST.lines().filter(|l| !l.contains("\"stick_0 ")) {
        out.push_str(line);
        out.push('\n');
    }
    // A line for an event that already has one adds to its bindings.
    let menu = menu(engine)
        .iter()
        .chain(WINDOW)
        .map(|(event, binds)| (event.to_string(), binds.to_vec()));
    for (event, binds) in play(engine, controls).into_iter().chain(menu) {
        out.push_str(&event);
        for bind in binds {
            out.push_str(&format!(" \"{bind}\""));
        }
        out.push('\n');
    }
    out
}

/// The mapper file's name, next to the launch layer that names it.
pub const FILE: &str = "keyboard.map";

/// Writes a profile's mapper file for the host's keyboard layout and the
/// game's `controls`.
pub fn ensure(
    paths: &AppPaths,
    profile_id: &str,
    engine: Engine,
    controls: &Controls,
) -> Result<()> {
    // Earlier versions wrote one file for all profiles, then one per engine.
    for old in [
        "keyboard.map",
        "keyboard-vorticons.map",
        "keyboard-galaxy.map",
        "keyboard-dreams.map",
    ] {
        let _ = std::fs::remove_file(paths.configs.join(old));
    }
    let letters = host_letters().unwrap_or_default();
    write_if_changed(
        &paths.profile(profile_id).mapper,
        mapper(&letters, engine, controls),
    )
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

    /// The controls with the game's default keys, Keen 1-3 with one-button
    /// firing.
    fn default(engine: Engine) -> Controls {
        let firing = (engine == Engine::Vorticons).then_some(SPACE);
        controls(engine, &ActionKeys::defaults(engine), firing)
    }

    /// An event's first binding, its host key.
    fn binding<'a>(map: &'a str, event: &str) -> &'a str {
        let rest = map
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{event} ")))
            .unwrap();
        let end = rest[1..].find('"').unwrap() + 2;
        &rest[..end]
    }

    #[test]
    fn us_layout_keeps_letters() {
        let us: Vec<(char, u16)> = ('a'..='z').zip(4..).collect();
        let map = mapper(&us, Engine::Galaxy, &default(Engine::Galaxy));
        assert_eq!(binding(&map, "key_y"), "\"key 28\"");
        assert_eq!(binding(&map, "key_z"), "\"key 29\"");
    }

    #[test]
    fn german_layout_swaps_y_and_z() {
        let map = mapper(
            &[('Y', 29), ('Z', 28), ('A', 4)],
            Engine::Galaxy,
            &default(Engine::Galaxy),
        );
        assert_eq!(binding(&map, "key_y"), "\"key 29\"");
        assert_eq!(binding(&map, "key_z"), "\"key 28\"");
        assert_eq!(binding(&map, "key_a"), "\"key 4\"");
        assert_eq!(binding(&map, "hand_shutdown"), "\"key 66 mod1\"");
    }

    #[test]
    fn french_layout_keeps_every_host_key_bound_once() {
        // AZERTY: A and Q, Z and W swap; M sits where US has the semicolon.
        let map = mapper(
            &[('A', 20), ('Q', 4), ('Z', 26), ('W', 29), ('M', 51)],
            Engine::Vorticons,
            &default(Engine::Vorticons),
        );
        assert_eq!(binding(&map, "key_m"), "\"key 51\"");
        assert_eq!(binding(&map, "key_semicolon"), "\"key 16\"");
        let hosts: Vec<&str> = map
            .lines()
            .filter(|l| l.starts_with("key_"))
            .map(|l| l.split('"').nth(1).unwrap())
            .filter(|b| b.starts_with("key "))
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

    /// The lines for playing: those without View.
    fn playing(engine: Engine, controls: &Controls) -> Vec<String> {
        mapper(&[], engine, controls)
            .lines()
            .filter(|l| l.contains("\"stick_0 ") && !l.contains("mod3"))
            .map(str::to_string)
            .collect()
    }

    const MOVE: [&str; 6] = [
        "key_esc \"stick_0 button 7\"",
        "key_up \"stick_0 hat 0 1\" \"stick_0 axis 1 0\"",
        "key_right \"stick_0 hat 0 2\" \"stick_0 axis 0 1\"",
        "key_down \"stick_0 hat 0 4\" \"stick_0 axis 1 1\"",
        "key_left \"stick_0 hat 0 8\" \"stick_0 axis 0 0\"",
        "mod_3 \"stick_0 button 6\"",
    ];
    const POGO: &str = "key_lalt \"stick_0 button 1\" \"stick_0 button 4\" \"stick_0 button 5\"";
    const FIRE: &str = "\"stick_0 button 2\" \"stick_0 axis 4 1\" \"stick_0 axis 5 1\"";

    #[test]
    fn default_keys_give_the_shipped_bindings() {
        let keen = |status: &str| {
            let mut lines = vec!["key_lctrl \"stick_0 button 0\"".to_string()];
            lines.extend(MOVE.map(String::from));
            lines.push(POGO.to_string());
            lines.push(format!("key_space {FIRE}"));
            lines.push(format!("{status} \"stick_0 button 3\""));
            lines
        };
        assert_eq!(
            playing(Engine::Galaxy, &default(Engine::Galaxy)),
            keen("key_enter")
        );
        assert_eq!(
            playing(Engine::Vorticons, &default(Engine::Vorticons)),
            keen("key_enter")
        );
        let mut dreams = vec![
            "key_5 \"stick_0 button 0\"".to_string(),
            "key_lctrl \"stick_0 button 0\"".to_string(),
        ];
        dreams.extend(MOVE.map(String::from));
        dreams.push(format!("key_lalt {FIRE}"));
        dreams.push("key_space \"stick_0 button 3\"".to_string());
        assert_eq!(playing(Engine::Dreams, &default(Engine::Dreams)), dreams);
    }

    #[test]
    fn buttons_follow_the_games_keys() {
        let keys = ActionKeys {
            jump: 0x1E,
            fire: Some(0x2D),
            up: 0x11,
            ..ActionKeys::defaults(Engine::Galaxy)
        };
        let lines = playing(Engine::Galaxy, &controls(Engine::Galaxy, &keys, None));
        assert_eq!(lines[0], "key_a \"stick_0 button 0\"");
        assert_eq!(lines[2], "key_w \"stick_0 hat 0 1\" \"stick_0 axis 1 0\"");
        assert!(lines.contains(&format!("key_x {FIRE}")));
        assert!(!lines.iter().any(|l| l.starts_with("key_lctrl ")));
    }

    #[test]
    fn keen_1_3_without_firing_shoots_with_jump_and_pogo() {
        let keys = ActionKeys {
            pogo: Some(0x1F),
            ..ActionKeys::defaults(Engine::Vorticons)
        };
        let c = controls(Engine::Vorticons, &keys, None);
        assert_eq!(c.fire, [0x1D, 0x1F]);
        let lines = playing(Engine::Vorticons, &c);
        let fire: Vec<&String> = lines.iter().filter(|l| l.ends_with(FIRE)).collect();
        assert_eq!(
            fire,
            [&format!("key_lctrl {FIRE}"), &format!("key_s {FIRE}")]
        );
        assert_eq!(lines.last().unwrap(), "key_space \"stick_0 button 3\"");
    }

    #[test]
    fn keen_1_3_fires_with_the_one_button_key() {
        let keys = ActionKeys::defaults(Engine::Vorticons);
        let c = controls(Engine::Vorticons, &keys, Some(0x2D));
        assert_eq!((c.fire, c.status), (vec![0x2D], ENTER));
    }

    #[test]
    fn view_and_rt_toggle_full_screen_in_every_engine() {
        for engine in [Engine::Vorticons, Engine::Galaxy, Engine::Dreams] {
            let map = mapper(&[], engine, &default(engine));
            let lines: Vec<&str> = map
                .lines()
                .filter(|l| l.starts_with("hand_fullscr "))
                .collect();
            // DOSBox's Alt+Enter stays, the controller adds to it.
            assert_eq!(
                lines,
                [
                    "hand_fullscr \"key 40 mod2\"",
                    "hand_fullscr \"stick_0 axis 5 1 mod3\""
                ]
            );
            // Nothing else uses RT with View held.
            assert_eq!(
                map.matches("\"stick_0 axis 5 1 mod3\"").count(),
                1,
                "{engine:?}"
            );
        }
    }

    #[test]
    fn keys_dosbox_has_no_event_for_fall_back() {
        let keys = ActionKeys {
            jump: 0x00,
            fire: None,
            ..ActionKeys::defaults(Engine::Galaxy)
        };
        let lines = playing(Engine::Galaxy, &controls(Engine::Galaxy, &keys, None));
        assert_eq!(lines[0], "key_lctrl \"stick_0 button 0\"");
        assert!(
            lines.contains(&format!("key_space {FIRE}")),
            "the special demo"
        );
    }

    #[test]
    fn every_scancode_names_an_emulated_key() {
        for (code, name) in SCANCODES {
            assert!(KEYS.iter().any(|(n, _)| n == name), "{code:#x} {name}");
        }
    }
}
