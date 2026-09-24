#!/usr/bin/env bash
# Builds target/release/sayit-<version>.dmg: sayit.app plus an Applications
# shortcut for drag-to-install, and sayit-<version>.dmg.sha256 next to it so a
# download can be checked with `shasum -a 256 -c`. Needs no admin rights;
# writes only inside target/.
set -euo pipefail
cd "$(dirname "$0")/.."

./scripts/bundle-macos.sh

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
DMG="target/release/sayit-$VERSION.dmg"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp -R target/release/sayit.app "$STAGE/"
ln -s /Applications "$STAGE/Applications"

hdiutil create -quiet -volname "sayit" -srcfolder "$STAGE" -format UDZO -ov "$DMG"
hdiutil verify -quiet "$DMG"
# Relative file name inside the checksum file, so `shasum -c` works wherever
# both files are downloaded to.
(cd target/release && shasum -a 256 "sayit-$VERSION.dmg" > "sayit-$VERSION.dmg.sha256")
echo "Built $DMG ($(du -h "$DMG" | cut -f1))"
cat "$DMG.sha256"
