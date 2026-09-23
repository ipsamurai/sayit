//! User settings, stored as TOML in the platform config directory.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::stt::ModelId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Hold the hotkey while speaking, release to transcribe.
    Hold,
    /// Press once to start, press again to stop.
    Toggle,
}

/// What closing the Settings window does. The menu-bar icon stays in every
/// case except `Quit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnClose {
    /// Keep running in the menu bar only.
    #[default]
    MenuBar,
    /// Keep running, with a Dock icon as well.
    Dock,
    /// Quit sayit.
    Quit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// e.g. "OptRight", "Fn", "Ctrl+Alt+Space".
    pub hotkey: String,
    pub mode: Mode,
    /// Speech model. Currently only "parakeet-v2"; see scripts/fetch-models.sh.
    pub model: ModelId,
    /// Put the previous clipboard contents back after pasting.
    pub restore_clipboard: bool,
    /// Drop hesitation sounds ("um", "uh", "erm") from transcripts.
    pub remove_fillers: bool,
    /// End each take with a line break (otherwise a space).
    pub newline_after_take: bool,
    /// Free the model's ~1 GB of RAM after this many idle minutes (0 = never).
    /// Reloading takes under a second.
    pub unload_after_idle_mins: u64,
    /// Recordings longer than this are cut off (protects memory).
    pub max_recording_secs: u64,
    /// Microphone name, e.g. "Built-in Microphone". Unset or not connected
    /// = system default. Bluetooth headset mics take ~1 s to start, which
    /// clips the first words.
    pub input_device: Option<String>,
    /// Set once the first-launch setup assistant has been finished.
    pub setup_complete: bool,
    /// "menu-bar", "dock" or "quit".
    pub on_close: OnClose,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "OptRight".into(),
            mode: Mode::Hold,
            model: ModelId::DEFAULT,
            restore_clipboard: true,
            remove_fillers: true,
            newline_after_take: true,
            unload_after_idle_mins: 0,
            max_recording_secs: 300,
            input_device: None,
            setup_complete: false,
            on_close: OnClose::MenuBar,
        }
    }
}

pub fn path() -> PathBuf {
    dirs::config_dir()
        .expect("no config dir")
        .join("sayit")
        .join("config.toml")
}

impl Config {
    /// Loads the config, writing a default one on first run.
    pub fn load() -> Result<Self> {
        let p = path();
        match std::fs::read_to_string(&p) {
            Ok(s) => toml::from_str(&s).with_context(|| format!("invalid config {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let c = Self::default();
                if let Some(dir) = p.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&p, toml::to_string_pretty(&c)?)?;
                Ok(c)
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Writes the config atomically (temp file + rename), so a crash can't
    /// leave a half-written file.
    pub fn save(&self) -> Result<()> {
        let p = path();
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &p).with_context(|| format!("saving {}", p.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_close_parses_and_defaults_to_menu_bar() {
        let cfg: Config = toml::from_str("on_close = \"dock\"").unwrap();
        assert_eq!(cfg.on_close, OnClose::Dock);
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.on_close, OnClose::MenuBar);
    }
}
