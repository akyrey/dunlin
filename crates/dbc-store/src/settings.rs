//! Application settings (task 8.8): one JSON document, every field defaulted, so settings
//! written by an older version still load.

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::{Store, StoreError};

const KEY: &str = "app";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

/// When staged edits show their SQL before Apply. A profile marked "always preview" always
/// does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
pub enum PreviewPolicy {
    Always,
    /// Until the user skips it for the rest of the session.
    #[default]
    OncePerSession,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: Theme,
    /// A CSS font family list.
    pub editor_font_family: String,
    pub editor_font_size: u8,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub default_row_limit: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub default_page_size: usize,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub history_cap: usize,
    pub preview_policy: PreviewPolicy,
    /// How `NULL` is shown in grids.
    pub null_text: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            editor_font_family: "ui-monospace, SFMono-Regular, Menlo, monospace".to_owned(),
            editor_font_size: 13,
            default_row_limit: 1000,
            default_page_size: 200,
            history_cap: crate::DEFAULT_HISTORY_CAP,
            preview_policy: PreviewPolicy::OncePerSession,
            null_text: "NULL".to_owned(),
        }
    }
}

/// Bounds shared by the backend and the settings view.
pub const FONT_SIZES: std::ops::RangeInclusive<u8> = 9..=32;
pub const ROW_LIMITS: std::ops::RangeInclusive<usize> = 1..=100_000;
pub const PAGE_SIZES: std::ops::RangeInclusive<usize> = 10..=10_000;
pub const HISTORY_CAPS: std::ops::RangeInclusive<usize> = 0..=1_000_000;
const MAX_TEXT: usize = 200;

impl Settings {
    /// Refuses values out of range rather than clamping them silently.
    pub fn validate(&self) -> Result<(), StoreError> {
        let invalid = |what: &str| Err(StoreError::Invalid(what.to_owned()));
        if !FONT_SIZES.contains(&self.editor_font_size) {
            return invalid("The editor font size must be between 9 and 32");
        }
        if !ROW_LIMITS.contains(&self.default_row_limit) {
            return invalid("The default row limit must be between 1 and 100 000");
        }
        if !PAGE_SIZES.contains(&self.default_page_size) {
            return invalid("The default page size must be between 10 and 10 000");
        }
        if !HISTORY_CAPS.contains(&self.history_cap) {
            return invalid("The history size must be between 0 and 1 000 000");
        }
        if self.editor_font_family.trim().is_empty() || self.editor_font_family.len() > MAX_TEXT {
            return invalid("Name an editor font");
        }
        if self.null_text.chars().count() > 20 {
            return invalid("The NULL text can have at most 20 characters");
        }
        Ok(())
    }
}

impl Store {
    pub fn settings(&self) -> Result<Settings, StoreError> {
        let stored: Option<String> = self
            .connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |row| {
                row.get(0)
            })
            .optional()?;
        match stored {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<(), StoreError> {
        settings.validate()?;
        self.connection.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            rusqlite::params![KEY, serde_json::to_string(settings)?],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn defaults_until_saved_then_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("store.db")).unwrap();
        assert_eq!(store.settings().unwrap(), Settings::default());

        let changed = Settings {
            theme: Theme::Light,
            editor_font_size: 15,
            null_text: "∅".into(),
            ..Settings::default()
        };
        store.save_settings(&changed).unwrap();

        assert_eq!(store.settings().unwrap(), changed);
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let partial: Settings = serde_json::from_str(r#"{"theme": "dark"}"#).unwrap();

        assert_eq!(partial.theme, Theme::Dark);
        assert_eq!(partial.default_page_size, 200);
    }

    #[test]
    fn refuses_values_out_of_range() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("store.db")).unwrap();
        let cases = [
            Settings {
                editor_font_size: 4,
                ..Settings::default()
            },
            Settings {
                default_row_limit: 0,
                ..Settings::default()
            },
            Settings {
                default_page_size: 5,
                ..Settings::default()
            },
            Settings {
                history_cap: 2_000_000,
                ..Settings::default()
            },
            Settings {
                editor_font_family: " ".into(),
                ..Settings::default()
            },
            Settings {
                null_text: "x".repeat(21),
                ..Settings::default()
            },
        ];

        for case in cases {
            assert!(
                matches!(store.save_settings(&case), Err(StoreError::Invalid(_))),
                "{case:?}"
            );
        }
        assert_eq!(store.settings().unwrap(), Settings::default());
    }
}
