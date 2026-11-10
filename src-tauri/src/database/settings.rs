//! Persistent application settings (non-sensitive), stored as one JSON document.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

const KEY: &str = "app";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearanceSettings {
    pub accent: String,
    /// Windows backdrop behind the (translucent) window: "acrylic" | "mica" | "none".
    pub window_material: String,
    /// Opacity of the dark tint over the backdrop (1.0 = solid).
    pub window_tint: f32,
    pub transparency: f32,
    pub blur: u32,
    pub animations: bool,
    pub reduced_motion: bool,
    pub sidebar_collapsed: bool,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            accent: "#A99CF6".into(),
            window_material: "acrylic".into(),
            window_tint: 0.55,
            transparency: 0.72,
            blur: 18,
            animations: true,
            reduced_motion: false,
            sidebar_collapsed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: u32,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub theme: String,
    /// Background opacity of terminals (local Windows Terminal profiles may override).
    pub opacity: f32,
    pub scrollback: u32,
    pub copy_on_select: bool,
    pub right_click_paste: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: "JetBrains Mono Variable".into(),
            font_size: 14,
            cursor_style: "bar".into(),
            cursor_blink: true,
            theme: "yoru".into(),
            opacity: 0.85,
            scrollback: 5000,
            copy_on_select: false,
            right_click_paste: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectionSettings {
    pub timeout_secs: u32,
    pub keepalive_secs: u32,
    pub default_port: u16,
    pub default_username: String,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self { timeout_secs: 15, keepalive_secs: 30, default_port: 22, default_username: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MonitoringSettings {
    pub enabled: bool,
    pub interval_secs: u32,
}

impl Default for MonitoringSettings {
    fn default() -> Self {
        Self { enabled: true, interval_secs: 1 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub appearance: AppearanceSettings,
    pub terminal: TerminalSettings,
    pub connections: ConnectionSettings,
    pub monitoring: MonitoringSettings,
}

impl AppSettings {
    pub fn validate(&self) -> AppResult<()> {
        let bad = |m: &str| Err(AppError::Validation(m.into()));
        let a = &self.appearance;
        if !is_hex_color(&a.accent) {
            return bad("Accent must be a hex color like #A99CF6");
        }
        if !(0.3..=1.0).contains(&a.transparency) {
            return bad("Transparency must be between 0.3 and 1.0");
        }
        if a.blur > 40 {
            return bad("Blur must be between 0 and 40 px");
        }
        if !["acrylic", "mica", "none"].contains(&a.window_material.as_str()) {
            return bad("Unknown window material");
        }
        if !(0.0..=1.0).contains(&a.window_tint) {
            return bad("Window tint must be between 0 and 1");
        }
        if !(0.3..=1.0).contains(&self.terminal.opacity) {
            return bad("Terminal opacity must be between 30% and 100%");
        }
        let t = &self.terminal;
        if !(8..=32).contains(&t.font_size) {
            return bad("Terminal font size must be between 8 and 32");
        }
        if !(100..=100_000).contains(&t.scrollback) {
            return bad("Scrollback must be between 100 and 100000 lines");
        }
        if !["block", "bar", "underline"].contains(&t.cursor_style.as_str()) {
            return bad("Unknown cursor style");
        }
        if t.font_family.trim().is_empty() || t.font_family.len() > 100 {
            return bad("Invalid terminal font family");
        }
        let c = &self.connections;
        if !(3..=120).contains(&c.timeout_secs) {
            return bad("SSH timeout must be between 3 and 120 seconds");
        }
        if c.keepalive_secs > 600 {
            return bad("Keepalive interval must be at most 600 seconds");
        }
        if c.default_port == 0 {
            return bad("Default port must be between 1 and 65535");
        }
        if !(1..=300).contains(&self.monitoring.interval_secs) {
            return bad("Monitoring interval must be between 1 and 300 seconds");
        }
        Ok(())
    }
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn load(conn: &Connection) -> AppResult<AppSettings> {
    let raw: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [KEY], |r| r.get(0))
        .optional()?;
    // Unknown/missing fields fall back to defaults; a corrupt document resets to defaults.
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

pub fn save(conn: &Connection, settings: &AppSettings) -> AppResult<()> {
    settings.validate()?;
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![KEY, serde_json::to_string(settings)?],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;

    #[test]
    fn defaults_then_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        db.with(|c| {
            assert_eq!(load(c)?, AppSettings::default());
            let mut s = AppSettings::default();
            s.terminal.font_size = 16;
            s.appearance.accent = "#8b9cf6".into();
            save(c, &s)?;
            assert_eq!(load(c)?, s);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn rejects_invalid_values() {
        let mut s = AppSettings::default();
        s.terminal.font_size = 99;
        assert!(s.validate().is_err());
        let mut s = AppSettings::default();
        s.appearance.accent = "red; background:url(x)".into();
        assert!(s.validate().is_err());
    }

    #[test]
    fn partial_documents_fill_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"terminal":{"fontSize":18}}"#).unwrap();
        assert_eq!(s.terminal.font_size, 18);
        assert_eq!(s.terminal.scrollback, 5000);
        assert_eq!(s.connections.default_port, 22);
    }
}
