# What rubrique is

This page holds the words used for rubrique in the README, the package
descriptions (`Cargo.toml`, `packaging/linux/rubrique.desktop`) and the
GitHub repository description. Change it first, then the others, so they
keep saying the same thing.

## Identity

rubrique is **a fast Markdown editor with agents on demand**. Two halves,
in this order:

1. **An editor first.** Small, quick, and it saves exactly what you typed.
   Open a folder, write, close. No agent is involved until you ask.
2. **Agentic on demand.** One shortcut away, agents work on your notes. You
   review every change and decide what lands in the file.

Everything else is a detail of the current version: which agents, which
systems, how they connect, whether a key is needed. Details change between
versions. The identity does not, so the headline, the short description and
the first paragraph of the README must not depend on them.

## One line

A fast Markdown editor with agents on demand.

## Short description

For GitHub, package managers and app listings:

> A fast Markdown editor with agents on demand: when you ask, agents work on
> your notes and you review every change.

## The pitch

rubrique is an editor first: small, quick, and it saves exactly what you
typed. When you want help, agents are one shortcut away. They work on your
notes, you review every change in a merge screen and decide what lands in
the file. A native app for macOS and Linux, written in Rust.

## What "agentic" means here

The agent is the engine. rubrique is the place where it works on prose. It
hands the agent the text, your request and a skill, runs it in the notes
folder, and brings the answer back into a merge screen. The agent decides how
to do the work. You decide what lands in the file.

"On demand" is part of the identity. Agents run only when you ask, except
local summaries, which can be turned off. Without an agent installed,
rubrique is still a complete editor.

## Details of the current version

Say these further down the page, after the identity, and in the present
tense ("today", "in this version"):

- Agents: Claude Code, Codex, Grok and a local model through Ollama or
  llama.cpp. rubrique runs the programs already on the computer.
- No API keys and no account of rubrique's own, in this version.
- Systems: macOS and Linux.
- Transcription with whisper.cpp on the computer.

## Where it is going

- More AI providers.
- Direct API access next to the command-line agents.

This is why the identity must not say "command-line tools", "no API keys" or
name a provider. Those sentences would become false.

## Who it is for

People who already work with agents and keep notes, docs or drafts in
Markdown. They want the same agents for writing, with every change under
their control.

## What sets it apart

1. **An editor you would use without the agents.** Fast, native, saves
   exactly what you typed.
2. **Agents on demand.** Ask one, or two at once and compare.
3. **Every change is reviewed.** The merge screen is the only way text from
   an agent reaches a file.
4. **Nothing hidden.** The Ask bar shows the request, the instructions, the
   skill, the exact command and the folder before anything runs.
5. **Your agents, your rules.** Skills in the same format as Claude Code,
   and your own commands for each agent.

## Words

Use: editor, fast, agent, on demand, merge screen, review, skill, note. Name an agent
when the sentence is about that agent.

Avoid: AI-assisted, assistant, copilot, chat, autopilot, "the AI writes for
you". Avoid putting a provider, a platform or "no API keys" in the headline.

## Not true yet, so do not claim it

- Agents that write files or run commands from rubrique. The built-in
  commands are read-only (Claude Code in plan mode, Codex in its read-only
  sandbox) and rubrique's instructions ask the agent not to use tools.
- Long tasks across many files, or an agent that keeps its session between
  requests.
- API access, or providers beyond the four above.
- Windows.
