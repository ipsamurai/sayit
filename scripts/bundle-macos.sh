#!/usr/bin/env bash
# Builds target/release/sayit.app: the release binary, Info.plist and icon,
# ad-hoc signed. Needs no admin rights and writes only inside target/.
# Models are not bundled; they stay in ~/Library/Application Support/sayit
# (see fetch-models.sh).
set -euo pipefail

cd "$(dirname "$0")/.."
[[ "$(uname -s)" == Darwin ]] || { echo "macOS only" >&2; exit 1; }

BUNDLE_ID="io.github.ipsamurai.sayit"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="target/release/sayit.app"

cargo build --release

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/sayit "$APP/Contents/MacOS/sayit"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>         <string>$BUNDLE_ID</string>
  <key>CFBundleName</key>               <string>sayit</string>
  <key>CFBundleDisplayName</key>        <string>sayit</string>
  <key>CFBundleExecutable</key>         <string>sayit</string>
  <key>CFBundleIconFile</key>           <string>AppIcon</string>
  <key>CFBundlePackageType</key>        <string>APPL</string>
  <key>CFBundleShortVersionString</key> <string>$VERSION</string>
  <key>CFBundleVersion</key>            <string>$VERSION</string>
  <key>CFBundleInfoDictionaryVersion</key> <string>6.0</string>
  <key>LSMinimumSystemVersion</key>     <string>13.0</string>
  <!-- Menu-bar only: no Dock icon or app switcher entry. -->
  <key>LSUIElement</key>                <true/>
  <key>NSHighResolutionCapable</key>    <true/>
  <!-- Shown in the microphone permission prompt. Without it macOS kills the
       app on first mic access. -->
  <key>NSMicrophoneUsageDescription</key>
  <string>sayit listens only while you hold the dictation key. Audio is transcribed on this Mac and never saved or sent anywhere.</string>
</dict>
</plist>
EOF
plutil -lint -s "$APP/Contents/Info.plist"

# Ad-hoc signature (no certificate needed) with the Hardened Runtime, which
# stops other processes from injecting code (e.g. DYLD_INSERT_LIBRARIES) to
# borrow sayit's microphone and Accessibility permissions. The only entitlement
# is microphone access. macOS ties permissions to this exact build, so after
# rebuilding you may need to re-enable sayit in Privacy & Security > Accessibility.
codesign --force --sign - --options runtime --entitlements assets/sayit.entitlements \
  --identifier "$BUNDLE_ID" "$APP"
codesign --verify --strict "$APP"

echo "Built $APP ($VERSION). Open it with: open $APP"
