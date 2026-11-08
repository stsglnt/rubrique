#!/bin/sh
# Install Rubrique from this folder. Default: for the current user, in ~/.local.
# For everyone: sudo ./install.sh /usr/local
set -eu
prefix="${1:-$HOME/.local}"
here="$(cd "$(dirname "$0")" && pwd)"

install -Dm755 "$here/bin/rubrique" "$prefix/bin/rubrique"
install -Dm644 "$here/share/applications/rubrique.desktop" "$prefix/share/applications/rubrique.desktop"
for icon in "$here"/share/icons/hicolor/*/apps/rubrique.png; do
    [ -f "$icon" ] || continue
    size_dir="$(basename "$(dirname "$(dirname "$icon")")")"
    install -Dm644 "$icon" "$prefix/share/icons/hicolor/$size_dir/apps/rubrique.png"
done

echo "Installed Rubrique to $prefix/bin/rubrique"
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) echo "Note: $prefix/bin is not on your PATH." ;;
esac
