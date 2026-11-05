# <img src="assets/icon/rubrique-128.png" alt="" width="40" align="top"> rubrique

A fast editor, agentic on demand.

Write Markdown (or plain text, JSON, SQL, XML) in a small native editor.
Press `Cmd/Ctrl+K` to hand the text to an agent, then accept or reject the
change. Uses the agents already on your computer: Claude Code, Codex, Grok or
a local model. No API keys. macOS and Linux.

<img src="docs/screenshots/ai.png" alt="rubrique comparing Claude and Codex answers in the merge screen, with the Ask bar and a hover summary from a local model" width="880">

## Install

Download from [Releases](https://github.com/witseko/rubrique/releases).

| System | How |
| --- | --- |
| macOS | Open the `.dmg`, drag rubrique to Applications |
| Debian, Ubuntu | `sudo apt install ./rubrique_X.Y.Z-1_amd64.deb` |
| Other Linux | Unpack the `.tar.gz`, run `./install.sh` |

macOS blocks the first open (the app is not notarized yet). Fix:
`xattr -dr com.apple.quarantine /Applications/rubrique.app`

## Features

**Agents**

- **Ask** (`Cmd+K`): a request, or Shorter, Fix grammar, Table, Translate, Explain.
- **Review**: every change opens in a merge screen. Nothing applies until you accept.
- **Compare**: one request to two agents, keep the best of each.
- **Named agents and skills**: type `@ai-` to pick one. Claude Code agents and skills work too.
- **Notes**: leave a note on a passage, let an agent resolve it.

Agents run only when you ask, read-only by default.

**Writing**

- Live preview: click a block to edit it.
- Voice memos (`Cmd+Shift+R`), transcribed locally with whisper.cpp.
- Drop images and audio onto the page.
- Scratch files (`Cmd+N`), optionally encrypted, and an optional app PIN.
- Format JSON, SQL, XML (`Cmd+Shift+F`). Render and lint Helm charts.

**Files**

- Several folders in one explorer. Double-click to rename, right-click to delete.
- Tabs keep unsaved changes, cursor and undo.
- Safe saves: your file is never reformatted; outside changes open a merge.
- Find in a file or across all folders (`Cmd+F`).
- Remote folders over SSH (off by default, Settings → Files).

<img src="docs/screenshots/themes.png" alt="One rubrique window shown in five themes: Light, Dark, Sepia, Midnight and High contrast" width="880">

Five themes, or follow the system, or [add your own](themes/). Text size, line
numbers and other options are in Settings → Accessibility.

## Shortcuts

`Cmd` on macOS, `Ctrl` on Linux.

| Shortcut | Action |
| --- | --- |
| Cmd+K | Ask an agent |
| Cmd+F | Find |
| Cmd+E | Preview / source |
| Cmd+N | New scratch file |
| Cmd+Shift+M | Add a note |

<details>
<summary>All shortcuts</summary>

| Shortcut | Action |
| --- | --- |
| Cmd+S | Save |
| Cmd+W | Close tab |
| Ctrl+Tab / Ctrl+Shift+Tab | Next / previous tab |
| Cmd+, | Settings |
| Cmd++ / Cmd+- / Cmd+0 | Larger / smaller / default text size |
| Cmd+Shift+R | Record a voice memo, or stop |
| Cmd+Shift+B | Open in browser |
| Cmd+Shift+F | Format JSON, SQL or XML |
| Cmd+Shift+C | Copy the open file's path |
| Cmd+G / Cmd+Shift+G | Next / previous match |
| Alt+Up / Alt+Down | Move lines |
| Cmd+D | Duplicate lines |
| Cmd+Shift+D | Insert today's date |
| Cmd+L | Lock the window (with a PIN set) |

</details>

## Set up the agents

Install the ones you want. rubrique finds them on your `PATH` and offers the
install command when one is missing (Settings → AI).

| Program | Used for |
| --- | --- |
| `claude`, `codex`, `grok` | Ask, Compare, Notes |
| `ollama` | Local summaries, file names |
| `whisper-cli` | Transcribing memos |
| `helm` | Helm charts |

whisper.cpp also needs its model:

```sh
curl -L --create-dirs -o ~/.local/share/whisper/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
```

## Build

Rust 1.95+. On Linux, also the X11, Wayland, OpenGL and ALSA dev libraries.

```sh
cargo run --release -- ~/notes
```

Before a pull request: `cargo test`, `cargo clippy --all-targets -- -D warnings`,
`cargo fmt --check`, `cargo deny check`.

Releases: bump `CHANGELOG.md` and `Cargo.toml`, push to `main`. See
[`CHANGELOG.md`](CHANGELOG.md).
