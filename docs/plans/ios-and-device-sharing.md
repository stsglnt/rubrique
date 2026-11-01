# Plan: shared sessions over Tailscale, then an iOS app

Status: proposal, nothing built yet.

## Goal

Open the same notes on a Mac or Linux computer and on an iPhone, and pick up
where you left off: the same folders, the same open tabs, unsaved text, and
the AI tools installed on the computer. Notes never go to a cloud service run
by us or anyone else. They move only between your own devices, over your own
Tailscale network (tailnet).

The iOS app comes second. First we prove that sharing works with a small
server in the desktop app and a throwaway test client.

## What "session" means in rubrique today

| Part | Where it lives now |
| --- | --- |
| Explorer folders | `Settings::folders`, in the `settings` file |
| Open tabs, active tab | `Settings::open_tabs`, `Settings::last_file` |
| Unsaved text, cursor, undo | Only in memory, per tab (`src/app/tabs.rs`) |
| Notes on text | Inside the Markdown file itself (`src/markdown/notes.rs`) |
| AI tools | Programs on the computer (`claude`, `codex`, local model), run by `src/ai/run.rs` |
| Voice transcription | whisper.cpp on the computer (`src/voice`) |

Two things matter for the design:

1. The files are the source of truth. rubrique already checks every few
   seconds whether a file changed on disk (`check_disk` in `src/app.rs`) and
   opens the merge screen if the tab has unsaved edits. A save from the phone
   is just another program changing the file, so the desktop needs no new
   conflict code.
2. AI and transcription only run on the computer. An iPhone cannot run
   `claude` or whisper.cpp from rubrique. So the phone should ask the
   computer to run them. This is the main reason to share a *session*, not
   just sync files.

## Approach: the computer hosts, the phone connects

```
 iPhone (rubrique iOS)                 Mac or Linux (rubrique desktop)
 ---------------------                 -------------------------------
 browse, edit, preview   --HTTP-->     small server, tailnet address only
 offline copy of recent  <--events--   reads/writes files in your folders
 files                                 runs AI tools and whisper.cpp
          \___ encrypted by Tailscale (WireGuard), device to device ___/
```

- The desktop app gets a "Share with my devices" switch. When on, it starts
  a small HTTP server that listens **only on its Tailscale address**
  (100.x.y.z), never on all interfaces.
- The phone talks to it with plain requests and gets live updates through
  one event stream.
- No full folder sync. The phone downloads files when you open them and
  keeps recent ones for reading offline. Edits made offline are saved when
  the computer is reachable again, with a merge if needed.

### Why not full two-way folder sync

Syncthing-style sync means copying whole folders to the phone, handling
deletes and renames on both sides, and running conflict logic on the phone.
It also does not help with AI, which needs the computer anyway. Host and
client is much less code and keeps one copy of the truth. We can add
"keep this folder on my phone" later if people ask.

### Why not live co-editing

Typing on two devices at the same moment (like Google Docs) needs CRDTs and
would change how rubrique stores text. The real use is handoff: start on the
Mac, continue on the phone. Saves with a version check plus the existing
merge screen cover that.

## Tailscale: two ways to connect the phone

| | A. Tailscale app on the phone | B. Tailscale built into our app |
| --- | --- | --- |
| How | User installs the official Tailscale app on both devices. rubrique connects to the Mac's tailnet name or IP. | Embed Tailscale's `tsnet` through `TailscaleKit` (part of `tailscale/libtailscale`). |
| Our work | Almost none | Build a Go library for iOS, sign-in screen, keep it up to date |
| App size | No change | Adds tens of MB (Go runtime) |
| VPN slot | Uses the phone's one VPN slot | No VPN slot needed (runs in the app) |
| Works in background | Yes, Tailscale stays up | Only while our app runs |
| Upstream support | Official | No official prebuilt package; community builds exist |

**Recommendation: A for the spike and the first iOS release.** It is
almost free and lets us test everything else. Look at B again only if
users say installing Tailscale is a blocker.

## Privacy: what "nothing goes to the cloud" really means

We should say this plainly in the app and the README.

- **Note contents** travel only between your devices, encrypted end to end
  by WireGuard. Nobody in the middle can read them.
- **Tailscale's coordination server** knows your device names, their
  addresses and public keys. It never sees traffic.
