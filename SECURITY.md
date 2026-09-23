# Security policy

sayit can see what you say and type into other apps, so security reports are taken seriously.

## Reporting a vulnerability

**Please don't open a public issue.** Report it privately through GitHub instead: go to the repository's **Security** tab and choose **Report a vulnerability**, which opens a private advisory. Include:

- what the issue is and what an attacker could do with it;
- steps to reproduce, and the affected version or commit;
- your OS and version.

You should get a reply within 7 days. Once a fix is released, the advisory will be published with credit to you, unless you'd rather stay anonymous.

## Supported versions

Only the latest release and the `main` branch receive fixes.

## Scope and design

In scope:
- anything that sends audio, transcripts or clipboard contents off the machine, or writes them to disk;
- anything that lets another process or user trigger recording, read dictated text, or inject input through sayit;
- tampering with the downloaded model, for example by bypassing the checksum checks in `scripts/fetch-models.sh`;
- memory-safety bugs in sayit's own `unsafe` code.

What sayit does by design:
- It runs with the user's own privileges and asks only for **Microphone** and **Accessibility**, which it needs to detect the hotkey and paste. It doesn't need admin rights.
- On macOS, the app is signed with the **Hardened Runtime** and a single entitlement, `com.apple.security.device.audio-input`. This blocks code injection (e.g. `DYLD_INSERT_LIBRARIES`) that would otherwise let another process borrow sayit's permissions.
- The global hotkey listener sees every key event, but only to match the configured hotkey. Keys are never stored, logged or sent anywhere.
- The microphone is open only while the hotkey is held, or for 1.5 seconds when the user clicks Test Microphone in Settings. The test audio stays in memory and is discarded after measuring its level.
- Clipboard history is off by default. When on, the last 3 to 10 dictations are kept in memory only, never on disk, and are cleared on quit, on **Clear**, or when the option is turned off.
- Settings › About opens only a fixed list of GitHub addresses, in the user's browser, and the license notices file inside the app. The app itself makes no network connections.
- Pasted text passes through the system clipboard for about 250 ms. It's marked Transient/Concealed so clipboard managers skip it, and your previous clipboard is restored afterwards. Other apps running as the same user can still read the clipboard during that window. That's a limitation of pasting through the clipboard, not something sayit can prevent.
- On Linux, the udev rule in the README gives every program the user runs access to input devices, which Wayland global hotkeys require. The README calls out this trade-off.

## How sayit is checked

Before each release:
- `cargo audit` checks every dependency against the RustSec advisory database;
- `cargo clippy` runs with `undocumented_unsafe_blocks`, so every `unsafe` block carries a `// SAFETY:` comment;
- the code is checked for network access with `cargo tree -e normal`.
