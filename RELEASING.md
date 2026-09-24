# Releasing sayit

A checklist for publishing a version as a GitHub Release with a DMG attached. In the commands, `X.Y.Z` stands for the new version.

## 1. Choose the version

Versions follow [Semantic Versioning](https://semver.org/). Before 1.0, new features bump the middle number (0.1.0 → 0.2.0) and fixes only bump the last (0.2.0 → 0.2.1).

## 2. Update the version and the changelog

- Set `version = "X.Y.Z"` in `Cargo.toml`, then run `cargo build --release` so `Cargo.lock` picks it up.
- In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` (today's date), and add a new, empty `## [Unreleased]` above it.
- If the README's status line mentions a version, update it.

## 3. Checks

All must be clean:

```sh
cargo audit                      # install with: cargo install cargo-audit
cargo clippy --all-targets -- -D warnings
cargo test
```

Then search the tree for anything that identifies your own machine or you: your username, home-folder paths, email address, hardware model. Use a Perl-style search, since `git grep -E` doesn't understand `\b`:

```sh
git grep -nIiP '<your terms here>' -- ':!assets/licenses'
```

`assets/licenses/` holds ONNX Runtime's notices verbatim. They list many contributors' email addresses, which are expected and must not be edited.

## 4. Build

```sh
./scripts/package-dmg.sh
```

This builds `sayit.app` (with `THIRD-PARTY-NOTICES.txt` inside, see [CONTRIBUTING.md](CONTRIBUTING.md#before-a-release)), then writes to `target/release/`:
- `sayit-X.Y.Z.dmg`
- `sayit-X.Y.Z.dmg.sha256`, its SHA-256 checksum

## 5. Test the DMG

Test the DMG you're about to upload, not a build run from a terminal:

- `cd target/release && shasum -a 256 -c sayit-X.Y.Z.dmg.sha256` prints `OK`.
- Open the DMG, drag sayit to Applications and start it. Setup, the model download, permissions and a dictation all work.
- Settings › About shows `X.Y.Z`, and **Licenses** opens the notices file.

## 6. Commit and tag

```sh
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "Release X.Y.Z"
git tag -a vX.Y.Z -m "sayit X.Y.Z"
git push origin main vX.Y.Z
```

## 7. Publish the GitHub Release

Use this version's CHANGELOG section as the release notes:

```sh
awk '/^## \[X\.Y\.Z\]/{f=1; next} /^## \[/{f=0} f' CHANGELOG.md > target/release/notes.md
```

Add a short **Installing** note at the top of `notes.md`:
- Download the DMG and check it: `shasum -a 256 -c sayit-X.Y.Z.dmg.sha256`.
- sayit is ad-hoc signed, not notarized by Apple, so macOS blocks the first launch. Open it once, then go to System Settings › Privacy & Security and click **Open Anyway**. On macOS 14 and earlier, you can instead right-click the app and choose **Open**.

Then create the release with both files attached:

```sh
gh release create vX.Y.Z \
  target/release/sayit-X.Y.Z.dmg \
  target/release/sayit-X.Y.Z.dmg.sha256 \
  --title "sayit X.Y.Z" --notes-file target/release/notes.md
```

Afterwards, open the release page. Check that both files are there, then download the DMG and run the checksum check on the downloaded copy.
