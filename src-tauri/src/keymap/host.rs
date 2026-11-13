//! Where the active keyboard layout puts each letter, as the SDL scancode of
//! its key (the key's place on a US keyboard). Windows, macOS's text input
//! sources and GTK all answer on the main thread, `keymap::host_letters`
//! asks there.

/// The host key of each letter, empty where the layout can't be told
/// (DOSBox's US positions then).
pub fn letters() -> Vec<(char, u16)> {
    imp::letters().unwrap_or_default()
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MAPVK_VK_TO_VSC, MapVirtualKeyExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    pub fn letters() -> Option<Vec<(char, u16)>> {
        // SAFETY: plain queries without pointers we own; a null window gives
        // thread 0, whose layout is the default one.
        let layout = unsafe {
            let thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
            GetKeyboardLayout(thread)
        };
        let letters = ('A'..='Z')
            .filter_map(|c| {
                // Letter virtual keys are their uppercase ASCII codes.
                let scan = unsafe { MapVirtualKeyExW(c as u32, MAPVK_VK_TO_VSC, layout) };
                super::sdl_scancode(scan).map(|s| (c, s))
            })
            .collect();
        Some(letters)
    }
}

/// GDK's keymap, from the X server or the Wayland compositor alike.
#[cfg(target_os = "linux")]
mod imp {
    pub fn letters() -> Option<Vec<(char, u16)>> {
        let display = gdk::Display::default()?;
        let keymap = gdk::Keymap::for_display(&display)?;
        let letters = ('a'..='z')
            .filter_map(|c| {
                // A lowercase letter's keyval is its ASCII code. With several
                // layouts GDK lists the key of each; the first one is taken,
                // as GDK 3 doesn't tell which one is active.
                let key = keymap
                    .entries_for_keyval(c as u32)
                    .into_iter()
                    .min_by_key(|k| (k.group(), k.level()))?;
                // Hardware keycodes are evdev codes plus 8, and in the typing
                // area evdev codes are PC (set 1) scancodes.
                let scan = key.keycode().checked_sub(8)?;
                super::sdl_scancode(scan).map(|s| (c, s))
            })
            .collect();
        Some(letters)
    }
}

/// The current keyboard layout of the text input sources (Carbon), which
/// stays a keyboard layout while an input method is active.
#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::c_void;

    type CFTypeRef = *const c_void;

    #[link(name = "Carbon", kind = "framework")]
    unsafe extern "C" {
        static kTISPropertyUnicodeKeyLayoutData: CFTypeRef;
        fn TISCopyCurrentKeyboardLayoutInputSource() -> CFTypeRef;
        fn TISGetInputSourceProperty(source: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
        fn LMGetKbdType() -> u8;
        fn UCKeyTranslate(
            layout: *const c_void,
            virtual_key: u16,
            action: u16,
            modifiers: u32,
            keyboard_type: u32,
            options: u32,
            dead_key_state: *mut u32,
            max_length: usize,
            length: *mut usize,
            chars: *mut u16,
        ) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
        fn CFRelease(cf: CFTypeRef);
    }

    const KEY_ACTION_DOWN: u16 = 0;
    const NO_DEAD_KEYS: u32 = 1;

    pub fn letters() -> Option<Vec<(char, u16)>> {
        // SAFETY: the source is ours until released; the layout data belongs
        // to it and is only read before that, by UCKeyTranslate with buffers
        // of the given sizes.
        unsafe {
            let source = TISCopyCurrentKeyboardLayoutInputSource();
            if source.is_null() {
                return None;
            }
            let data = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData);
            let letters = (!data.is_null()).then(|| {
                let layout = CFDataGetBytePtr(data).cast::<c_void>();
                let keyboard = u32::from(LMGetKbdType());
                let mut letters: Vec<(char, u16)> = Vec::new();
                for &(code, sdl) in super::MAC_KEYS {
                    let (mut dead, mut len, mut chars) = (0u32, 0usize, [0u16; 4]);
                    let status = UCKeyTranslate(
                        layout,
                        code,
                        KEY_ACTION_DOWN,
                        0,
                        keyboard,
                        NO_DEAD_KEYS,
                        &mut dead,
                        chars.len(),
                        &mut len,
                        chars.as_mut_ptr(),
                    );
                    let c = (status == 0 && len == 1)
                        .then(|| char::from_u32(u32::from(chars[0])))
                        .flatten()
                        .filter(char::is_ascii_lowercase);
                    if let Some(c) = c
                        && !letters.iter().any(|&(l, _)| l == c)
                    {
                        letters.push((c, sdl));
                    }
                }
                letters
            });
            CFRelease(source);
            letters
        }
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
mod imp {
    pub fn letters() -> Option<Vec<(char, u16)>> {
        None
    }
}

/// The SDL scancode of a PC (set 1) scancode in the typing area.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
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

/// macOS's virtual key codes in the typing area (`kVK_ANSI_*` in
/// `Events.h`) and their SDL scancodes, as SDL2's `darwin_scancode_table`
/// has them. Left out: the keys left of 1 and Z, which SDL2 swaps on ISO
/// keyboards; no letter sits there.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[rustfmt::skip]
const MAC_KEYS: &[(u16, u16)] = &[
    (0x12, 30), (0x13, 31), (0x14, 32), (0x15, 33), (0x17, 34), (0x16, 35),
    (0x1A, 36), (0x1C, 37), (0x19, 38), (0x1D, 39), (0x1B, 45), (0x18, 46),
    (0x0C, 20), (0x0D, 26), (0x0E, 8), (0x0F, 21), (0x11, 23), (0x10, 28),
    (0x20, 24), (0x22, 12), (0x1F, 18), (0x23, 19), (0x21, 47), (0x1E, 48),
    (0x00, 4), (0x01, 22), (0x02, 7), (0x03, 9), (0x05, 10), (0x04, 11),
    (0x26, 13), (0x28, 14), (0x25, 15), (0x29, 51), (0x27, 52), (0x2A, 49),
    (0x06, 29), (0x07, 27), (0x08, 6), (0x09, 25), (0x0B, 5), (0x2D, 17),
    (0x2E, 16), (0x2B, 54), (0x2F, 55), (0x2C, 56),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc_scancodes_map_to_sdl() {
        assert_eq!(sdl_scancode(0x15), Some(28)); // Y
        assert_eq!(sdl_scancode(0x2C), Some(29)); // Z
        assert_eq!(sdl_scancode(0x27), Some(51)); // semicolon
        assert_eq!(sdl_scancode(0x29), Some(53)); // grave
        assert_eq!(sdl_scancode(0x01), None);
    }

    #[test]
    fn mac_keys_are_the_typing_area_once_each() {
        let mut codes: Vec<u16> = MAC_KEYS.iter().map(|k| k.0).collect();
        let mut sdl: Vec<u16> = MAC_KEYS.iter().map(|k| k.1).collect();
        codes.sort();
        codes.dedup();
        sdl.sort();
        sdl.dedup();
        assert_eq!(codes.len(), MAC_KEYS.len());
        assert_eq!(sdl.len(), MAC_KEYS.len());
        // Every key of the typing area but grave (53) and the ISO key (100).
        let mut pc: Vec<u16> = (0x02..0x0E)
            .chain(0x10..0x1C)
            .chain(0x1E..0x29)
            .chain(0x2B..0x36)
            .filter_map(sdl_scancode)
            .collect();
        pc.sort();
        assert_eq!(sdl, pc);
        // kVK_ANSI_Y and kVK_ANSI_Z.
        assert!(MAC_KEYS.contains(&(0x10, 28)) && MAC_KEYS.contains(&(0x06, 29)));
    }
}
