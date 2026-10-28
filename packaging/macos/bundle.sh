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
