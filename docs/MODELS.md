# Speech models

sayit ships with one model: **NVIDIA Parakeet TDT 0.6B v2** (English, int8 ONNX, CC-BY-4.0). `scripts/fetch-models.sh` downloads it from a pinned Hugging Face revision and checks every file's SHA-256.

See [PLAN.md](../PLAN.md#model-choice-m2-8-gb-2026-09-23) for why it was chosen.

## Adding a model

1. **`src/stt.rs`:** add a `ModelId` variant, then fill in `key()`, `label()`, `dir()` and `is_installed()`. The `key` is the name used in `config.toml` and by the fetch script. Add it to `ALL`.
2. **`src/stt.rs`:** if the model belongs to a new family, add a `Model` variant plus a loader arm and a transcribe arm in `Engine`. [transcribe-rs](https://crates.io/crates/transcribe-rs) (the `onnx` feature, already enabled) also supports Moonshine, Canary, Cohere, SenseVoice and GigaAM, so this needs no new dependencies.
3. **`scripts/fetch-models.sh`:** add a function that fetches each file from a **pinned revision** with its SHA-256. Then add it to `AVAILABLE` and the `case`.
4. Once `ModelId::ALL` has more than one entry, the **Model** menu appears in the menu bar and switches models live.

Only use models whose weights allow any use: MIT, Apache-2.0, or CC-BY-4.0 with attribution. Add the model to the license table in the README.

## Tested candidates (not shipped)

These were benchmarked on an entry-level 8 GB laptop, CPU only (results in PLAN.md), and all worked with the steps above.

| Key | Model | Source (pinned) | Files and SHA-256 |
|---|---|---|---|
| `parakeet-v3` | Parakeet TDT 0.6B v3 int8, 25 languages, CC-BY-4.0 | `huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx` @ `8f23f0c03c8761650bdb5b40aaf3e40d2c15f1ce` | `encoder-model.int8.onnx` 6139d2fa7e1b086097b277c7149725edbab89cc7c7ae64b23c741be4055aff09 · `decoder_joint-model.int8.onnx` eea7483ee3d1a30375daedc8ed83e3960c91b098812127a0d99d1c8977667a70 · `nemo128.onnx` a9fde1486ebfcc08f328d75ad4610c67835fea58c73ba57e3209a6f6cf019e9f · `vocab.txt` d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d |
| `moonshine-small` | Moonshine v2 Small streaming, English, MIT | `blob.handy.computer/moonshine-small-streaming-en.tar.gz` | tarball dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5 |
| `moonshine-medium` | Moonshine v2 Medium streaming, English, MIT | `blob.handy.computer/moonshine-medium-streaming-en.tar.gz` | tarball 07a66f3bff1c77e75a2f637e5a263928a08baae3c29c4c053fc968a9a9373d13 |
| `cohere` | Cohere Transcribe 03-2026 int4, 14 languages, Apache-2.0 | `huggingface.co/cstr/cohere-transcribe-onnx-int4` @ `2c870f89692c61348481884eb6e1222ec3ba25c9` | `cohere-encoder.int4.onnx` 1bd619df7113a27d75b136ddc9f35d6b65131baeec15a7680ddda0c18c0b20a5 · `cohere-encoder.int4.onnx.data` f5bffddbf657edff4b14e650926cdb39b62a8644feb9f6ee4e3287df38e7f900 · `cohere-decoder.int4.onnx` f176a71e8b743d32bb8e4b8ec19bf70f367c5c3c93b08857826d8f5b60b6ad1e · `cohere-decoder.int4.onnx.data` ed9461dd2233c44cd86f56c0a4d17c089de317bdbdcf18dc92e44a8a0cf525b3 · `tokens.txt` 013ede043ae2480e3a9205cc34550d9686100cc682bacc90f702facdfbb93035 |

Loader notes:
- **Moonshine** tarballs extract to a directory of `.ort` files. Load them with `onnx::moonshine::StreamingModel::load(dir, threads, &Quantization::FP32)`.
- **Cohere** loads with `onnx::cohere::CohereModel::load(dir, &Quantization::Int4)` and needs `CohereParams { language: Some("en"), .. }`.
