# Changelog

All notable changes to rubrique are listed here, newest first.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Until 1.0, a minor version (0.x.0) may change behavior or settings.

## [Unreleased]

### Fixed

- Scrolling a page to its end with a trackpad no longer overshoots and
  springs back (the "jump" at the bottom of the preview).
- Hovering a tab shifted the tabs to its right by a few pixels while the
  close cross was shown; the tabs after the active one sat too close too.

### Added

- **Unsaved files are green in the explorer**, with a dot after the name,
  for every tab with unsaved changes, not only the open one.
- **Double-click a file in the explorer to rename it.** A single click
  still opens it.
- **Files with notes get a speech bubble** before their name in the
  explorer. Turn it on in Settings → Files → **Mark files that have notes**.
  Files are checked when shown and read again only after they change.
- **Click the empty space under the last block** in the preview to type a
  new block there. It goes to the end of the file after a blank line; an
  empty box adds nothing.
- **Tables** (`| a | b |` with a `| --- | --- |` row) show as a grid in the
  preview, in AI answers and in **Open in browser**, with bold headers,
  striped rows and the column alignment from the colons. A table can follow
  a paragraph line directly, as on GitHub.
- **Find** (`Cmd/Ctrl+F`): a bar at the top right of the page. Matches in
  the open file are tinted, the current one in the accent color; Enter,
  Shift+Enter, `Cmd/Ctrl+G` and `Cmd/Ctrl+Shift+G` step through them, and
  Esc closes the bar with the cursor on the match. **All files** searches
  every text file under the explorer's folders (hidden entries and files
  over 1 MB skipped) by name and by content, lists the first 300 hits with
  their line, and opens the file at the match on a click. Find works on the
  source, so a preview switches to it.
- **Line numbers** in the source editor: Settings → Accessibility → **Line
  numbers**. A wrapped line is numbered once.

- **Agents**: named helpers with their own personality, skills, AI tool and
  model. Each one is a folder with an `AGENT.md` file in the agents folder.
  Claude Code's agents in `~/.claude/agents` work too.
- In Ask (`Cmd/Ctrl+K`), type `@` to pick an agent, or use **Agents…**.
  Mentions start with `ai-` (`@ai-editor`), so they never clash with people's
  names. The agent's personality and skills go with the request, and its
  model is passed to the tool (`--model`, or the model in `ollama run`).
  **What Claude gets** shows the agent and its skills, and the command
  with the model.
- **Settings → Agents**: the agents folder, **Also use Claude's agents**, the
  list of agents with **Open**, and **Create example agent**.

### Changed

- rubrique is now described as an **agentic Markdown editor**: a Markdown
  editor built around the agents you already run (Claude Code, Codex, Grok
  or a local model), with every change reviewed before it is applied. The
  README, the package descriptions and the repository description say so,
  and [`docs/positioning.md`](docs/positioning.md) keeps the wording in one
  place. Nothing in the app changed.

## [0.4.4] - 2026-10-31

### Changed

- The **Ask bar shows what the tool gets** before it runs: open **What
  Claude gets** for the request, the text, rubrique's instructions, the
  skill, the command and the folder. The first run in a folder asks there
  instead of in a separate dialog.

### Fixed

- Selecting text across several list items, task items or quote lines in
  the preview and then adding a note or asking AI said "Cannot find the
  selection in the file".
- The preview's **+ Add note** button showed behind the open Ask bar.

## [0.4.3] - 2026-10-31

### Changed

- Every Markdown file opens in **preview**. A **Preview | Source** switch at
  the right end of the tab bar (or `Cmd/Ctrl+E`) shows the source, for that
  tab only. New scratch files open on their source, ready to type. The old
  status bar Edit / Preview button is gone.
- **Quitting keeps unsaved edits** as drafts and restores them on the next
  start, without asking. The files themselves are not touched until you
  save. If a file changed on disk meanwhile, the merge screen opens.

## [0.4.2] - 2026-10-30

rubrique is now described as what it has become: an AI-assisted Markdown
editor. This version adds voice memos, transcribed on your computer.

