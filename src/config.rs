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
    /// Free the model's ~1 GB of RAM after this many idle minutes (0 = never).
    /// Reloading takes under a second.
    pub unload_after_idle_mins: u64,
    /// Recordings longer than this are cut off (protects memory).
    pub max_recording_secs: u64,
    /// Microphone name, e.g. "Built-in Microphone". Unset or not connected
    /// = system default. Bluetooth headset mics take ~1 s to start, which
    /// clips the first words.
    pub input_device: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "OptRight".into(),
            mode: Mode::Hold,
            model: ModelId::DEFAULT,
            restore_clipboard: true,
            unload_after_idle_mins: 0,
            max_recording_secs: 300,
            input_device: None,
        }
    }
}

pub fn path() -> PathBuf {
    dirs::config_dir().expect("no config dir").join("sayit").join("config.toml")
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
