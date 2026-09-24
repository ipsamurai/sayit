# Contributing to sayit

Thanks for helping! Forks, suggestions and pull requests are all welcome. sayit is small on purpose, and a few rules keep it private, light and trustworthy.

If you build something on top of sayit instead of contributing back, that's fine too. When you share it, keep the copyright notice (and the [NOTICE](NOTICE) file under Apache-2.0); see "Use, forks and contributions" in the README.

## Ground rules

1. **No network access in the binary.** Don't add HTTP, telemetry or crash reporting. `cargo tree -e normal | grep -iE 'http|reqwest|hyper|tokio'` must stay empty. Anything that has to go online, like model downloads, runs through a bundled script using the system's `curl`, and only when the user asks.
2. **Never persist audio or transcripts.** Audio stays in memory. Transcripts may be printed only in verbose mode (`-v`), never written to disk or logs. The optional clipboard history is memory only.
3. **Keep it light.** The target is an 8 GB machine running on the CPU: each dictation should finish in well under 0.5 s and use about 1 GB of RAM. Include before-and-after numbers for changes that affect speed or memory (`sayit transcribe --model <m> clip.wav` prints them).
4. **Justify new dependencies.** Prefer crates that are already in `Cargo.lock`. Every dependency must be MIT, Apache-2.0 or similarly permissive.
5. **Models must allow any use** (MIT, Apache-2.0, or CC-BY-4.0 with attribution). Pin a source revision and a SHA-256 for every file. See [docs/MODELS.md](docs/MODELS.md).
6. **Least privilege.** Don't ask for new OS permissions or entitlements unless the feature can't work without them, and explain why in the PR.

## Building and testing

```sh
./scripts/fetch-models.sh
cargo build --release
cargo test
./target/release/sayit app -v
```

During development, run the menu-bar app from a terminal. The terminal's Accessibility permission survives rebuilds, but an ad-hoc signed `.app` loses its permission each time you rebuild it.

Linux changes are very welcome. That code path has never been run, so please say what you tested it on.

**Porting to another platform** (such as Windows): keep the platform's code behind `#[cfg(target_os = "...")]` in its own functions or modules, the way the macOS and Linux code is split in `src/inject.rs`, `src/daemon.rs` and `src/ui/`, so it never changes how the other platforms build. The ground rules above apply unchanged. Open an issue first to agree on the approach, then send the work in small PRs.

## Before a release

Follow the step-by-step checklist in [RELEASING.md](RELEASING.md). In short:

- Run `cargo audit` (install it with `cargo install cargo-audit`), `cargo clippy --all-targets` and `cargo test`. All must be clean.
- Update `CHANGELOG.md` and the version in `Cargo.toml`.
- Publish the DMG's SHA-256 checksum next to it. `scripts/package-dmg.sh` writes it as `sayit-<version>.dmg.sha256`.
- **If you publish a binary**, such as a DMG attached to a GitHub Release, it must carry the license notices of everything compiled into it (the MIT and Apache-2.0 licenses require this). `scripts/bundle-macos.sh` does this for you: it runs `scripts/third-party-notices.py`, which lists every bundled Rust crate with its license text, plus ONNX Runtime and its own notices from `assets/licenses/`. The app contains the result as `Contents/Resources/THIRD-PARTY-NOTICES.txt`. If `ort-sys` is upgraded, the script stops until the ONNX Runtime notices are refreshed. Never bundle the speech model; users download it with `fetch-models.sh`.

## Pull requests

- Keep each PR focused on one change, and say how you tested it: which OS, which apps you dictated into, and any timings.
- Match the style of the surrounding code: short doc comments that explain *why*, and no extra abstraction layers.
- `unsafe` code needs a `// SAFETY:` comment explaining why it's sound.
- AI-assisted changes are welcome, but you must understand and test everything you submit, and say that an AI tool helped. See [AI_POLICY.md](AI_POLICY.md).
- By submitting a contribution, you agree that it's dual licensed under MIT OR Apache-2.0 (see README).
