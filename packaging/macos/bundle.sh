#!/bin/sh
# Build rubrique.app from the two release binaries (Apple Silicon and Intel)
# and pack it as dist/rubrique-VERSION-macos-universal.dmg and .zip.
# Usage: packaging/macos/bundle.sh VERSION
#
# Signing (see packaging/macos/README.md):
# - MACOS_SIGN_IDENTITY set, e.g. "Developer ID Application: Name (TEAMID)":
#   signs with that identity and the hardened runtime.
# - APPLE_ID, APPLE_TEAM_ID and APPLE_APP_PASSWORD also set: notarizes the
#   .dmg with Apple and staples the ticket to the .dmg and the app, so
#   Gatekeeper opens it with no warning.
# - Nothing set: ad-hoc signature only. macOS blocks the downloaded app.
set -eu
version="$1"
name="rubrique-$version-macos-universal"
app="dist/rubrique.app"
dmg="dist/$name.dmg"
identity="${MACOS_SIGN_IDENTITY:-}"

rm -rf "$app" "$dmg" "dist/$name.zip"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create \
    target/aarch64-apple-darwin/release/rubrique \
    target/x86_64-apple-darwin/release/rubrique \
    -output "$app/Contents/MacOS/rubrique"
sed "s/__VERSION__/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
# App icon: an .iconset from the PNG sizes, turned into .icns by iconutil.
iconset="dist/rubrique.iconset"
rm -rf "$iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    cp "assets/icon/rubrique-$size.png" "$iconset/icon_${size}x${size}.png"
    cp "assets/icon/rubrique-$((size * 2)).png" "$iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/rubrique.icns"
rm -rf "$iconset"

if [ -n "$identity" ]; then
    # Hardened runtime and a secure timestamp: both needed for notarization.
    # The entitlements let the hardened runtime use the microphone.
    codesign --force --options runtime --timestamp \
        --entitlements packaging/macos/entitlements.plist --sign "$identity" "$app"
else
    # Ad-hoc signature: keeps Apple Silicon happy, but Gatekeeper rejects it.
    codesign --force --sign - "$app"
fi
codesign --verify --strict --verbose=2 "$app"

dmg_root="dist/dmg"
rm -rf "$dmg_root"
mkdir -p "$dmg_root"
cp -R "$app" "$dmg_root/"
ln -s /Applications "$dmg_root/Applications"
hdiutil create -volname "rubrique $version" -srcfolder "$dmg_root" -ov -format UDZO "$dmg"
rm -rf "$dmg_root"

if [ -n "$identity" ]; then
    codesign --force --timestamp --sign "$identity" "$dmg"
    if [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
        # Notarizing the .dmg covers the app inside it too, so both get the ticket.
        xcrun notarytool submit "$dmg" \
            --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" \
            --wait --timeout 30m
        xcrun stapler staple "$dmg"
        xcrun stapler staple "$app"
        xcrun stapler validate "$dmg"
        # The same checks Gatekeeper runs on a downloaded app.
        spctl --assess --type execute --verbose=2 "$app"
        spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
    fi
fi

# The .zip is made last, so it holds the stapled app.
ditto -c -k --keepParent "$app" "dist/$name.zip"
echo "$dmg dist/$name.zip"
