#!/bin/sh
# Build rubrique.app from the two release binaries (Apple Silicon and Intel)
# and pack it as dist/rubrique-VERSION-macos-universal.dmg and .zip.
# Usage: packaging/macos/bundle.sh VERSION
set -eu
version="$1"
name="rubrique-$version-macos-universal"
app="dist/rubrique.app"

rm -rf "$app" "dist/$name.dmg" "dist/$name.zip"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create \
    target/aarch64-apple-darwin/release/rubrique \
    target/x86_64-apple-darwin/release/rubrique \
    -output "$app/Contents/MacOS/rubrique"
sed "s/__VERSION__/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
if [ -f assets/icon/rubrique.icns ]; then
    cp assets/icon/rubrique.icns "$app/Contents/Resources/rubrique.icns"
fi
# Ad-hoc signature: not notarized, but keeps Apple Silicon happy.
codesign --force --deep --sign - "$app"

ditto -c -k --keepParent "$app" "dist/$name.zip"

dmg_root="dist/dmg"
rm -rf "$dmg_root"
mkdir -p "$dmg_root"
cp -R "$app" "$dmg_root/"
ln -s /Applications "$dmg_root/Applications"
hdiutil create -volname "rubrique $version" -srcfolder "$dmg_root" -ov -format UDZO "dist/$name.dmg"
rm -rf "$dmg_root"
echo "dist/$name.dmg dist/$name.zip"
