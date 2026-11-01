# What rubrique is

This page holds the words used for rubrique in the README, the package
descriptions (`Cargo.toml`, `packaging/linux/rubrique.desktop`) and the
GitHub repository description. Change it first, then the others, so they
keep saying the same thing.

## One line

An agentic Markdown editor for macOS and Linux.

## Short description

For GitHub, package managers and app listings:

> Agentic Markdown editor for macOS and Linux. Write with Claude Code, Codex,
> Grok or a local model; every change is reviewed in a merge screen. No API
> keys.

## The pitch

rubrique puts the agents you already have to work on your notes: Claude Code,
Codex, Grok or a local model. Ask one agent, or ask two and compare their
answers. Agents run in your notes folder, read-only, and every change waits
in a merge screen until you accept it. There are no API keys, no account and
no cloud of rubrique's own. It is a small native app written in Rust, and it
saves exactly what you typed.

## What "agentic" means here

Terminal agents such as Claude Code and Codex are the engine. rubrique is
the place where they work on prose. It hands them the text, your request and
a skill, runs them in the notes folder, and brings the answer back into a
merge screen. The agent decides how to do the work. You decide what lands in
the file.

It does not mean rubrique has a model of its own or acts without being asked.
Agents run only when you ask, except local summaries, which can be turned off.

## Who it is for

People who already use Claude Code, Codex or a similar agent for code and
keep notes, docs or drafts in Markdown. They want the same agent for writing,
with no second subscription, no API keys and no cloud that holds their notes.

## What sets it apart

1. **Your agents, not ours.** Their login, their skills, their bill. rubrique
   adds nothing in between.
2. **Every change is reviewed.** The merge screen is the only way text from
   an agent reaches a file.
3. **Two agents at once.** Compare answers side by side and keep the best
   parts of each.
4. **Nothing hidden.** The Ask bar shows the request, the instructions, the
   skill, the exact command and the folder before anything runs.
5. **Private by default.** A small native app. Local models do the
   background work. Cloud agents run only in folders you allowed.

## Words

Use: agent, agentic editor, Claude Code, Codex, Grok, local model, merge
screen, review, skill, note.

Avoid: AI-assisted, assistant, copilot, chat, autopilot, "the AI writes for
you".

Name the agents. "Claude Code and Codex" says more than "AI".

## Not true yet, so do not claim it

- Agents that write files or run commands from rubrique. The built-in
  commands are read-only (Claude Code in plan mode, Codex in its read-only
  sandbox) and rubrique's instructions ask the agent not to use tools.
- Long tasks across many files, or an agent that keeps its session between
  requests.
- Windows.
