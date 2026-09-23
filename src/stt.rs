//! Speech-to-text engines. The model is loaded once and kept resident so each
//! dictation pays only inference cost.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::onnx::moonshine::{MoonshineStreamingParams, StreamingModel};
use transcribe_rs::onnx::parakeet::{ParakeetModel, ParakeetParams};

use crate::paths;

/// Models sayit can run. Keys match `scripts/fetch-models.sh` and the
/// `model` setting in config.toml. Adding a model means a variant here, a
/// loader arm in `Engine`, and a fetch function in the script (see
/// docs/MODELS.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelId {
    ParakeetV2,
    ParakeetV3,
    MoonshineMedium,
    MoonshineSmall,
}

impl ModelId {
    pub const DEFAULT: ModelId = ModelId::ParakeetV2;
    pub const ALL: [ModelId; 4] = [
        ModelId::ParakeetV2,
        ModelId::ParakeetV3,
        ModelId::MoonshineMedium,
        ModelId::MoonshineSmall,
    ];

    /// Name used by fetch-models.sh and config.toml.
    pub fn key(self) -> &'static str {
        match self {
            ModelId::ParakeetV2 => "parakeet-v2",
            ModelId::ParakeetV3 => "parakeet-v3",
            ModelId::MoonshineMedium => "moonshine-medium",
            ModelId::MoonshineSmall => "moonshine-small",
        }
    }

    /// Human-readable name for menus.
    pub fn label(self) -> &'static str {
        match self {
            ModelId::ParakeetV2 => "Parakeet v2",
            ModelId::ParakeetV3 => "Parakeet v3",
            ModelId::MoonshineMedium => "Moonshine Medium",
            ModelId::MoonshineSmall => "Moonshine Small",
        }
    }

    /// What to compare when choosing, from our benchmarks on an 8 GB laptop
    /// (see PLAN.md): languages, memory in use, speed and accuracy.
    pub fn specs(self) -> [(&'static str, &'static str); 4] {
        let (languages, ram, speed, accuracy) = match self {
            ModelId::ParakeetV2 => ("English", "1.2 GB", "Very fast", "Highest"),
            ModelId::ParakeetV3 => ("25 European", "1.2 GB", "Very fast", "High"),
            ModelId::MoonshineMedium => ("English", "0.9 GB", "Moderate", "Good"),
            ModelId::MoonshineSmall => ("English", "0.65 GB", "Fast", "Fair"),
        };
        [
            ("Languages", languages),
            ("RAM", ram),
            ("Speed", speed),
            ("Accuracy", accuracy),
        ]
    }

    /// Total download size, for the progress bar.
    pub fn download_bytes(self) -> u64 {
        match self {
            ModelId::ParakeetV2 => 661_331_448,
            ModelId::ParakeetV3 => 670_619_706,
            ModelId::MoonshineMedium => 201_780_020,
            ModelId::MoonshineSmall => 104_842_676,
        }
    }

    /// Folder name under the models directory. Downloads in progress use
    /// files starting with this name too.
    pub fn dir_name(self) -> &'static str {
        match self {
            ModelId::ParakeetV2 => "parakeet-tdt-0.6b-v2-int8",
            ModelId::ParakeetV3 => "parakeet-tdt-0.6b-v3-int8",
            ModelId::MoonshineMedium => "moonshine-medium-streaming-en",
            ModelId::MoonshineSmall => "moonshine-small-streaming-en",
        }
    }

    pub fn dir(self) -> PathBuf {
        paths::model_dir(self.dir_name())
    }

    /// True when fetch-models.sh has finished installing this model. Parakeet
    /// files are verified and moved into place one by one, the last being
    /// vocab.txt; Moonshine folders appear only once fully extracted.
    pub fn is_installed(self) -> bool {
        let last = match self {
            ModelId::ParakeetV2 | ModelId::ParakeetV3 => "vocab.txt",
            ModelId::MoonshineMedium | ModelId::MoonshineSmall => "tokenizer.bin",
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

/// Boxed: the two model types differ greatly in size.
enum Model {
    Parakeet(Box<ParakeetModel>),
    Moonshine(Box<StreamingModel>),
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
            ModelId::ParakeetV2 | ModelId::ParakeetV3 => {
                ParakeetModel::load(&dir, &Quantization::Int8).map(|m| Model::Parakeet(Box::new(m)))
            }
            ModelId::MoonshineMedium | ModelId::MoonshineSmall => {
                // The .ort files in these downloads are unquantized.
                let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(4));
                StreamingModel::load(&dir, threads, &Quantization::FP32)
                    .map(|m| Model::Moonshine(Box::new(m)))
            }
        }
        .with_context(|| format!("loading {} from {}", id.key(), dir.display()))?;
        Ok(Self { id, model })
    }

    /// Transcribes 16 kHz mono f32 samples.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String> {
        let res = match &mut self.model {
            Model::Parakeet(m) => m.transcribe_with(samples, &ParakeetParams::default()),
            Model::Moonshine(m) => m.transcribe_with(samples, &MoonshineStreamingParams::default()),
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
