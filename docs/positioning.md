# What rubrique is

This page holds the words used for rubrique in the README, the package
descriptions (`Cargo.toml`, `packaging/linux/rubrique.desktop`) and the
GitHub repository description. Change it first, then the others, so they
keep saying the same thing.

## Identity

rubrique is an **agentic Markdown editor**. That is the whole identity.
Agents do the work on your notes. You ask, review every change, and decide
what lands in the file.

Everything else is a detail of the current version: which agents, which
systems, how they connect, whether a key is needed. Details change between
versions. The identity does not, so the headline, the short description and
the first paragraph of the README must not depend on them.

## One line

An agentic Markdown editor.

## Short description

For GitHub, package managers and app listings:

> An agentic Markdown editor: agents work on your notes, you review every
> change.

## The pitch

Agents do the work on your notes. You ask, review every change in a merge
screen, and decide what lands in the file. rubrique is a small native app
for macOS and Linux, written in Rust, and it saves exactly what you typed.

## What "agentic" means here

The agent is the engine. rubrique is the place where it works on prose. It
hands the agent the text, your request and a skill, runs it in the notes
folder, and brings the answer back into a merge screen. The agent decides how
to do the work. You decide what lands in the file.

It does not mean rubrique acts without being asked. Agents run only when you
ask, except local summaries, which can be turned off.

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

1. **Agents do the writing.** Ask one, or two at once and compare.
2. **Every change is reviewed.** The merge screen is the only way text from
   an agent reaches a file.
3. **Nothing hidden.** The Ask bar shows the request, the instructions, the
   skill, the exact command and the folder before anything runs.
4. **Your agents, your rules.** Skills in the same format as Claude Code,
   and your own commands for each agent.
5. **A real editor.** Small, native, saves exactly what you typed.

## Words

Use: agent, agentic editor, merge screen, review, skill, note. Name an agent
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
