# Privacy

**sayit collects nothing.** It has no accounts, no sign-up, no analytics, no telemetry, no crash reports and no ads. Everything happens on your own computer, and nothing you say or dictate ever leaves it.

## What stays on your computer

sayit keeps only these files, all in `~/Library/Application Support/sayit/` on macOS (`~/.local/share/sayit/` and `~/.config/sayit/` on Linux). The folder is readable only by your user account:

| File | Contents |
|---|---|
| `config.toml` | Your settings: hotkey, microphone name, toggles |
| `models/` | The speech model you downloaded |
| `sayit.lock` | An empty file that stops two copies of sayit from running |

If you turn on **Autostart** (Settings › General), macOS adds sayit to *System Settings › General › Login Items*. Turning it off removes it.

sayit also asks macOS every 2 seconds whether it still has its two permissions, so it can warn you if one is revoked. That's a local check; nothing is sent anywhere.

**Never stored:**
- **Audio:** held in memory only while you hold the hotkey, then discarded.
- **What you dictate:** it's pasted into your app and then forgotten. It's never written to disk or to logs, and it's printed only if you run `sayit` with `-v` in your own terminal.

**Clipboard history (off unless you turn it on):** with **Clipboard history** on (in setup or Settings › Clipboard), sayit keeps your last 3, 5 or 10 dictations **in memory only**, so you can copy one again from the menu bar's **Clipboard** menu. They're never written to disk or logs. They're gone when sayit quits, when you choose **Clear**, or when you turn the option off. Clicking one copies it to the clipboard like a normal copy, so a clipboard manager may record that copy.

If a future feature stores anything else, it will be off by default and documented here first.

## Network connections

**The sayit app contains no networking code.** It can't send anything anywhere. The only network access happens when **you** ask for it:

| When | What connects | To | Why |
|---|---|---|---|
| You click **Download** in Settings (or run `fetch-models.sh`) | the bundled `fetch-models.sh` script, using the system's `curl` | Hugging Face; the Moonshine files come from blob.handy.computer | To download the model files, each checked against a pinned SHA-256 hash |
| You build sayit from source | Rust's `cargo` | crates.io and the ONNX Runtime download server | To fetch the code sayit is built from |
| *Planned:* checking for updates | the bundled script, using `curl` | GitHub | **Off unless you turn it on.** It only tells you a new version exists; it never downloads or installs anything by itself |

These services see your IP address, as with any download, and their own privacy policies apply. Once the model is downloaded, sayit works fully **offline**. If you'd rather never let sayit check for updates, leave the option off and build new versions yourself from the GitHub page.

## Permissions

sayit asks macOS for only two permissions:

| Permission | Why | What it can't do |
|---|---|---|
| **Microphone** | To hear you while you hold the hotkey | The mic is open **only** while the key is held, or for 1.5 seconds when you click **Test Microphone** in Settings. macOS's orange microphone dot confirms this. Nothing is recorded to disk. |
| **Accessibility** | To notice the hotkey from any app, and to paste the text with ⌘V | sayit uses it only to watch for your hotkey and to send ⌘V. It never records, stores or sends your other keystrokes. |

These permissions belong to **sayit alone**. They give no extra access to any other app, and they don't change how the rest of your system works. You can see and revoke them at any time in **System Settings › Privacy & Security** (the Microphone and Accessibility sections), or with:

```sh
tccutil reset Accessibility io.github.ipsamurai.sayit
tccutil reset Microphone io.github.ipsamurai.sayit
```

Accessibility is a powerful permission for any app, because an app that has it *could* read keystrokes. That's why sayit's full source code is public, so you can check what it does, or build it yourself.

## Check it yourself

You don't have to take our word for any of this:

- **Network:** while sayit is running, `lsof -a -i -p $(pgrep -x sayit)` lists its network connections, and it prints nothing. You can also watch it with a firewall such as [LuLu](https://objective-see.org/products/lulu.html) or Little Snitch.
- **No networking code:** in the source folder, `cargo tree -e normal | grep -iE 'http|reqwest|hyper|tokio'` prints nothing.
- **Permissions requested:** `codesign -d --entitlements - /Applications/sayit.app` shows a single entitlement, microphone access.
- **Microphone use:** the orange dot in the menu bar appears only while you hold the hotkey (or during a Test Microphone you started).
- **Stored files:** `ls -la ~/Library/Application\ Support/sayit/` shows everything sayit keeps.
- **The code itself:** it's all on GitHub, and you can build it yourself instead of using a downloaded app.

## Questions

Open an issue on GitHub. To report a privacy or security problem privately, see [SECURITY.md](SECURITY.md).