### Added

- **Record** (`Cmd/Ctrl+Shift+R` or the status bar button): records into a
  WAV file in a `memos` folder next to the note and links it at the cursor.
  The recording bar shows the time, a level meter, **Stop** and **Cancel**.
- **Drop audio** (WAV, M4A, MP3, OGG, Opus, FLAC, WebM, AAC) on the page to
  add it as a memo. **Audio…** in the status bar does the same. Drops do not
  work on Wayland yet.
- **Play** memos in the preview, with play, pause and a seek bar. WAV plays
  inside rubrique; other formats open in your audio player.
- **Transcribe** a memo: the text goes under it, through the merge screen.
  The default tool is whisper.cpp (`whisper-cli`); any tool that prints text
  works.
- **Settings → Voice memos**: memos folder, copy or link dropped files, the
  transcription command, and **Transcribe after recording**.
- **Drop images** on the page: they are copied into an `images` folder next
  to the note and linked where the cursor is.
- **Install** a missing tool with one click: when whisper.cpp, its model,
  Claude Code, Codex or Ollama is not found (on Transcribe, Ask, or in
  Settings), rubrique offers its install command (`brew`, `npm`, `curl` or
  `ollama pull`) and shows the command first. Install scripts piped to a
  shell, and tools without a known command, get **Copy install command**
  and **Open install page** instead.
- **Settings → Accessibility**: text size for the whole window (also
  `Cmd/Ctrl` + `+`, `-`, `0`), editor font size, **Reduce motion** and a
  **Stronger focus outline**.

### Changed

- **Settings** is now a window over the app, with groups on the left:
  Appearance, Files, AI, Skills and Voice memos. A click outside or Esc
  closes it.
- macOS: release builds are signed with a Developer ID and notarized by
  Apple once the signing secrets are set. Until then the release notes say
  how to open the app the first time.
- AI answers are shown formatted: headings, lists, code blocks and links,
  with **Copy** for the raw text. Code blocks in an answer have their own
  **Copy**. Hover summaries show bold, italic and code.
- macOS asks for the microphone the first time you record.
- New dependency `cpal` for the microphone and speakers. Building on Linux
  needs `libasound2-dev`.

### Fixed

- Clicking the close cross of a tab with unsaved changes did nothing. It now
  asks to save, discard or cancel.
- Clicking **+ Add note** after selecting text in the preview opened the
  paragraph for editing instead of the note dialog.
- A block right after a code block inside a list or quote was drawn on top
  of the code block's header.

## [0.4.1] - 2026-10-28

### Added

- **Tabs**: every file opened from the explorer gets its own tab, right
  after the active one; opening a file that has a tab switches to it. Each
  tab keeps its unsaved changes, cursor, undo history and scroll position,
  so switching never asks to save. A dot marks unsaved changes. Close with
  the cross, a middle click or `Cmd/Ctrl+W` (unsaved changes ask first);
  right-click for **Close others**, **Close to the right** and **Copy
  path**; `Ctrl+Tab` / `Ctrl+Shift+Tab` move between tabs. Tabs reopen at
  the next start.
- **Images**: PNG, JPEG, GIF, WebP, BMP, ICO and TIFF files open in an image
  view, fitted to the page or at actual size (click the image or use the
  status bar button), with the size in pixels and the file size in the
  status bar. Animated GIF and WebP play.
