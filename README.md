# rubrique

A fast, lightweight Markdown editor for Linux and macOS, written in Rust.
It is source-first: rubrique never rewrites your file, it only saves what you typed.

<img src="docs/screenshots/themes.png" alt="One rubrique window shown in five themes: Light, Dark, Sepia, Midnight and High contrast" width="880">

One window, five themes, all built in: Light, Dark, Sepia, Midnight and High contrast.

## Features (v0.1)

- File explorer that shows **all** files; a **Markdown only** checkbox hides the rest
- Right-click any file or folder: copy absolute path, relative path, or name
- Plain-text editor with Markdown syntax colors, and a rendered preview (`Cmd/Ctrl+E`)
- Atomic saves, unsaved-changes prompt, reload when the file changes on disk
- Line shortcuts: move (`Alt+Up/Down`), duplicate (`Cmd/Ctrl+D`), insert date (`Cmd/Ctrl+Shift+D`)
- Five built-in themes (Light, Dark, Sepia, Midnight, High contrast), or follow the system; add your own as small text files
- One dependency (`eframe`); the Markdown parser and everything else is our own code

## Build and run

Needs Rust 1.95 or newer.

```sh
cargo run --release -- ~/notes          # open a folder
cargo run --release -- ~/notes/todo.md  # open a file and its folder
```

Linux also needs the usual X11/Wayland and OpenGL development libraries.

## Shortcuts

`Cmd` on macOS, `Ctrl` on Linux.

| Shortcut | Action |
| --- | --- |
| Cmd+S | Save |
| Cmd+E | Toggle edit / preview |
| Cmd+, | Open or close Settings |
| Cmd+Shift+C | Copy path of the open file |
| Alt+Up / Alt+Down | Move line(s) |
| Cmd+D | Duplicate line(s) |
| Cmd+Shift+D | Insert today's date |

## Themes

Open **Settings** with the gear at the top right (or `Cmd/Ctrl+,`) and click a
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
```

Specs live in [`specs/`](specs/): start with `00-overview.md`.
