# Contributing to sayit

Thanks for helping! Forks, suggestions and pull requests are all welcome. sayit is small on purpose, and a few rules keep it private, light and trustworthy.

If you build something on top of sayit instead of contributing back, that's fine too. When you share it, keep the copyright notice (and the [NOTICE](NOTICE) file under Apache-2.0); see "Use, forks and contributions" in the README.

## Ground rules

1. **No network access in the binary.** Don't add HTTP, telemetry, crash reporting or update checks. `cargo tree -e normal | grep -iE 'http|reqwest|hyper|tokio'` must stay empty. Only `scripts/fetch-models.sh` downloads anything.
2. **Never persist audio or transcripts.** Audio stays in memory. Transcripts may be printed only in verbose mode (`-v`), never written to disk or logs.
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

## Before a release

- Run `cargo audit` (install it with `cargo install cargo-audit`), `cargo clippy --all-targets` and `cargo test`. All must be clean.
- Update `CHANGELOG.md` and the version in `Cargo.toml`.
- **If you publish a binary**, such as a DMG attached to a GitHub Release, include the license notices of the bundled dependencies. The MIT and Apache-2.0 licenses require this, and the binary statically links ONNX Runtime and the Rust crates. Generate the notices with a tool such as [`cargo-about`](https://github.com/EmbarkStudios/cargo-about) and add ONNX Runtime's `ThirdPartyNotices.txt`. Never bundle the speech model; users download it with `fetch-models.sh`.

## Pull requests

- Keep each PR focused on one change, and say how you tested it: which OS, which apps you dictated into, and any timings.
- Match the style of the surrounding code: short doc comments that explain *why*, and no extra abstraction layers.
- `unsafe` code needs a `// SAFETY:` comment explaining why it's sound.
- By submitting a contribution, you agree that it's dual licensed under MIT OR Apache-2.0 (see README).