- **Relays (DERP):** if two devices cannot reach each other directly,
  Tailscale relays the *encrypted* packets. The relay cannot read them, but
  it does carry them. `tailscale ping` shows whether a link is direct.
- **For zero third parties**, run [Headscale](https://github.com/juanfont/headscale)
  as your own coordination server (and optionally your own relay). The plan
  should work with Headscale with no code changes; we test it in the spike.
- **HTTPS certificates:** `tailscale cert` gets a Let's Encrypt certificate
  for the device name. That name then appears in public certificate logs.
  Prefer plain HTTP inside the tailnet (already encrypted) if iOS allows it;
  see open questions.
- **AI from the phone** runs on the computer under the same rules as today:
  local models stay local, and cloud tools (Claude, Codex, Grok) only run in
  folders you allowed. The phone must show which tool will run, same as the
  desktop Ask bar.

## Who may connect

No passwords and no accounts. Tailscale already knows who each device
belongs to.

1. The server accepts connections only on the tailnet address and only from
   tailnet ranges (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`).
2. For each new peer it asks Tailscale who it is: `tailscale whois --json <ip>`
   (on Linux, the LocalAPI socket is an option too). Answer includes the
   user login and the device name.
3. Allow only devices owned by **the same Tailscale user** as the computer.
   Tagged devices (servers with no human owner) are refused.
4. The first time a device connects, the desktop asks: "Vitalii's iPhone
   wants to open your notes. Allow / Deny". Allowed devices are stored in
   settings and can be removed in Settings → Devices.
5. If the whois check fails or Tailscale is not running, refuse
   (fail closed). Cache answers for about a minute.
6. Use the real TCP peer address only. Ignore any identity headers.

## Server design (desktop, Rust)

New module `src/remote/`:

- `server.rs`: runs on its own thread, like AI runs do today. Talks to the
  UI through a channel and wakes it with `ctx.request_repaint()`. Uses a
  small blocking HTTP crate (`tiny_http` or similar) to stay in line with the
  current design (threads, no async runtime). Event stream as Server-Sent
  Events.
- `auth.rs`: whois check, allowed devices, cache.
- `protocol.rs`: request and response types, versioned (`/v1/...`).
- `paths.rs`: every path must be inside one of the explorer folders after
  resolving `..` and symlinks. Everything else is refused.
- A shared `SharedSession` (behind `Arc<Mutex<..>>`) that the UI updates
  when tabs, the active tab or unsaved text change, so the server can read it
  without touching the UI thread.

Also a headless mode, `rubrique --serve`, for a Linux machine without a
screen (a home server with your notes and models).

### First version of the API

| Request | What it does |
| --- | --- |
| `GET /v1/hello` | Host name, rubrique version, API version |
| `GET /v1/folders` | Explorer folders |
| `GET /v1/tree?path=` | Files and subfolders of one folder |
| `GET /v1/file?path=` | Text plus `version` (hash of the content) |
| `PUT /v1/file?path=` with `base_version` | Save. `409` with the current disk text if the file changed, so the phone can merge |
| `GET /v1/session` | Open tabs, active tab, which tabs have unsaved text |
| `GET /v1/session/draft?path=` | Unsaved text of a desktop tab ("continue on phone") |
| `GET /v1/events` | Live stream: file changed, tabs changed, AI output |
| `POST /v1/ai/run` | Run an Ask request on the computer. Result comes back as a proposed change, never applied directly |
| `POST /v1/voice` | Upload a memo recorded on the phone, transcribe on the computer |

Rules kept from the desktop app: never reformat a file, and every AI change
is a proposal the user accepts in a merge screen.

### How conflicts work

- Phone saves while the desktop tab has no unsaved edits: desktop reloads
  the file as it does today.
- Phone saves while the desktop tab has unsaved edits: `check_disk` notices,
  desktop shows its existing merge screen.
- Desktop saved since the phone opened the file: the server answers `409`,
  the phone shows a merge.

## The spike (before any iOS app)

Each step has a clear "done" check. Stop and rethink if one fails.

**Step 0. Can the phone reach the computer? (half a day)**
- On the Mac: `python3 -m http.server 8080 --bind <tailscale ip>`.
- On the iPhone with Tailscale on: open it in Safari.
- Run `tailscale ping` between them at home, on mobile data, and on a
  hotel-like network. Note direct vs relay and the delay.
- Small SwiftUI test app with `URLSession`: does plain `http://` to the
  tailnet address work, and what App Transport Security setting does it need?
  Does iOS ask for Local Network permission?
- Done when: a test app loads a file from the Mac over the tailnet.

**Step 1. Read-only server in rubrique (2 to 3 days)**
- `rubrique --serve` with `hello`, `folders`, `tree`, `file`.
- Whois check and the "Allow this device?" prompt (in headless mode: an
  allow list in settings).
- Path checks with tests (`..`, symlinks out of a folder, absolute paths).
- Done when: `curl` from a laptop on the tailnet lists notes, and a device of
  another Tailscale user is refused.

**Step 2. Saving and conflicts (2 days)**
- `PUT /v1/file` with versions. Test the three conflict cases above by hand
  with the desktop app open.
- Done when: an edit from the test client shows up on the desktop, and a
  clash opens the merge screen with nothing lost.

**Step 3. Session sharing (2 days)**
- `SharedSession`, `/v1/session`, drafts, event stream.
- Done when: the test client sees a tab open on the desktop within a second
  and can read its unsaved text.

**Step 4. AI and voice from the phone (2 days)**
- `POST /v1/ai/run` reusing `src/ai/run.rs`, output streamed as events.
- `POST /v1/voice` reusing the whisper.cpp path.
- Done when: the test client asks "Shorter" and receives a proposed change.

**Step 5. Decisions (half a day)**
- Try the same setup with Headscale.
- Decide: Tailscale app (A) or built in (B), HTTP or HTTPS, and whether the
  desktop needs to keep serving while its window is closed.

About two weeks in total. The output is a working desktop server, its tests,
and a short write-up of what we learned.

## The iOS app (after the spike)

**Tech**
- SwiftUI, iOS 17 or later, `URLSession`, no third-party packages to start.
- Not egui: text input, the iOS keyboard, selection and VoiceOver are much
  better in native controls.
- Later, share Rust code (Markdown parsing, notes, merge) with the desktop
  through a small `rubrique-core` crate and UniFFI, so preview and merge look
  and behave the same on both. For the first version, render the preview and
  merge with simple Swift code, or get rendered HTML from the server.

**First version**
- Add a computer: pick it from tailnet devices or type its name. The
  desktop shows the "Allow?" prompt.
- Browse folders, open, edit and save Markdown files.
- Preview mode.
- "Continue from Mac": open the desktop's active tab, including unsaved text.
- Ask AI (runs on the computer), review the change, accept or reject.
- Record a voice memo on the phone, transcribe on the computer.
- Recent files readable offline; offline edits saved later with a merge.

**Later**
- Notes (add, resolve).
- Several computers.
- Themes and text size matching desktop settings.
- Share sheet ("Save to rubrique"), widgets, Shortcuts.
- Built-in Tailscale (option B) if needed.

**Release**
- TestFlight first. For App Store review the app needs a computer to talk
  to, so we need a demo setup and clear review notes.

## Open questions for the spike

1. Does iOS allow plain `http://` to a `100.x` address, or to a `*.ts.net`
   name, with a narrow App Transport Security exception? If not, use
   `tailscale cert` and accept the public name, or use a self-signed
   certificate pinned on first connect.
2. Does `tailscale whois` work the same with the App Store, standalone and
   open source Tailscale builds on macOS? The CLI location differs.
3. Should the desktop keep serving after the last window closes (menu bar
   or tray icon), or only while the app is open?
4. How long can the phone keep the event stream open in the background? We
   may need to refresh on app open instead.
5. Does every request work through a Tailscale relay with acceptable delay
   for typing and saving?

## Sources

- [TailscaleKit prebuilt framework (third-party)](https://github.com/indiagrams/tunnelless)
- [tailscale-localapi Rust crate](https://docs.rs/crate/tailscale-localapi/latest/source/src/types.rs)
- [Example whois-based auth in a Rust server](https://docs.rs/isb-server/1.2.0/src/isb_server/server/tailnet.rs.html)
- [Example identity checks and fail-closed rules](https://packagist.org/packages/geocodio/laravel-tailscale-identity)
