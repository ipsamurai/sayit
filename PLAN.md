# sayit — plan

Local-only, private, fast dictation for macOS and Linux (Wayland). Hold a key, speak, release: the text appears in the focused field.

## Constraints
- **Everything runs locally.** The `sayit` binary has no networking code; `cargo tree -e normal` has no HTTP crates. Models come from `scripts/fetch-models.sh`, which pins the source revision and checks SHA-256. The only network access at build time is `ort-sys` fetching a hash-verified ONNX Runtime.
- **Built for lower-end hardware: laptops with 8 GB of RAM, running on the CPU with no GPU.** A dictation must finish in under 0.5 s after key release. Idle memory stays around 1 GB, with an option to unload the model when idle.
- **Platforms:** macOS 13+ and Linux Wayland (GNOME/KDE), with X11 as a bonus.

## Architecture
```
handy-keys (CGEventTap / evdev) ─► controller ─► cpal mic (open only while recording)
                                         │
                                         ▼  16 kHz mono, in memory only
                               worker: trim silence → Parakeet TDT 0.6B v2 (ONNX int8, CPU)
                                         │
                                         ▼
                               filler removal (P4) → vault (P3) → injector (+ line break)
                                                      macOS: NSPasteboard + CGEvent Cmd+V
                                                      Linux: arboard + uinput Ctrl+V
```

## Model choice (entry-level 8 GB laptop, CPU only)
Every model was run through `sayit transcribe --model` on the same text-to-speech clips, then compared in live dictation. Peak RSS includes the process.

| Model | License | 5 s clip | 18 s clip | Peak RSS | Open ASR English WER | Result |
|---|---|---|---|---|---|---|
| **Parakeet TDT 0.6B v2 int8** | CC-BY-4.0 | **213 ms** | **760 ms** | 1.24 GB | 6.05 % | **Chosen.** Fastest and most accurate in live use. |
| Parakeet TDT 0.6B v3 int8 | CC-BY-4.0 | 223 ms | 751 ms | 1.25 GB | 6.32 % | Dropped. Covers 25 languages but was less accurate in English ("Cuban Eats" for "Kubernetes"). |
| Moonshine v2 Medium | MIT | 553 ms | 2.0 s | 0.92 GB | 6.65 % | Dropped. Slower, and weaker in live use. |
| Moonshine v2 Small | MIT | 367 ms | 1.0 s | 0.65 GB | 7.84 % | Dropped. Weaker in live use. It's the candidate if a low-RAM tier is needed. |
| Cohere Transcribe 2B int4 | Apache-2.0 | ~2 s | ~6 s | 2.5 GB | 5.42 % | Dropped. Too slow and heavy for real-time CPU dictation. |

Models with more restrictive terms, such as the NVIDIA Open Model License or non-commercial licenses, weren't considered. The code keeps a model list (`stt::ModelId`), a loader per model family, and a fetch function per model, so a model can be added back in a few lines. The pinned sources and hashes for the dropped models are in [docs/MODELS.md](docs/MODELS.md). The **Model** menu appears automatically once more than one model is listed.

## Backend benchmarks (entry-level 8 GB laptop, Parakeet v3 int8)
| Backend | 4 s clip | 21 s clip | Load | Peak RSS |
|---|---|---|---|---|
| **CPU (chosen)** | **219 ms** | **1.0 s** | 0.7 s | 1.05–1.25 GB |
| XNNPACK | 337 ms | 2 s | 0.7 s | 1.2 GB |
| CoreML | 733 ms | 5 s | 5 s | 1.2 GB |

## Phases
- [x] **P1 Core:** mic capture, resampler, silence trim, Parakeet engine, `listen` and `transcribe` benchmarks.
- [x] **P2 Daemon:** global hold or toggle hotkey, paste with clipboard save and restore, privacy markers for clipboard managers, TOML config, idle unload. *The Linux code path is written but has not been compiled or tested yet.*
- [ ] **P3 History vault:** encrypted history of recent dictations with a TTL, unlocked by PIN or Touch ID.
  - Entries are text only and never include audio. They're encrypted with XChaCha20-Poly1305 in `data_dir/vault.bin` and each carries an `expires_at`. The default TTL is 24 h. Expired entries are purged on a timer and at startup.
  - A random 256-bit data key (DEK) encrypts the entries.
    - macOS: the DEK sits in the Keychain with a `SecAccessControl` of `.biometryCurrentSet | .or | .devicePasscode`, so Touch ID releases it. A PIN-wrapped copy serves as the fallback.
    - Linux: the DEK is wrapped with a key derived from `HKDF(Argon2id(PIN, salt) ‖ secret-service secret)`. The secret sits in GNOME Keyring or KWallet, so a stolen `vault.bin` can't be brute-forced offline without the unlocked login keyring.
  - The vault locks again after N idle minutes. Plaintext is held in `Zeroizing` buffers, and 5 wrong PINs trigger an exponential backoff.
  - A `sayit history` CLI shows entries and can copy one, marked concealed. The tray menu gets a history view in P5.
- [~] **P4 Text cleanup:**
  - *Done:* filler words (um, uh, erm) are removed, and each take ends with a line break (`newline_after_take`).
  - *Dropped:* voice commands ("new line") and context-aware spacing, which would mean reading other apps' text or processes. They risk false triggers, and the privacy cost outweighs the benefit.
  - *Last:* a personal dictionary. It's deferred because replacements can misfire on similar-sounding phrases, and the model already handles most words.
- [~] **P5 Tray and overlay:** *Done on macOS:* menu-bar icon for loading, idle, listening, transcribing, paused and error; Pause, Microphone picker, Model picker (shown when there is more than one model) and Quit. *Still to do:* floating "listening" pill, history view, and the Linux tray.
- [ ] **P6 Optional LLM cleanup:** Qwen3-1.7B Q4 through llama.cpp with Metal, toggled per use. It adds about 1.1 GB of RAM and 0.3–1 s per dictation.
- [~] **P7 Packaging:** *Done:* macOS `.app` (menu-bar app, optional Dock icon, ad-hoc signed with the Hardened Runtime, own permissions), app icon, drag-to-install `.dmg`, start at login (SMAppService). *Still to do:* an optional stable local signing identity so permissions survive rebuilds, and on Linux an AppImage or `.deb`, a udev `uaccess` rule and a systemd user unit.

## Known limitations
- Clipboard restore on Linux keeps plain text only. On macOS all pasteboard types are kept.
- On macOS, Cmd+V uses the ANSI keycode for V, so non-QWERTY layouts such as Dvorak need a fix in P5.
- Terminals on Linux paste with Ctrl+Shift+V. A per-app override is planned.
- On Linux, pasted text isn't yet marked as excluded from clipboard-manager history, as it is on macOS.
