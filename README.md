# <img src="assets/icon/rubrique-128.png" alt="" width="40" align="top"> rubrique

A fast, lightweight Markdown editor for Linux and macOS, written in Rust.
It is source-first: rubrique never rewrites your file, it only saves what you typed.

<img src="docs/screenshots/themes.png" alt="One rubrique window shown in five themes: Light, Dark, Sepia, Midnight and High contrast" width="880">

One window, five themes, all built in: Light, Dark, Sepia, Midnight and High contrast.

## Features

- File explorer with several folders at once (the folder-plus icon opens the system folder window), showing **all** files; a **Markdown only** checkbox hides the rest
- Scratch files (`Cmd/Ctrl+N`): an empty note with a random name, for pasting things for later; once saved, a local model can suggest a real name from the content
- Right-click any file or folder: copy absolute path, relative path, or name; rename files
- Plain-text editor with Markdown syntax colors, and a rendered preview (`Cmd/Ctrl+E`)
- Edit in the preview: click a paragraph, list or heading to edit just that block as Markdown
- Notes: select text in the preview and add a note; it is saved in the file as a hidden comment, highlighted in the preview and listed in a Notes panel
- **Ask AI** (`Cmd/Ctrl+K`): Claude, Codex, Grok or a local model rewrites the selection; you review every change in the merge screen before it is applied. rubrique runs the tools you have installed and holds no API keys
- **Hover summaries**: a two-sentence summary of a file when you rest the pointer on it in the explorer, from a local model by default
- **Compare tools**: run two AI tools on the same request and pick the best parts of each in the merge screen
- **Resolve notes with AI**: one note or all of them, reviewed in the merge screen
- **Skills**: reusable AI instructions as `SKILL.md` folders (the same format as Claude Code skills), picked from the Ask bar; the skills folder is set in Settings
- Code blocks in the preview have **Copy**, and **Prettify** for ` ```json `, ` ```sql ` and ` ```xml ` blocks
- **Format** (`Cmd/Ctrl+Shift+F`): pretty-print JSON, SQL (PostgreSQL style, like pgFormatter) or XML, for the whole file or the selection
- **Open in browser** (`Cmd/Ctrl+Shift+B`): Markdown as a full web page in your theme's colors, or HTML files as they are
- Atomic saves and an unsaved-changes prompt
- Notices when another program changes the open file: reloads it if you have no edits, otherwise offers a side-by-side **merge** (on disk | result | mine), reload or keep mine, without interrupting your typing
- Line shortcuts: move (`Alt+Up/Down`), duplicate (`Cmd/Ctrl+D`), insert date (`Cmd/Ctrl+Shift+D`)
- Five built-in themes (Light, Dark, Sepia, Midnight, High contrast), or follow the system; add your own as small text files
- Two dependencies (`eframe` for the window, `rfd` for the folder window); the Markdown parser and everything else is our own code

## Install

Download the latest version from [Releases](https://github.com/witseko/rubrique/releases):

- **macOS** (Apple Silicon and Intel): open the `.dmg` and drag rubrique to
  Applications. The app is not notarized yet, so the first time right-click
  rubrique and choose **Open**.
- **Debian / Ubuntu**: `sudo apt install ./rubrique_X.Y.Z-1_amd64.deb`
- **Other Linux**: unpack the `.tar.gz` and run `./install.sh` (installs to
  `~/.local`; `sudo ./install.sh /usr/local` for all users).

Every push to `main` also builds these packages; find them under the run's
artifacts in [Actions](https://github.com/witseko/rubrique/actions/workflows/release.yml).

## Build and run

Needs Rust 1.95 or newer.

```sh
cargo run --release -- ~/notes             # open a folder
cargo run --release -- ~/notes/todo.md     # open a file and its folder
cargo run --release -- ~/notes ~/work/docs # open several folders at once
```

Linux also needs the usual X11/Wayland and OpenGL development libraries.

## Shortcuts

`Cmd` on macOS, `Ctrl` on Linux.

| Shortcut | Action |
| --- | --- |
| Cmd+S | Save |
| Cmd+E | Toggle edit / preview |
| Cmd+, | Open or close Settings |
| Cmd+K | Ask AI about the selection |
| Cmd+N | New scratch file |
| Cmd+Shift+B | Open in browser (Markdown and HTML) |
| Cmd+Shift+F | Format JSON, SQL or XML |
| Cmd+Shift+M | Add a note about the selected text |
| Cmd+Shift+C | Copy path of the open file |
| Alt+Up / Alt+Down | Move line(s) |
| Cmd+D | Duplicate line(s) |
| Cmd+Shift+D | Insert today's date |

## Themes

Open **Settings** with the gear at the bottom left (or `Cmd/Ctrl+,`) and click a
theme card: **System** (follows your OS), **Light**, **Dark**, **Sepia**,
**Midnight**, **High contrast**, or one of your own.

To add a theme, put a `.theme` file in `~/.config/rubrique/themes/`
(macOS: `~/Library/Application Support/rubrique/themes/`) and press
**Reload themes** in Settings.
Set only the colors you want to change; the rest comes from `base`:

```
name=Ocean
base=dark
accent=#00aaff
```

For a full file to start from, see the built-in themes in [`themes/`](themes/).
All keys are listed in [`specs/07-themes.md`](specs/07-themes.md).

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo deny check   # known security problems, licenses, sources
```

The Security workflow runs `cargo deny` on dependency changes and every
Monday; Dependabot opens weekly pull requests for outdated crates and actions.

Specs live in [`specs/`](specs/): start with `00-overview.md`.

## Versions

rubrique follows [Semantic Versioning](https://semver.org); see
[`CHANGELOG.md`](CHANGELOG.md) for what changed in each version. To release:

1. Move the notes under **Unreleased** in `CHANGELOG.md` to a new version heading.
2. Set the same `version` in `Cargo.toml` and push to `main`.

The Release workflow sees a version with no `rubrique-vX.Y.Z` tag, builds the macOS
and Linux packages, then creates the tag and a GitHub release with the
changelog notes. Pushing a `rubrique-vX.Y.Z` tag yourself works too.
