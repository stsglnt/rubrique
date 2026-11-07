#!/bin/zsh
# Remake docs/screenshots/themes.png: the sample notes folder open in the
# window, captured once per theme (macOS only: uses screencapture and
# swift) and joined into one image, a strip per theme.
#
#   cargo build && docs/screenshots/tools/themes.sh target/debug/rubrique
#
# Each run starts the app with its own HOME, so your settings stay as they
# are, and with an empty tmux socket folder, so no terminal of yours is
# listed. The window shows on your screen for about two seconds per theme.
set -eu
binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
tools=$(cd "$(dirname "$0")" && pwd)
shots=$(dirname "$tools")
work=$(mktemp -d)
themes=(light dark sepia midnight high-contrast)
labels=("Light" "Indigo" "Sepia" "Midnight" "High contrast")
for theme in "${themes[@]}"; do
  home="$work/home-$theme"
  mkdir -p "$home/Library/Application Support/rubrique"
  printf 'theme=%s\nmarkdown_only=false\nwindow_width=1100\nwindow_height=700\n' "$theme" \
    > "$home/Library/Application Support/rubrique/settings"
  # An empty tmux socket folder: the app sees no terminal sessions.
  mkdir -p "$work/tmux"
  TMUX_TMPDIR="$work/tmux" PATH=/usr/bin:/bin:/usr/sbin:/sbin HOME="$home" "$binary" "$shots/sample/notes/todo.md" > "$work/$theme.log" 2>&1 &
  pid=$!
  id=""
  for try in $(seq 12); do
    id=$(swift "$tools/list-windows.swift" 2>/dev/null | awk -F'\t' '$2=="rubrique" && index($3,"todo.md")>0 {print $1; exit}')
    [ -n "$id" ] && break
  done
  # One more listing (about a second) so the first frames are drawn.
  swift "$tools/list-windows.swift" > /dev/null 2>&1
  if [ -z "$id" ]; then
    echo "$theme: no window; see $work/$theme.log" >&2
    kill "$pid" 2>/dev/null || true
    exit 1
  fi
  screencapture -x -o -l "$id" "$work/$theme.png"
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  echo "$theme: captured"
done
parts=()
for index in {1..5}; do
  parts+=("${labels[$index]}=$work/${themes[$index]}.png")
done
swift "$tools/compose.swift" "$shots/themes.png" 2200 1400 "${parts[@]}"
rm -rf "$work"
