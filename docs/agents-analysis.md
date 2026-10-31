# Agents in rubrique: analysis

Status: proposal, nothing built yet.

## The idea

In Settings you set up named agents. Each one has a personality, a set of
skills and an AI tool it runs on. In a note you write `@John`, and John does
the job the note asks for, in his own way.

```markdown
We ship on the first Monday<!-- note on "the first Monday": @John check this against the release plan -->
```

John reads the note, the text it is about and the file, and comes back with a
change you review in the merge screen, or with a reply to the note.

## What rubrique already has

Most of the pieces exist. Agents are mostly a new way to combine them.

| Piece | Where | What it gives agents |
| --- | --- | --- |
| AI tools as command templates (`claude`, `codex`, `grok`, `ollama`) | `src/ai.rs` (`AiTool`, `AiSettings`, `command_args`) | The "engine" an agent runs on. `{system}` already carries extra instructions. |
| Skills as `SKILL.md` folders | `src/ai/skills.rs` | The file format and loader to copy for agents. `Prompt::with_skill` shows how to add instructions. |
| Notes as HTML comments | `src/markdown/notes.rs` | The place to write `@John`. Notes stay hidden on GitHub, so a mention is safe to keep in the file. |
| Resolve one note / all notes | `src/app/ask.rs` (`resolve_notes`, `Resolving`) | The flow an agent run reuses: build prompt, run tool, open merge. |
| Compare two tools | `PendingRun.compare`, `MergeSession::compare` | `@John @Mary` can show both answers side by side. |
| Merge screen | `src/document/merge*.rs` | Nothing an agent does is applied without review. |
| Folder consent | `Consent`, `AiSettings.allowed_folders` | Cloud agents still ask once per folder. |
| No shell, arguments passed directly | `split_command`, `AiRun::start` | Text in a note cannot run commands. |

What is missing:

1. A way to describe an agent (a file format and a loader).
2. Finding `@Name` in notes and in the Ask bar.
3. More than one AI job at a time (today `AskState.job` holds one).
4. A place for an agent to answer without changing text (note replies).
5. A Settings group to create and edit agents.

## What an agent is

An agent is a folder with an `AGENT.md` file, the same shape as a skill:

```markdown
---
name: John
handle: john
description: Fact checker. Checks dates, numbers and names against nearby files.
tool: claude
model: opus
skills: fact-check, plain-english
output: reply
context: file
---
You are John, a careful fact checker on a small product team.

- Check every date, number and name against the other notes in this folder.
- If something is wrong, fix it. If you are not sure, say so and do not guess.
- Write short. No praise, no filler.
```

| Field | Meaning | Default |
| --- | --- | --- |
| `name` | Shown in the UI | Folder name |
| `handle` | What you type after `@` (case does not matter) | `name` in lower case, spaces removed |
| `description` | One line, shown in the `@` picker and Settings | "Agent {name}" |
| `tool` | `claude`, `codex`, `grok` or `local` | The default tool from Settings → AI |
| `model` | Passed to tools that support it (for example `--model` for `claude`) | Tool default |
| `command` | Full command template, for anything not covered above | The tool's command |
| `skills` | Skill names to always add | None |
| `output` | `rewrite` (change text, merge screen), `reply` (answer in the note) or `auto` (agent decides) | `rewrite` |
| `context` | `selection` (quoted text only), `file` or `folder` | `file` |
| Body | The personality and rules. Goes into `{system}` after rubrique's own rules. | Required |

This is close to Claude Code's subagent files (`.claude/agents/*.md` with
`name`, `description`, `tools`, `model`), the same way rubrique skills match
Claude Code skills. rubrique can offer to read `~/.claude/agents` too, like
it reads `~/.claude/skills` today.

### Where agents live

- **Your agents**: `<config>/rubrique/agents/` (next to the skills folder).
- **Folder agents**: `.rubrique/agents/` inside a notes folder, so a team can
  share John through git. These load only in folders you allowed for AI,
  because a downloaded folder could ship a hostile agent.
- **Claude Code agents**: `~/.claude/agents`, behind a checkbox.

If two have the same handle, the folder agent wins over yours, and yours wins
over Claude Code's. Settings shows which one is used.

## How a mention works

### In a note

1. You add a note: `@John check this against the release plan`.
2. The notes panel shows the note with a small `John` chip and a **Run**
   button. The quoted text in the preview gets John's color.
3. You press **Run** (or **Run all mentions** at the top of the panel).
4. rubrique builds the prompt:
   - **system**: rubrique's rules for the output type, then John's body, then
     his skills.
   - **user**: the note text without `@John`, the quoted text, and the file
     (or only the quote, if `context: selection`).
5. John runs on his tool, in the file's folder, read-only, as Resolve does now.
6. The result:
   - `rewrite`: the merge screen opens with the change and the note removed.
     Title: "John · Resolve note: the first Monday".
   - `reply`: John's answer is added under the note as a reply (see below).
     You can then press **Apply** on the reply to turn it into a rewrite, or
     reply back with `@John` again.

### Several mentions

- `@John @Mary check this`: both run at once and the merge screen compares
  them. This is the existing Compare feature, with agents instead of tools.
- A mention of an unknown handle shows "No agent called @jon. Did you mean
  @john?" on the note card and does nothing.

### In the Ask bar

Typing `@` in the Ask bar (Cmd+K) opens a picker of agents. `@John make it
shorter` runs John on the selection with his personality and skills. This is
the cheapest place to start, because Ask already handles selection, consent,
compare and merge.

## Note replies (the new storage format)

A reply has to live in the file, so it survives closing the app and is seen
by teammates. Proposal: a second comment right after the note:

