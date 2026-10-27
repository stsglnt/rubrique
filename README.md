# rubrique

A fast, lightweight Markdown editor for Linux and macOS, written in Rust.
It is source-first: rubrique never rewrites your file, it only saves what you typed.

## Features (v0.1)

- File explorer that shows **all** files; a **Markdown only** checkbox hides the rest
- Right-click any file or folder: copy absolute path, relative path, or name
- Plain-text editor with a rendered Markdown preview (`Cmd/Ctrl+E`)
- Atomic saves, unsaved-changes prompt, reload when the file changes on disk
- Line shortcuts: move (`Alt+Up/Down`), duplicate (`Cmd/Ctrl+D`), insert date (`Cmd/Ctrl+Shift+D`)
- Light and Dark themes built in, or follow the system; add your own as small text files
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
| Cmd+Shift+C | Copy path of the open file |
| Alt+Up / Alt+Down | Move line(s) |
| Cmd+D | Duplicate line(s) |
| Cmd+Shift+D | Insert today's date |

## Themes

Pick a theme from the drop-down at the top right: **System** (follows your OS),
**Light**, **Dark**, or one of your own.

To make a theme, put a `.theme` file in `~/.config/rubrique/themes/`
(macOS: `~/Library/Application Support/rubrique/themes/`) and restart rubrique.
Start from [`themes/light.theme`](themes/light.theme) or
[`themes/dark.theme`](themes/dark.theme), or set only what you want to change:

```
name=Ocean
base=dark
accent=#00aaff
```

All keys are listed in [`specs/07-themes.md`](specs/07-themes.md).

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Specs live in [`specs/`](specs/): start with `00-overview.md`.
