# Changelog

All notable changes to rubrique are listed here, newest first.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Until 1.0, a minor version (0.x.0) may change behavior or settings.

## [Unreleased]

### Added

- Settings → AI: pick the tool for requests (Claude, Codex, Grok or a local
  model) and for summaries, and edit each tool's command. rubrique shows which
  tools are installed.
- **Ask** (`Cmd/Ctrl+K`, or the status bar button): ask an AI tool to change
  the selection (or the whole file). Quick actions: Shorter, Fix grammar,
  Table, Translate, Explain. The output streams into a panel with **Stop**;
  **Review in merge** shows each change next to the original, and nothing
  changes until you press **Apply**. The first run in a folder shows the
  exact command and asks first.

## [0.3.0] - 2026-10-28

### Added

- App icon: a lowercase "m" ending in a marker tip. Shown on the window,
  in the macOS app and in Linux menus.
- Scratch files: the page-plus icon or `Cmd/Ctrl+N` creates an empty note with a
  random name like `2026-10-28-brave-otter.md` in a scratches folder
  (set in Settings → Files).
- **Rename…** in a file's right-click menu.
- The explorer's top row uses icons: add folder, new scratch and refresh
  (hover for a tip), like the settings gear.
- **Open in browser** (`Cmd/Ctrl+Shift+B`, or the status bar button): shows
  the Markdown as a full web page in the current theme's colors, or an HTML
  file as it is, in the default browser. Unsaved changes are included.
- Live check for changes by other programs (every 2 seconds). A file without
  unsaved edits reloads with a status message. With unsaved edits a bar
  offers **Compare and merge…**, **Reload theirs** or **Keep mine**, and you
  can keep typing meanwhile.
- Merge screen: the file on disk, the result and your version side by side
  (**Swap sides** flips them). One-sided changes merge by themselves; for
  conflicts pick left, right or both, or edit the result, then **Apply**.
- **Format** (`Cmd/Ctrl+Shift+F`, or the status bar button in `.json`,
  `.sql` and `.xml` files): pretty-prints JSON, XML, and SQL in
  PostgreSQL / pgFormatter style. Works on the selection too, guessing its
  language, so code blocks in notes can be formatted. Undo with `Cmd/Ctrl+Z`.
- Edit in the preview: click a block (paragraph, heading, list, quote, code)
  to edit just that block's Markdown in place; Esc or a click outside shows it
  rendered again.
- Notes: drag to select text in the preview, then **+ Add note** (or
  `Cmd/Ctrl+Shift+M`, also in Edit mode). The note is stored in the file as a
  hidden comment after the text, highlighted in the preview with the note on
  hover, and listed in a **Notes** panel with Edit and Delete.
- The preview hides HTML comments; the editor dims them.
- Dependency security checks: cargo-deny in CI (known security problems,
  licenses, crate sources), weekly and on every dependency change, and
  Dependabot updates for crates and GitHub Actions.

### Changed

- The settings gear is redrawn to match the app icon, like the new explorer
  icons: heavy rounded lines with one detail in the theme's accent color.

### Fixed

- The unsaved-changes dot showed as an empty box.
- A long file path in the status bar ran into the word count; it is now
  shortened with "…".
- Long names in the explorer wrapped onto two lines with the folder arrow
  drawn over them; rows now stay on one line and the explorer scrolls sideways.

## [0.2.0] - 2026-10-28

### Added

- Themes: Light, Dark, Sepia, Midnight and High contrast are built in, and
  **System** follows the operating system. Add your own as small `.theme`
  files in the config folder.
- Settings panel (gear at the bottom of the explorer, or `Cmd/Ctrl+,`) with
  theme cards and **Reload themes**.
- Markdown syntax colors in the editor.
- Several folders in the explorer at once: **+ Add folder…** opens the system
  folder window; right-click a folder to remove it. Several folders can also
  be passed on the command line, and the list is remembered.
- Click **Edit / Preview** in the status bar to switch modes.

### Changed

- Cleaner preview: larger text with more line spacing, lines under the top
  headings, drawn bullets and checkboxes, a colored quote bar, outlined code
  blocks.
- Borderless editor and preview page with margins.
- The Folder bar at the top is gone; use **+ Add folder…** instead.
- Settings store explorer folders as `folder=` lines. An older
  `last_folder=` line is still read.

### Dependencies

- Added `rfd` for the native folder window.

## 0.1.0 - 2026-10-27

### Added

- First version: file explorer with a Markdown-only filter and copy-path menu,
  plain-text editor with a rendered Markdown preview, atomic saves, unsaved
  changes prompt, reload on outside changes, and line shortcuts.

[Unreleased]: https://github.com/witseko/rubrique/compare/rubrique-v0.3.0...HEAD
[0.3.0]: https://github.com/witseko/rubrique/compare/rubrique-v0.2.0...rubrique-v0.3.0
[0.2.0]: https://github.com/witseko/rubrique/releases/tag/rubrique-v0.2.0
