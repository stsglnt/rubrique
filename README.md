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

### Theme examples

Copy one into a file such as `~/.config/rubrique/themes/sepia.theme`. Keys left
out come from the `base` theme.

**Sepia**: warm paper for long reading.

```
name=Sepia
base=light
background=#f6efe1
sidebar=#ece3d0
surface=#e3d8c1
hover=#d9ccb2
border=#d8cbb3
text=#433422
text_strong=#2a1f12
text_weak=#6e5c45
accent=#9a4f1c
selection=#e3cfa8
code_background=#efe6d4
```

**Midnight**: deep blue for night work.

```
name=Midnight
base=dark
background=#0f1626
sidebar=#0b111d
surface=#1b2438
hover=#253049
border=#1d2740
text=#cdd6e6
text_strong=#f2f5fa
text_weak=#8391a8
accent=#7fb4ff
selection=#23395f
code_background=#151e31
```

**High contrast**: black and white with a yellow accent.

```
name=High contrast
base=dark
background=#000000
sidebar=#000000
surface=#1a1a1a
hover=#333333
border=#ffffff
text=#ffffff
text_strong=#ffffff
text_weak=#c8c8c8
accent=#ffd84d
selection=#1f4fbf
code_background=#141414
```

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Specs live in [`specs/`](specs/): start with `00-overview.md`.
