# Changelog

All notable changes to sayit are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Filler-word removal: "um", "uh", "erm" and similar hesitation sounds are dropped from transcripts, with punctuation and capitalization tidied up. Turn it off with `remove_fillers = false`.
- Each dictation now ends with a line break, so the next one starts on a new line. `newline_after_take = false` uses a space instead.
- Settings window (menu bar › Settings…, ⌘,) with the text options. Changes apply to the next dictation without a restart.
- Only one copy of sayit runs at a time; a second copy exits instead of pasting everything twice.
- Model manager in Settings: download (with progress and cancel), switch or delete Parakeet v2, Parakeet v3, Moonshine Medium and Moonshine Small. Downloads run the bundled, checksum-verifying `fetch-models.sh`, so the app itself still has no network code.
- On first launch without a model, Settings opens and dictation starts as soon as a download finishes.
- Setup assistant on first launch: welcome, model download, permissions checklist (live green/red), microphone, hotkey (key and hold/toggle), text options, and a how-to. Closing or quitting partway is safe: the menu bar offers Continue Setup…, and it restarts next launch.
- Settings › Permissions: live checklist, Test Microphone (records 1.5 seconds in memory and reports whether it heard you), and manual steps. The menu bar shows Fix Permissions… when macOS reports a permission missing.

## [0.1.0] - 2026-09-23

The first public release.

### Added
- Local push-to-talk dictation: hold a global hotkey (default Right Option), speak, and release. The text is pasted into the focused app and the previous clipboard is restored.
- Speech recognition with NVIDIA Parakeet TDT 0.6B v2 (int8 ONNX) on the CPU. A pluggable model layer makes it easy to add other models later.
- macOS menu-bar app with status (loading, ready, listening, transcribing, paused, error), Pause, and a microphone picker. It has no Dock icon.
- `sayit.app` bundle and drag-to-install DMG, ad-hoc signed with the Hardened Runtime.
- `sayit run` (terminal mode), `sayit listen` and `sayit transcribe` (for testing and benchmarks).
- `scripts/fetch-models.sh`: model download from a pinned revision, with SHA-256 verification.
- Linux (Wayland/X11) code path. **It hasn't been tested yet.**
