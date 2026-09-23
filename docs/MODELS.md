# Speech models

sayit offers four models. You can download them from **Settings › Speech model** or with `scripts/fetch-models.sh <key>`:

| Key | Model | Languages | License |
|---|---|---|---|
| `parakeet-v2` (default) | NVIDIA Parakeet TDT 0.6B v2, int8 ONNX | English | CC-BY-4.0 |
| `parakeet-v3` | NVIDIA Parakeet TDT 0.6B v3, int8 ONNX | 25 European | CC-BY-4.0 |
| `moonshine-medium` | Moonshine v2 Medium, streaming | English | MIT |
| `moonshine-small` | Moonshine v2 Small, streaming | English | MIT |

Every file comes from a pinned source, either an exact Hugging Face revision or a fixed archive, and is checked against its SHA-256 before use. The pins are in `scripts/fetch-models.sh`, and the benchmarks are in [PLAN.md](../PLAN.md#model-choice-entry-level-8-gb-laptop-cpu-only).

## Adding a model

1. **`src/stt.rs`:** add a `ModelId` variant and include it in `ALL`. Fill in `key()`, which is the name used in `config.toml` and by the fetch script, plus `label()`, `summary()`, `download_bytes()` (for the progress bar), `dir_name()` and `is_installed()`.
2. **`src/stt.rs`:** if the model belongs to a new family, add a `Model` variant with a loader arm and a transcribe arm in `Engine`. [transcribe-rs](https://crates.io/crates/transcribe-rs) (the `onnx` feature, already enabled) also supports Canary, Cohere, SenseVoice and GigaAM, so this needs no new dependencies.
3. **`scripts/fetch-models.sh`:** add a function that fetches each file from a **pinned** source with its SHA-256, and add it to `AVAILABLE` and the `case`.

The model then appears in Settings and in the menu bar's **Model** menu.

Only use models whose weights allow any use: MIT, Apache-2.0, or CC-BY-4.0 with attribution. Add the model to the table in the README.

## Benchmarked but not offered

**Cohere Transcribe 03-2026** (Apache-2.0, 14 languages) was the most accurate model on the server leaderboard. On a laptop CPU, though, it took about 2 s for a 5-second clip and used 2.5 GB of memory, which is too slow and heavy for real-time dictation. Its pinned source is kept here in case that changes:

- `huggingface.co/cstr/cohere-transcribe-onnx-int4` @ `2c870f89692c61348481884eb6e1222ec3ba25c9`
- `cohere-encoder.int4.onnx` 1bd619df7113a27d75b136ddc9f35d6b65131baeec15a7680ddda0c18c0b20a5
- `cohere-encoder.int4.onnx.data` f5bffddbf657edff4b14e650926cdb39b62a8644feb9f6ee4e3287df38e7f900
- `cohere-decoder.int4.onnx` f176a71e8b743d32bb8e4b8ec19bf70f367c5c3c93b08857826d8f5b60b6ad1e
- `cohere-decoder.int4.onnx.data` ed9461dd2233c44cd86f56c0a4d17c089de317bdbdcf18dc92e44a8a0cf525b3
- `tokens.txt` 013ede043ae2480e3a9205cc34550d9686100cc682bacc90f702facdfbb93035

It loads with `onnx::cohere::CohereModel::load(dir, &Quantization::Int4)` and needs `CohereParams { language: Some("en"), .. }`.
