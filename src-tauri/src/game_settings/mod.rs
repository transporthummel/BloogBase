//! Settings for all games and mods: sound, music, the sound device, the score
//! box and the keys. Each can stay unset, which leaves it to the game.

mod format;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::db::Db;
use crate::error::{Error, Result};
use crate::settings;

const SETTING_KEY: &str = "games.global_settings";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GameSettings {
    pub sound: Option<bool>,
    pub music: Option<bool>,
    pub device: Option<SoundDevice>,
    pub score_box: Option<bool>,
    pub keys: GameKeys,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SoundDevice {
    PcSpeaker,
    SoundBlaster,
}

/// DOS scancodes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct GameKeys {
    pub up: Option<u8>,
    pub down: Option<u8>,
    pub left: Option<u8>,
    pub right: Option<u8>,
    pub jump: Option<u8>,
    pub pogo: Option<u8>,
    pub fire: Option<u8>,
}

impl GameKeys {
    fn all(&self) -> [Option<u8>; 7] {
        [
            self.up, self.down, self.left, self.right, self.jump, self.pogo, self.fire,
        ]
    }
}

/// Keys the games use themselves: Esc, Enter and the F-keys.
fn is_reserved(key: u8) -> bool {
    matches!(key, 0x01 | 0x1C | 0x3B..=0x44 | 0x57 | 0x58)
}

pub fn get(db: &Db) -> Result<GameSettings> {
    let Some(value) = settings::get(db, SETTING_KEY)? else {
        return Ok(GameSettings::default());
    };
    Ok(serde_json::from_str(&value).unwrap_or_else(|e| {
        tracing::warn!("ignoring unreadable global game settings: {e}");
        GameSettings::default()
    }))
}

pub fn set(db: &Db, value: &GameSettings) -> Result<()> {
    let keys: Vec<u8> = value.keys.all().into_iter().flatten().collect();
    if let Some(key) = keys.iter().find(|k| is_reserved(**k)) {
        return Err(Error::InvalidSetting(format!("key {key:#04x} is reserved")));
    }
    for (i, key) in keys.iter().enumerate() {
        if keys[..i].contains(key) {
            return Err(Error::InvalidSetting(format!(
                "key {key:#04x} is set twice"
            )));
        }
    }
    let json = serde_json::to_string(value).map_err(|e| Error::InvalidSetting(e.to_string()))?;
    settings::set(db, SETTING_KEY, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn jump(key: u8) -> GameSettings {
        GameSettings {
            keys: GameKeys {
                jump: Some(key),
                ..GameKeys::default()
            },
            ..GameSettings::default()
        }
    }

    #[test]
    fn unset_by_default() {
        assert_eq!(get(&db()).unwrap(), GameSettings::default());
    }

    #[test]
    fn round_trips() {
        let db = db();
        let settings = GameSettings {
            sound: Some(false),
            music: Some(true),
            device: Some(SoundDevice::PcSpeaker),
            score_box: Some(false),
            ..jump(0x11)
        };
        set(&db, &settings).unwrap();
        assert_eq!(get(&db).unwrap(), settings);
    }

    #[test]
    fn corrupt_value_counts_as_unset() {
        let db = db();
        settings::set(&db, SETTING_KEY, "{").unwrap();
        assert_eq!(get(&db).unwrap(), GameSettings::default());
    }

    #[test]
    fn refuses_reserved_keys() {
        let db = db();
        for key in [0x01, 0x1C, 0x3B, 0x44, 0x57, 0x58] {
            assert!(
                matches!(set(&db, &jump(key)), Err(Error::InvalidSetting(_))),
                "{key:#x}"
            );
        }
        assert_eq!(get(&db).unwrap(), GameSettings::default());
    }

    #[test]
    fn refuses_two_settings_on_one_key() {
        let mut settings = jump(0x39);
        settings.keys.fire = Some(0x39);
        assert!(matches!(
            set(&db(), &settings),
            Err(Error::InvalidSetting(_))
        ));
    }
}
