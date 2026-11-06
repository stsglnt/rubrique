# Simpler agent setup

Status: design. Mockups: https://claude.ai/artifact/4DhZkfHc9BYUvazjgUJywo

## The problem

Today, making an agent means: Settings → Agents → "Create example agent",
then open `AGENT.md` and edit YAML-like front matter by hand (`handle`,
`tool`, `model`, `skills`). The folder controls sit at the top, before the
agents. There is no way to test an agent before using it.

## The new flow

Describe → Review → Try → Save. All inside Settings → Agents.

1. **Agents list.** One card per agent: avatar letter, `@ai-handle`, name,
   engine ("Claude · opus"), description, and Try / Edit buttons. A
   "+ New agent" button at the top. Claude Code agents get a small
   "from Claude Code" tag and an Open button (we do not rewrite their files).
   Folder settings move into a closed "Where agents live" section at the
   bottom.
2. **Describe.** One box: "What should this agent do?" Starter chips
   (Editor, Fact checker, Translator, Summarizer, Meeting notes to tasks)
   fill the box. "Draft agent with [tool]" asks an installed AI tool to
   draft the fields. "Start blank" skips the AI.
3. **Review.** A form with every field the AI drafted:
   - Name, with the mention shown live (`@ai-john`). The handle follows the
     name until you edit it.
   - What it does (description).
   - Rules (the body of `AGENT.md`).
   - Runs on: pills for Default and each tool; missing tools are disabled.
     A model drop-down when a tool is picked.
   - Skills: toggle chips from the skills found.
   - Advanced (closed): own command, and "Open file" for the raw `AGENT.md`.
4. **Try.** Runs the unsaved agent on sample text (the current selection, or
   a short sample). Shows the answer inline. "Save agent" or "Change rules".

Nothing is written to disk until Save. Save writes
`<agents folder>/<handle>/AGENT.md` in the same format as today, so
existing agents and Claude Code agents keep working. Edit opens the same
Review form filled from the file.

## Implementation notes

- `src/ai/agents.rs`: add `AgentDraft` (form fields), `to_markdown()` to
  write `AGENT.md`, and `save_agent(folder, draft)`. Parse the AI's draft
  from a small fixed format (front matter + body), reusing `parse_agent`.
- `src/settings/view.rs`: replace `agents_section` with a small state
  machine (`List`, `Describe`, `Review`, `Try`) kept in the app.
- Drafting and Try reuse `AiRun` from `src/app/ask.rs` (one job at a time,
  folder consent still applies).
- Starter templates are constants next to `EXAMPLE_AGENT`.