```markdown
the first Monday<!-- note on "the first Monday": @John check this against the release plan --><!-- reply from "John": The release plan says Tuesday 3 March. -->
```

- `find_notes` collects replies that follow a note into a thread.
- GitHub and other viewers still hide both comments.
- Deleting a note deletes its replies.
- A reply is plain text; it never changes the document by itself.

Why not keep replies outside the file (a sidecar file or app memory)? The file
would move or be renamed and lose them, and teammates would not see them.
Keeping everything in the Markdown matches how notes already work.

## Running more than one job

Today `AskState` has one `job: Option<AiJob>`, and a second request shows
"Wait for the AI tool to finish". With agents you will often run several
notes at once.

Proposal:

- `jobs: Vec<AiJob>`, each with an id, the file, the note and the agent.
- A limit (for example 3 at a time); more wait in a queue.
- The notes panel shows per note: waiting, running (with seconds), done,
  failed. This builds on the existing `resolving_state`.
- Finished rewrites wait in a **To review** list (status bar badge) instead of
  opening the merge screen on top of whatever you are doing. You open them one
  by one. Each review still checks that the text did not change meanwhile, as
  `review_answer` does now.

## Settings → Agents

A new group next to Skills, using the same window.

- A list of agents with name, handle, tool and where the file is.
- **New agent**: a form with Name, Handle, Description, Tool (dropdown),
  Model, Skills (checkboxes from the loaded skills), Output, Context and a
  large Personality field. **Save** writes `AGENT.md`, so the file stays the
  source of truth and can be edited by hand or shared.
- **Open file** and **Show in folder**, like Skills.
- **Try it**: run the agent on a sample text inside Settings.
- A few starter agents to copy: Editor (tightens prose), Fact checker,
  Translator, Summarizer (local model). Same idea as the example skill.

## Safety

The rule "every AI change waits in the merge screen" stays. Agents make this
more important, not less.

- **Read-only tools.** Agents use the same read-only command flags as today
  (`--permission-mode plan`, `--sandbox read-only`). An agent file cannot turn
  on write mode by itself; a custom `command` that allows writes shows a
  warning in Settings and in the consent dialog.
- **Mentions never run by themselves.** A file from someone else may contain
  `@John delete the second half`. Running needs a click. An opt-in "run on
  save" per agent can come later, and only for local tools or allowed folders.
- **Folder agents are untrusted until the folder is allowed** (see above).
- **Prompt injection.** The note text and file go in the user part of the
  prompt, inside tags, as now. The agent body goes in `{system}`. This does
  not make injection impossible, which is another reason the merge screen
  stays mandatory.
- **Cost and time.** Show the tool and model on each chip, so a user sees that
  `@John` runs on a cloud model and `@Sum` on a local one.

## Code changes, by file

| File | Change |
| --- | --- |
| `src/ai/agents.rs` (new) | `Agent` struct, `parse_agent`, `load_agents` (copy of `skills.rs` patterns), starter agents, tests |
| `src/ai.rs` | `Prompt::with_agent`, `agent_note_request`, reply output rules, agent command (tool + model or own `command`) |
| `src/markdown/notes.rs` | `mentions(text) -> Vec<String>`, reply comments, notes with threads, delete with replies |
| `src/markdown/render.rs` / `inlines.rs` | Highlight `@handle` in notes and agent colors on quoted text |
| `src/app/ask.rs` | `PendingRun.agent`, `Resolving::One` by agent, `jobs: Vec<AiJob>`, queue, review list |
| `src/app.rs` | Notes panel: chips, Run, Run all mentions, threads, Apply on replies |
| `src/ai/view.rs` | `@` picker in the Ask bar, agent name in the output panel and consent dialog |
| `src/settings.rs`, `src/ai.rs` (`AiSettings`) | `agents_folder`, `use_claude_agents`, parse and write lines |
| `src/settings/view.rs` | `SettingsGroup::Agents` and its form |

## Plan in steps

Each step is useful on its own and can ship separately.

1. **Agents in Ask.** `AGENT.md` format and loader, Settings → Agents as a
   list with Open file (like Skills), `@` picker in the Ask bar. One job at a
   time. Smallest change, tests the format.
2. **Agents in notes.** `@John` in a note, chip and Run on the note card,
   output `rewrite` through the existing Resolve flow. Unknown handle hint.
3. **Replies and threads.** `output: reply`, reply comments in the file,
   threads in the notes panel, Apply on a reply.
4. **Many jobs.** Job list, queue, per-note status, To review list, Run all
   mentions. `@John @Mary` compare.
5. **Settings form and team agents.** Create and edit agents without touching
   the file, starter agents, folder agents in `.rubrique/agents`, Claude Code
   agents import.
6. **Later, maybe.** Run on save for chosen agents. Agent hand-off
   (`@John then @Mary`). Folder-wide jobs ("@John check every note in this
   folder"). Agents that may write files, but only into a copy that comes
   back through the merge screen.

## Open questions

1. Should `@John` in normal text (not in a note) mean anything? Proposal: no,
   only in notes and the Ask bar, so writing about a person called John never
   triggers anything.
2. Default output: `rewrite` (closest to today) or `reply` (safer, more
   "chat with a colleague")? Proposal: `rewrite` for step 2, make `reply`
   available in step 3, and let each agent choose.
3. Handle collisions with real people in shared notes: should agent handles
   use a prefix such as `@ai-john`? Proposal: plain handles, plus a Settings
   check that warns if a handle looks like a common name used in the folder.
4. Should the merge screen show which agent made each change when an agent
   rewrites a whole file with many notes? Useful for "Run all mentions".