- **Videos**: MP4, MOV, WebM, MKV, AVI and other video files show a card
  with **Play in video player** (the system's default player) and **Play
  in browser**.
- Markdown images `![alt](path)` show in the preview (relative paths start
  at the file's folder); a video path shows a card with **Play**. Open in
  browser shows them as `<img>` and `<video>`. Web images are not loaded in
  the preview.

### Changed

- New explorer icons in the style of Apple's symbols: **Add folder** is a
  folder with a solid plus badge, **New scratch** is a square with a pencil
  whose tip uses the accent color.
- Closing the window with unsaved changes in several tabs asks about each file in turn.
- New dependencies `egui_extras` and `image`, only to decode images.

### Fixed

- Undo after opening another file could bring back text from the file
  shown before; each file now has its own undo history.

## [0.4.0] - 2026-10-28

AI-first, still small: rubrique runs the AI command-line tools you already
have (Claude Code, Codex, Grok, or a local model through Ollama or
llama.cpp). It holds no API keys, and every change is shown in the merge
screen before it touches your text.

### Added

- **Ask** (`Cmd/Ctrl+K`, or the status bar button): ask an AI tool to change
  the selection (or the whole file). Quick actions: Shorter, Fix grammar,
  Table, Translate, Explain. The output streams into a panel with **Stop**;
  **Review in merge** shows each change next to the original, and nothing
  changes until you press **Apply**. The first run in a folder shows the
  exact command and asks first.
- Compare tools: pick **Compare with** in the Ask bar to run a second tool
  at the same time. The output panel shows both answers side by side, and
  **Compare in merge** puts them left and right of the result, with the
  time each took; every block where they differ is a choice.
- Resolve notes with AI: **Resolve** on a note card, or **Resolve all** in
  the Notes panel, sends the file and the note to the default tool. While it
  runs the card shows the time and **Stop**; the answer opens in the merge
  screen, where the resolved notes are removed once you apply.
- Skills: reusable instructions for Ask, one folder per skill with a
  `SKILL.md` file, the same format as Claude Code skills. Pick one from
  **Skills…** in the Ask bar. Settings → AI → Skills sets the folder
  (default `skills` in the config folder), can include Claude's own
  `~/.claude/skills`, and can create an example skill.
- Hover summaries: rest the pointer on a file in the explorer to see a
  two-sentence summary, made by a local model (Ollama or llama.cpp) by
  default and cached by content, so a file is summarized again only after
  it changes. Pick the tool or turn it off in Settings → AI → Summaries use.
- Smart scratch names: after you save a scratch that still has its random
  name, the summaries tool suggests a name from its content, like
  `2026-10-28-beta-launch-checklist.md`, in a bar with **Rename** and
  **Not now**. The Rename dialog has **Suggest** too.
- Settings → AI: pick the default tool (Claude, Codex, Grok or Local; ones
  not installed are greyed out), edit each tool's command (Installed with
  its path on hover, or Not found with a hint), pick the tool for summaries
  or turn them off, and forget the folders where AI runs were allowed.
  Built-in commands run read-only: Claude in plan mode with rubrique's
  instructions as a system prompt, Codex in a read-only sandbox.
- Code blocks in the preview: hover one to get **Copy**, and **Prettify**
  for ` ```json `, ` ```sql ` (also `psql`, `postgres`) and ` ```xml `
  blocks, which formats the code in the file like Format does. The
  language label and buttons sit in a slim row above the code.

### Changed

- Add folder and new scratch icons redrawn: a plain folder and a page with a
  separate "+" badge, and thinner lines on all icons so they stay sharp.

### Fixed

- Long lines in the merge screen's columns were stretched across the
  column with wide gaps between letters.

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

[Unreleased]: https://github.com/witseko/rubrique/compare/rubrique-v0.4.4...HEAD
[0.4.4]: https://github.com/witseko/rubrique/compare/rubrique-v0.4.3...rubrique-v0.4.4
[0.4.3]: https://github.com/witseko/rubrique/compare/rubrique-v0.4.2...rubrique-v0.4.3
[0.4.2]: https://github.com/witseko/rubrique/compare/rubrique-v0.4.1...rubrique-v0.4.2
[0.4.1]: https://github.com/witseko/rubrique/compare/rubrique-v0.4.0...rubrique-v0.4.1
[0.4.0]: https://github.com/witseko/rubrique/compare/rubrique-v0.3.0...rubrique-v0.4.0
[0.3.0]: https://github.com/witseko/rubrique/compare/rubrique-v0.2.0...rubrique-v0.3.0
[0.2.0]: https://github.com/witseko/rubrique/releases/tag/rubrique-v0.2.0
