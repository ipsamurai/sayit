# Documentation checklist: sayit

_Last reviewed 2026-09-25, before the 0.2.0 release. Review it again if sayit's license, distribution or data handling changes (for example, adding an update check, a Windows build or a paid version)._

**Project profile:** open source (MIT OR Apache-2.0) · free · desktop menu-bar app (macOS; Linux untested) · on GitHub, with DMGs in GitHub Releases

Legend: ✅ done · 🟡 drafted, needs review · ⬜ not started · ➖ doesn't apply · ⏸ later (reason given)

## Core

- ✅ README.md
- ✅ License: LICENSE-MIT, LICENSE-APACHE, NOTICE
- ✅ This checklist

## Legal and compliance

- ✅ Privacy: PRIVACY.md. sayit collects no data: no accounts, no network code, audio in memory only.
- ➖ Terms of service: no service, no accounts, no paid tiers.
- ➖ EULA: the open-source licenses cover use of the app.
- ✅ Disclaimer: DISCLAIMER.md
- ✅ AI policy: AI_POLICY.md. It discloses AI-assisted development, sets expectations for AI-assisted pull requests, and explains that the app's speech model is local. It adds no scraping or use restrictions, which would conflict with MIT and Apache-2.0.
- ✅ Third-party attribution: `THIRD-PARTY-NOTICES.txt` is generated into the app by `scripts/third-party-notices.py` (Rust crates and ONNX Runtime). Speech models aren't bundled (their licenses are in the README). The app icon is original, drawn by `scripts/make-icon.sh`. The menu-bar icons are Apple's SF Symbols, loaded from macOS at runtime and not bundled.

## Open-source hygiene

- ✅ CONTRIBUTING.md
- ✅ CODE_OF_CONDUCT.md
- ✅ SECURITY.md (private reporting through GitHub)
- ✅ CHANGELOG.md
- ✅ RELEASING.md
- ✅ AGENTS.md (with CLAUDE.md importing it)
- ✅ Issue and pull request templates
- ✅ Branch and tag rulesets on GitHub (`main`: pull requests, no force-push or deletion; `v*` tags protected)
- ⬜ Repository description, website and topics on GitHub (all empty). Suggested description: "Private, local-only push-to-talk dictation for macOS. Hold a key, speak, release." Suggested topics: `dictation`, `speech-to-text`, `offline`, `privacy`, `macos`, `rust`, `menu-bar`.
- ⬜ Turn on **Private vulnerability reporting** (SECURITY.md relies on it), plus **Secret scanning with push protection** and **Dependabot alerts**, under Settings › Security.
- ⏸ Continuous integration (a GitHub Actions workflow running clippy and tests on pull requests). Worth adding before outside pull requests arrive, such as the Windows port. After that, the `main` ruleset can require it to pass.
- ➖ `.github/FUNDING.yml`: no sponsorship planned.

## Promotional and listing content

- 🟡 PROMO.md: tagline, short and long description, features, screenshot list. Needs review.
- ⏸ Screenshots: needed for the showcase page (the list is in PROMO.md).
- ✅ App icon at the required sizes (`assets/AppIcon.icns`, 16–1024 px)

## Distribution: GitHub Releases

- ✅ DMG with a SHA-256 checksum file (`scripts/package-dmg.sh`)
- ✅ Third-party notices inside the app (Settings › About › Licenses)
- ✅ Install notes: checksum check, and **Open Anyway** for the first launch (README and release notes)
- ⬜ Publish the v0.2.0 GitHub Release (see RELEASING.md step 7)

## Showcase page on the maintainer's site

- ⏸ Page: short description, screenshots and a "View on GitHub" link. The copy is ready in PROMO.md; to be built later. The repo's LICENSE covers usage, so the page needs no separate legal text.
- ⏸ Make sure the page's copyright line matches NOTICE ("ipsamurai and the sayit contributors").

## Notes

- Public docs use the `ipsamurai` handle and GitHub for contact only, with no real name, email or hardware details.
- If sayit ever gains network features, such as the planned opt-in update check, update PRIVACY.md, SECURITY.md and README's "no network code" claims first.
