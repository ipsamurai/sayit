# Changelog

All notable changes to sayit are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/).

## [0.1.0] - Unreleased

The first public release.

### Added
- Local push-to-talk dictation: hold a global hotkey (default Right Option), speak, and release. The text is pasted into the focused app and the previous clipboard is restored.
- Speech recognition with NVIDIA Parakeet TDT 0.6B v2 (int8 ONNX) on the CPU. A pluggable model layer makes it easy to add other models later.
- macOS menu-bar app with status (loading, ready, listening, transcribing, paused, error), Pause, and a microphone picker. It has no Dock icon.
- `sayit.app` bundle and drag-to-install DMG, ad-hoc signed with the Hardened Runtime.
- `sayit run` (terminal mode), `sayit listen` and `sayit transcribe` (for testing and benchmarks).
- `scripts/fetch-models.sh`: model download from a pinned revision, with SHA-256 verification.
- Linux (Wayland/X11) code path. **It hasn't been tested yet.**
