//! Speech-to-text engines. The model is loaded once and kept resident so each
//! dictation pays only inference cost.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams};

use crate::paths;

/// Models sayit can run. Keys match `scripts/fetch-models.sh` and the
/// `model` setting in config.toml. Adding a model means a variant here, a
/// loader arm in `Engine`, and a fetch function in the script; the Model menu
/// appears once there is more than one. See PLAN.md for the models that were
/// benchmarked and dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelId {
    ParakeetV2,
}

impl ModelId {
    pub const DEFAULT: ModelId = ModelId::ParakeetV2;
    pub const ALL: [ModelId; 1] = [ModelId::ParakeetV2];

    /// Name used by fetch-models.sh and config.toml.
    pub fn key(self) -> &'static str {
        match self {
            ModelId::ParakeetV2 => "parakeet-v2",
        }
    }

    /// Human-readable name for menus.
    pub fn label(self) -> &'static str {
        match self {
            ModelId::ParakeetV2 => "Parakeet v2 (English)",
        }
    }

    pub fn dir(self) -> PathBuf {
        paths::model_dir(match self {
            ModelId::ParakeetV2 => "parakeet-tdt-0.6b-v2-int8",
        })
    }

    /// True when fetch-models.sh has finished downloading this model. Checks
    /// the file written last, so a partial download doesn't count.
    pub fn is_installed(self) -> bool {
        let last = match self {
            ModelId::ParakeetV2 => "vocab.txt",
        };
        self.dir().join(last).is_file()
    }
}

impl std::str::FromStr for ModelId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        ModelId::ALL
            .into_iter()
            .find(|m| m.key() == s)
            .ok_or_else(|| {
                let keys: Vec<_> = ModelId::ALL.iter().map(|m| m.key()).collect();
                anyhow::anyhow!("unknown model {s:?} (available: {})", keys.join(", "))
            })
    }
}

/// A config naming a model this build doesn't have (e.g. one that was removed)
/// falls back to the default instead of stopping sayit from starting.
impl<'de> Deserialize<'de> for ModelId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let key = String::deserialize(d)?;
        Ok(key.parse().unwrap_or_else(|e| {
            eprintln!("config: {e:#}; using {}", ModelId::DEFAULT.key());
            ModelId::DEFAULT
        }))
    }
}

enum Model {
    Parakeet(ParakeetModel),
}

pub struct Engine {
    pub id: ModelId,
    model: Model,
}

impl Engine {
    pub fn load(id: ModelId) -> Result<Self> {
        let dir = id.dir();
        // Plain CPU is fastest: CoreML was 3-5x and XNNPACK 1.5x slower on Apple Silicon.
        transcribe_rs::accel::set_ort_accelerator(transcribe_rs::accel::OrtAccelerator::CpuOnly);
        let model = match id {
            ModelId::ParakeetV2 => {
                ParakeetModel::load(&dir, &Quantization::Int8).map(Model::Parakeet)
            }
        }
        .with_context(|| format!("loading {} from {}", id.key(), dir.display()))?;
        Ok(Self { id, model })
    }

    /// Transcribes 16 kHz mono f32 samples.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let res = match &mut self.model {
            Model::Parakeet(m) => m.transcribe_with(samples, &ParakeetParams::default()),
        }
        .context("transcription failed")?;
        Ok(res.text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_keys_match_config_names() {
        #[derive(Serialize, Deserialize)]
        struct C {
            model: ModelId,
        }
        for m in ModelId::ALL {
            assert_eq!(m.key().parse::<ModelId>().unwrap(), m);
            let toml = toml::to_string(&C { model: m }).unwrap();
            assert_eq!(toml.trim(), format!("model = \"{}\"", m.key()));
            assert_eq!(toml::from_str::<C>(&toml).unwrap().model, m);
        }
        assert!("whisper".parse::<ModelId>().is_err());
        // Removed or unknown models fall back to the default.
        assert_eq!(
            toml::from_str::<C>("model = \"no-such-model\"")
                .unwrap()
                .model,
            ModelId::DEFAULT
        );
    }
}
