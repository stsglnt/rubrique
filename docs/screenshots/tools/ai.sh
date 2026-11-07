#!/bin/zsh
# Remake docs/screenshots/ai.png: a Compare run of Claude and Codex on the
# sample notes, the merge screen open, with the Ask bar and a hover summary
# (macOS only: uses screencapture and swift).
#
#   cargo build && docs/screenshots/tools/ai.sh target/debug/rubrique
#
# Run it from a terminal that has Accessibility and Screen Recording access
# (System Settings, Privacy & Security): it drives the window with
# synthetic clicks and keystrokes, so keep your hands off the keyboard and
# mouse for the ten seconds it takes. The app starts with its own HOME, an
# empty tmux socket folder and stand-in `claude`, `codex` and `ollama`
# programs first on the PATH, which print the prepared answers in
# tools/ai; your settings and agents are not touched. The sample folder is
# copied to /tmp/notes so the status bar shows a short path.
#
# The picture is composed from two captures, because the app does not open
# the Ask bar over the merge screen: the merge screen, with the Ask bar and
# the summary card cut from an earlier frame and pasted at the bottom.
set -eu
binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
tools=$(cd "$(dirname "$0")" && pwd)
shots=$(dirname "$tools")
work=$(mktemp -d)
notes=/tmp/notes
pid=""
cleanup() {
  [ -n "$pid" ] && { kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; }
  rm -rf "$work" "$notes"
}
trap cleanup EXIT
if [ -e "$notes" ]; then
  echo "$notes exists; move it away first" >&2
  exit 1
fi
cp -R "$shots/sample/notes" "$notes"
# The summary card says when the file was edited: two days ago.
touch -A -480000 "$notes/ideas.md"

# Stand-ins: each waits a moment (the merge screen shows the time taken)
# and prints its prepared answer, whatever the arguments.
mkdir -p "$work/bin"
stand_in() {
  printf '#!/bin/sh\nsleep %s\ncat "%s"\n' "$2" "$tools/ai/$3" > "$work/bin/$1"
  chmod +x "$work/bin/$1"
}
stand_in claude 2 claude.md
stand_in codex 3 codex.md
stand_in ollama 1 ideas-summary.txt

# Compiled once: each `swift` run would take a second.
swiftc -O "$tools/drive.swift" -o "$work/drive"
drive="$work/drive"

home="$work/home"
mkdir -p "$home/Library/Application Support/rubrique" "$work/tmux"
# Indigo theme; the notes folder is allowed, so no permission question.
printf 'theme=dark\nmarkdown_only=false\nwindow_width=1100\nwindow_height=700\nai_allowed_folder=%s\n' "$notes" \
  > "$home/Library/Application Support/rubrique/settings"
TMUX_TMPDIR="$work/tmux" PATH="$work/bin:/usr/bin:/bin:/usr/sbin:/sbin" HOME="$home" \
  "$binary" "$notes/todo.md" > "$work/app.log" 2>&1 &
pid=$!
id=""
for try in $(seq 20); do
  id=$("$drive" window rubrique todo.md | cut -d' ' -f1)
  [ -n "$id" ] && break
  sleep 0.5
done
if [ -z "$id" ]; then
  echo "no window; see $work/app.log" >&2
  exit 1
fi
sleep 2

# Positions are points from the window's top left corner, title bar
# included, for the 1100 x 700 window.
"$drive" activate "$pid"; sleep 0.5
"$drive" key cmd+k; sleep 0.8
"$drive" click "$id" 640 143; sleep 0.3   # the request field, so typing goes there
"$drive" type "make it shorter"; sleep 0.3
"$drive" click "$id" 510 243; sleep 0.5   # Compare with: open the picker
"$drive" click "$id" 505 296; sleep 0.5   # Codex
# Rest the pointer on ideas.md: the summary card shows after half a second
# and the stand-in answers a second later.
"$drive" move "$id" 70 177; sleep 0.2; "$drive" move "$id" 75 178; sleep 2.5
screencapture -x -o -l "$id" "$work/ask.png"
"$drive" click "$id" 950 243; sleep 5.5   # Run; both stand-ins are done after 3 s
"$drive" click "$id" 992 484; sleep 0.5   # Compare in merge
# Park the pointer on an empty part of the explorer, so nothing is hovered.
"$drive" move "$id" 130 450; sleep 0.2; "$drive" move "$id" 132 452; sleep 1
screencapture -x -o -l "$id" "$work/merge.png"

# The Ask bar and the card, moved to the bottom of the merge screen.
swift "$tools/paste.swift" "$shots/ai.png" "$work/merge.png" 1100 700 \
  "$work/ask.png" 380 124 602 141 380 541 \
  "$work/ask.png" 62 192 316 118 48 564
