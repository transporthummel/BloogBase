//! What a game left on the DOS screen when it ended, e.g. CKPatch's
//! "Unrecognised version of KEEN4C.EXE". DOSBox closes right after the
//! game, so nobody gets to read it; and patchers end with errorlevel 0 even
//! then. After the game, a tiny program saves the text screen to a file in
//! a folder mounted as drive X, and BloogBase shows what the game printed.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::dosconf::write_if_changed;
use crate::error::Result;

/// The drive the folder with the program and the saved screen is mounted as.
pub const DRIVE: char = 'X';
const PROGRAM: &str = "SCREEN.COM";
const SCREEN: &str = "SCREEN.BIN";

/// Saves the 80x25 text screen (B800:0000, 4000 bytes of character and
/// attribute) to the file named on its command line, unless the screen is
/// in a graphics mode (BIOS mode not 2 or 3).
const SCREEN_COM: [u8; 72] = [
    0xB8, 0x40, 0x00, //       mov ax,0040h
    0x8E, 0xC0, //             mov es,ax
    0x26, 0xA0, 0x49, 0x00, // mov al,[es:0049h]   ; BIOS video mode
    0x3C, 0x02, //             cmp al,2
    0x72, 0x36, //             jb done
    0x3C, 0x03, //             cmp al,3
    0x77, 0x32, //             ja done
    0xBE, 0x81, 0x00, //       mov si,0081h        ; command tail
    0xAC, //                   skip: lodsb
    0x3C, 0x20, //             cmp al,' '
    0x74, 0xFB, //             je skip
    0x4E, //                   dec si
    0x89, 0xF2, //             mov dx,si           ; file name
    0xAC, //                   end: lodsb
    0x3C, 0x0D, //             cmp al,0Dh
    0x75, 0xFB, //             jne end
    0xC6, 0x44, 0xFF, 0x00, // mov byte [si-1],0
    0xB4, 0x3C, //             mov ah,3Ch          ; create
    0x31, 0xC9, //             xor cx,cx
    0xCD, 0x21, //             int 21h
    0x72, 0x16, //             jc done
    0x89, 0xC3, //             mov bx,ax
    0x1E, //                   push ds
    0xB8, 0x00, 0xB8, //       mov ax,0B800h
    0x8E, 0xD8, //             mov ds,ax
    0x31, 0xD2, //             xor dx,dx
    0xB9, 0xA0, 0x0F, //       mov cx,4000
    0xB4, 0x40, //             mov ah,40h          ; write
    0xCD, 0x21, //             int 21h
    0x1F, //                   pop ds
    0xB4, 0x3E, //             mov ah,3Eh          ; close
    0xCD, 0x21, //             int 21h
    0xB8, 0x00, 0x4C, //       done: mov ax,4C00h
    0xCD, 0x21, //             int 21h
];

/// Puts the program in `dir` and removes the screen of the last launch.
pub fn prepare(dir: &Path) -> Result<()> {
    write_if_changed(&dir.join(PROGRAM), SCREEN_COM)?;
    match fs::remove_file(screen_file(dir)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// The DOS command that saves the screen.
pub fn command() -> String {
    format!("{DRIVE}:\\{PROGRAM} {DRIVE}:\\{SCREEN}")
}

fn screen_file(dir: &Path) -> PathBuf {
    dir.join(SCREEN)
}

/// What the game printed before it ended, if it left plain DOS text.
pub fn message(dir: &Path) -> Option<String> {
    message_from(&fs::read(screen_file(dir)).ok()?)
}

/// The lines between the prompt that started the game and the one that
/// saved the screen. Only plain light grey text counts: games end with
/// colorful screens (like Keen's ordering info) on purpose, while DOS,
/// patchers and the games' own error messages print plain text.
fn message_from(screen: &[u8]) -> Option<String> {
    let rows: Vec<&[[u8; 2]]> = screen
        .as_chunks::<160>()
        .0
        .iter()
        .map(|r| r.as_chunks::<2>().0)
        .collect();
    let text = |row: &[[u8; 2]]| -> String {
        row.iter()
            .map(|[c, _]| cp437(*c))
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    let is_prompt = |line: &str| {
        let b = line.as_bytes();
        b.len() > 3
            && b[0].is_ascii_alphabetic()
            && b[1] == b':'
            && b[2] == b'\\'
            && line.contains('>')
    };
    let saved = rows
        .iter()
        .rposition(|r| is_prompt(&text(r)) && text(r).ends_with(&command()))?;
    let start = rows[..saved]
        .iter()
        .rposition(|r| is_prompt(&text(r)))
        .map_or(0, |p| p + 1);
    let output = &rows[start..saved];
    let plain = output
        .iter()
        .flat_map(|r| r.iter())
        .all(|&[c, attribute]| c == b' ' || c == 0 || attribute == 0x07);
    let lines: Vec<String> = output.iter().map(|r| text(r)).collect();
    let message = lines.join("\n").trim_matches('\n').to_string();
    (plain && !message.trim().is_empty()).then_some(message)
}

/// Code page 437, as far as text goes; box drawing and the like become `?`.
fn cp437(byte: u8) -> char {
    const HIGH: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿";
    match byte {
        0 => ' ',
        0x20..=0x7E => char::from(byte),
        0x80..=0xA7 => HIGH.chars().nth(usize::from(byte - 0x80)).unwrap_or('?'),
        0xE1 => 'ß',
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A text screen with `lines` from the top, in `attribute`.
    fn screen(lines: &[&str], attribute: u8) -> Vec<u8> {
        let mut s: Vec<u8> = [b' ', 0x07].repeat(80 * 25);
        for (row, line) in lines.iter().enumerate() {
            for (col, b) in line.bytes().enumerate() {
                s[(row * 80 + col) * 2] = b;
                s[(row * 80 + col) * 2 + 1] = attribute;
            }
        }
        s
    }

    #[test]
    fn reads_what_the_game_printed() {
        let saved = format!("C:\\>{}", command());
        let s = screen(
            &[
                "Z:\\>mount C \"cache\\composed\\pak-een\"",
                "C:\\>CK4PATCH.EXE PAT.PAT -NOWAIT",
                "Error: Unrecognised version of KEEN4C.EXE",
                "",
                &saved,
            ],
            0x07,
        );
        assert_eq!(
            message_from(&s).as_deref(),
            Some("Error: Unrecognised version of KEEN4C.EXE")
        );

        // Cleared by the game: everything above.
        let s = screen(&["Out of memory!", "", &saved], 0x07);
        assert_eq!(message_from(&s).as_deref(), Some("Out of memory!"));
    }

    #[test]
    fn ignores_colorful_end_screens_and_silence() {
        let saved = format!("C:\\>{}", command());
        let mut s = screen(&["Thanks for playing!", "", &saved], 0x07);
        s[1] = 0x1F;
        assert_eq!(message_from(&s), None);
        assert_eq!(
            message_from(&screen(&["C:\\>KEEN4E.EXE", &saved], 0x07)),
            None
        );
        // Not saved by us, or no screen at all.
        assert_eq!(message_from(&screen(&["Error"], 0x07)), None);
        assert_eq!(message_from(&[]), None);
    }

    #[test]
    fn prepare_writes_the_program_and_forgets_the_last_screen() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(screen_file(tmp.path()), b"old").unwrap();
        prepare(tmp.path()).unwrap();
        assert_eq!(fs::read(tmp.path().join(PROGRAM)).unwrap(), SCREEN_COM);
        assert!(!screen_file(tmp.path()).exists());
        prepare(tmp.path()).unwrap();
    }
}
