# sayit

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**Private, local-only dictation for lower-end machines.** Hold a key, speak, and release: your words are typed into whatever app you're using. Nothing leaves your computer.

- 🔒 **Private and local.** The app contains no networking code and collects nothing: no account, no sign-up, no telemetry. See [PRIVACY.md](PRIVACY.md).
- 🎙️ **Audio never touches disk.** It's held in memory only while you speak, then discarded.
- 🪶 **Light.** It runs on the CPU, with no GPU needed. It uses about 1 GB of RAM while the model is loaded and typically takes 0.1–0.3 s per sentence on an entry-level 8 GB laptop.
- 🖥️ **Menu-bar app** (macOS) with status, pause, and a microphone picker.

> **Status: early (0.1).** macOS is supported. The Linux code path (Wayland/X11) is written but has **not been tested yet**. English only for now.

## Contents
- [Requirements](#requirements)
- [Quick start (macOS)](#quick-start-macos)
- [Using sayit](#using-sayit)
- [Configuration](#configuration)
- [Updating and uninstalling](#updating-and-uninstalling)
- [Troubleshooting](#troubleshooting)
- [Linux (untested)](#linux-untested)
- [Privacy and security](#privacy-and-security)
- [Speech model](#speech-model)
- [Contributing](#contributing)
- [License](#license)
- [Disclaimer](#disclaimer)

## Requirements

- **macOS 13 (Ventura) or later**, with 8 GB of RAM recommended
- About 1 GB of free disk space for the speech model and the app
- **To build:** [Rust](https://rustup.rs) (stable) and the Xcode command-line tools (`xcode-select --install`)

sayit is currently distributed as source code. You build it yourself, which takes about two minutes.

## Quick start (macOS)

```sh
git clone https://github.com/ipsamurai/sayit.git
cd sayit
./scripts/package-dmg.sh      # builds target/release/sayit-<version>.dmg
open target/release/sayit-*.dmg
```

1. Drag **sayit** into **Applications** and open it. A microphone icon appears in the menu bar; sayit has no Dock icon.
2. The **setup assistant** walks you through choosing and downloading a speech model, allowing **Accessibility** and **Microphone** access (with a live green/red checklist), picking your microphone and hotkey, and the text options.
3. Click into any text field, **hold Right Option** (or your chosen key), and speak. Release, and the text appears.

If you close or quit setup partway, the menu bar offers **Continue Setup…**, and setup starts again next time.

The app is ad-hoc signed, not notarized by Apple. On the Mac that built it, it opens normally. If you copy it to another Mac, Gatekeeper blocks the first launch: right-click the app and choose **Open**.

## Using sayit

| To… | Do this |
|---|---|
| Dictate | Hold the hotkey (default **Right Option**), speak, release |
| Pause | Menu bar › **Pause Dictation**. The hotkey is ignored and any take in progress is discarded. |
| Choose a microphone | Menu bar › **Microphone** |
| Change settings | Menu bar › **Settings…** (⌘,): Models, Text and Permissions tabs. Changes apply immediately. |
| Check permissions | Settings › **Permissions**: a live checklist, a **Test Microphone** button, and step-by-step fixes. The menu bar shows **Fix Permissions…** if one goes missing. |
| Quit | Menu bar › **Quit sayit** (⌘Q) |

### Command line
The same binary also runs from a terminal:

| Command | What it does |
|---|---|
| `sayit app -v` | The menu-bar app, with timings and transcripts printed to the terminal |
| `sayit run -v` | Dictation without a menu-bar icon |
| `sayit listen` | Press Enter to start and stop; prints the text and timings |
| `sayit transcribe file.wav` | Transcribes a WAV file (useful for benchmarks) |

When you run sayit from a terminal, macOS grants permissions to the **terminal app** (Terminal, iTerm…), not to sayit. Enable your terminal under *Privacy & Security › Accessibility*, then quit the terminal fully (⌘Q) and reopen it.

## Configuration

sayit creates `config.toml` on first run and saves menu choices to it. It lives in `~/Library/Application Support/sayit/` on macOS and `~/.config/sayit/` on Linux.

```toml
hotkey = "OptRight"            # or "Fn", "CtrlRight", "Ctrl+Alt+Space"
mode = "hold"                  # or "toggle": press once to start, again to stop
model = "parakeet-v2"
restore_clipboard = true       # put your previous clipboard back after pasting
remove_fillers = true          # drop "um", "uh", "erm" from what you dictate
newline_after_take = true      # each dictation ends with a line break (false: a space)
unload_after_idle_mins = 0     # e.g. 10 to free ~1 GB of RAM when idle (reload takes <1 s)
max_recording_secs = 300       # capped at 3600
# input_device = "Built-in Microphone"   # unset = system default
```

Restart sayit after editing the file by hand.

## Updating and uninstalling

**Update:**
1. Run `git pull` and then `./scripts/package-dmg.sh`.
2. Replace the app in Applications with the new one.
3. The new build counts as a different app to macOS, so run `tccutil reset Accessibility io.github.ipsamurai.sayit` and allow it again when prompted.

**Uninstall:**
```sh
osascript -e 'quit app "sayit"'
rm -rf /Applications/sayit.app
rm -rf ~/Library/Application\ Support/sayit    # config and speech model
tccutil reset Accessibility io.github.ipsamurai.sayit
tccutil reset Microphone io.github.ipsamurai.sayit
```

## Troubleshooting

| Problem | Fix |
|---|---|
| The first words are cut off | Bluetooth headset microphones take about a second to switch on. Choose the built-in mic under **Microphone**. |
| Something doesn't respond | Open Settings › **Permissions**. Both rows should be green, and **Test Microphone** should say it can hear you. The tab lists the manual fixes. |
| The hotkey stopped working after a rebuild | macOS still shows sayit as allowed, but the permission belongs to the old build. Run `tccutil reset Accessibility io.github.ipsamurai.sayit`, relaunch, and allow it again. |
| The menu says "No speech model yet" | Open **Settings** and download a model. Dictation starts as soon as it finishes. |
| Nothing is pasted in some apps | sayit pastes with ⌘V. Password fields and apps that block synthetic keystrokes won't accept it. |
| The wrong character is pasted on Dvorak or other non-QWERTY layouts | This is a known limitation, and a fix is planned. |
| Dictating into a terminal runs the text as a command | Each dictation ends with a line break, which works like pressing Return. Modern shells (zsh, which is the macOS default, and bash 5.1+) don't run pasted text, but older shells, some SSH sessions and some REPLs do. Set `newline_after_take = false` if you dictate into terminals. |
| Short or odd words appear when you didn't really speak | Speak for at least half a second. Very short or near-silent takes can produce stray words. |

## Linux (untested)

sayit needs read access to `/dev/input/event*` for the hotkey and write access to `/dev/uinput` for pasting. This udev rule grants both to the logged-in user:

```sh
sudo tee /etc/udev/rules.d/70-sayit.rules <<'EOF'
KERNEL=="uinput", TAG+="uaccess"
SUBSYSTEM=="input", KERNEL=="event*", TAG+="uaccess"
EOF
sudo udevadm control --reload && sudo udevadm trigger
```

> ⚠️ **Security trade-off.** This rule lets *every* program you run read all keyboard input and create virtual input devices. That's what any global-hotkey tool on Wayland needs, but only install it if you trust the software you run. To undo it, delete the file and run the two `udevadm` commands again.

Build dependency: `libasound2-dev` (ALSA headers). The menu-bar app is macOS-only for now; use `sayit run` on Linux.

## Privacy and security

sayit collects no data and needs no account. The full details, including how to check each claim yourself, are in [PRIVACY.md](PRIVACY.md). In short:

- **No network code.** `cargo tree -e normal | grep -iE 'http|reqwest|hyper|tokio'` prints nothing. The only downloads are:
  - the speech model, fetched by `scripts/fetch-models.sh` from a pinned revision, with every file checked against its SHA-256 hash;
  - ONNX Runtime, which the `ort` crate downloads and hash-checks while building.
- **Audio** stays in memory and is discarded after each dictation. The microphone is open only while you hold the key.
- **Transcripts** are never written to disk or logs. They're printed only when you run with `-v`.
- **Clipboard.** Pasted text is marked Transient/Concealed so clipboard managers skip it, and your previous clipboard is restored about 250 ms later.
- **Least privilege.** sayit asks only for Microphone and Accessibility. The app is signed with the macOS Hardened Runtime, which blocks other programs from injecting code to borrow those permissions. Its only entitlement is microphone access.

To report a vulnerability, see [SECURITY.md](SECURITY.md). Please don't open a public issue.

## Speech model

Choose and download a model in **Settings › Speech model**, or with `./scripts/fetch-models.sh <name>`. Switching models takes effect on your next dictation.

| Model | Languages | RAM | Speed | Accuracy | Download | License |
|---|---|---|---|---|---|---|
| **Parakeet v2** (default) | English | 1.2 GB | Very fast | Highest | 660 MB | [CC-BY-4.0](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2) (NVIDIA) |
| Parakeet v3 | 25 European | 1.2 GB | Very fast | High | 670 MB | [CC-BY-4.0](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) (NVIDIA) |
| Moonshine Medium | English | 0.9 GB | Moderate | Good | 200 MB | [MIT](https://github.com/moonshine-ai/moonshine) (Moonshine AI) |
| Moonshine Small | English | 0.65 GB | Fast | Fair | 105 MB | [MIT](https://github.com/moonshine-ai/moonshine) (Moonshine AI) |

The benchmark behind these ratings is in [PLAN.md](PLAN.md#model-choice-entry-level-8-gb-laptop-cpu-only). Models aren't part of this repository: they're downloaded from Hugging Face and each file is checked against a pinned SHA-256 checksum. If you redistribute a model, follow its license. [docs/MODELS.md](docs/MODELS.md) explains how to add others.

## Contributing

Contributions are welcome, especially testing on Linux. Please read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md). Changes are listed in [CHANGELOG.md](CHANGELOG.md), and the roadmap is in [PLAN.md](PLAN.md).

## Use, forks and contributions

sayit is free to use, copy, modify and share, for personal or commercial purposes.

- **Fork it** and build on it.
- **Suggest changes** by opening an issue.
- **Send a pull request.** Changes that fit the project's goals (private, local, light) will be reviewed and may be merged; see [CONTRIBUTING.md](CONTRIBUTING.md).

**Credit when you share it.** Using or modifying sayit for yourself needs no permission and no credit. When you **redistribute** it, whether as copies, a published fork, or a project that includes sayit's code, the license requires you to keep the copyright notice. That notice names **ipsamurai** as the original author and links to https://github.com/ipsamurai/sayit. Under Apache-2.0 you must also include the [NOTICE](NOTICE) file. Beyond that, we'd appreciate a mention of the original project in your README.

## License

Copyright © 2026 ipsamurai and the sayit contributors ([github.com/ipsamurai/sayit](https://github.com/ipsamurai/sayit)).

sayit is licensed under either the [Apache License, Version 2.0](LICENSE-APACHE) or the [MIT License](LICENSE-MIT), at your option. Attribution notices are in [NOTICE](NOTICE).

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in sayit, as defined in the Apache-2.0 license, is dual licensed as above, without any additional terms or conditions.

## Disclaimer

sayit is provided **"as is", without warranty of any kind**, and its authors aren't liable for any harm arising from its use. **You're responsible for how you use it:** lawfully, with the consent of anyone whose voice you record, and after checking what it writes. Read the full [DISCLAIMER.md](DISCLAIMER.md) before using sayit.

sayit is an independent project. It is not affiliated with, endorsed by or sponsored by Apple, NVIDIA or any other company named here. Apple, macOS and Mac are trademarks of Apple Inc., and NVIDIA is a trademark of NVIDIA Corporation. They're used here only to describe compatibility and the model in use.
