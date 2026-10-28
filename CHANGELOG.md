# Changelog

All notable changes to rubrique are listed here, newest first.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Until 1.0, a minor version (0.x.0) may change behavior or settings.

## [Unreleased]

### Added

- App icon: a lowercase "m" ending in a marker tip. Shown on the window,
  in the macOS app and in Linux menus.

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

[Unreleased]: https://github.com/witseko/rubrique/compare/rubrique-v0.2.0...HEAD
[0.2.0]: https://github.com/witseko/rubrique/releases/tag/rubrique-v0.2.0
