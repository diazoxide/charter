### Removed

- **Persona sub-agents are retired. After updating, run `purlis doctor --fix persona-agents` in
  each project, then save.** purlis no longer generates `.claude/agents/<persona>.md`, and
  `purlis persona sync-agents` (with its `--approve-mcp`) is gone: the word is still recognised
  and answers with one sentence that says what replaced it. The fix removes the files purlis
  generated, where git tracks them and they have no uncommitted change, and leaves and names
  every other file. It prints everything it changed, makes no commit, runs only by name, and
  changes nothing on a second run. Until it is run, Claude Code still lists each generated
  file as an agent type; a call to one is refused, and each chat is told so at its start. A
  project's own `CLAUDE.md`, README, charters and scripts may still say `sync-agents` or
  `subagent_type`: search for both (#1451).
- **A persona whose `model:` named a model now runs on the asking chat's profile.** `model:
  opus`, `sonnet` or `haiku` picked the generated sub-agent's model, and nothing reads it now,
  so that persona's chats run on the model of the chat that dispatched them, which may cost
  more or less. Name the profile a persona's chats start on with `profile:`. The fix and
  `purlis persona lint` say this for each persona it applies to. A `model:` that is the name of
  a profile the project carries is rewritten to `profile:` (#1451).
- **`agent-tools:`, `skills:` and `memory:` are read by nothing.** A
  persona chat has every tool of its harness, so a persona that listed no editing tool could
  not edit files as a sub-agent and can as a chat. Nothing asks before two persona chats write
  in one tree; `dispatch-isolation: worktree` now gives a dispatched chat a worktree of its own
  instead (#1453). `purlis persona lint` and the fix say what widened for each persona and leave
  the lines. To keep a persona from a tool, name it in `disallowed-tools:`. The hooks no longer
  log a returned sub-agent or a message to one in the dispatch log (#1451).

### Changed

- **A persona is no longer a harness sub-agent.** A sub-agent call whose type is a persona is
  refused on Claude Code, and the refusal names the route: `purlis dispatch --to <persona>`,
  or, for a draft, that its definition has to be finished first. A helper that is not named
  for a persona still runs, as its chat's persona. On opencode the `task` tool reaches the same
  refusal; Codex's sub-agents carry no type, so there is nothing to refuse there (#1451).
- **A persona chat on Claude Code is started with its persona's MCP servers.** The servers of
  `personas/<name>/mcp.json` were started for the generated sub-agent; they are started for
  the chat that runs as the persona, for that chat alone. A server that takes a credential
  from the persona's vault still goes through `purlis secret exec`, and is started only once a
  person approved it on this machine. An approval given before carries over. An unapproved
  server is withheld, and the chat is told which and how it is approved. On Codex and opencode
  a persona's servers are not started, and the chat is told (#1451).
- **`disallowed-tools:` is honoured for a persona chat on Claude Code, and refuses the chat
  elsewhere.** The chat is started with those tools denied. On Codex and opencode purlis
  cannot deny them, so starting a chat as that persona, or dispatching to it, is refused with
  a sentence (#1451).
- **A persona's one-line description is told to its chat.** `agent-description:`, or
  `description:` where there is none, is quoted in the briefing of a chat that runs as the
  persona and shown by `purlis persona show`. `purlis persona create` and a project template
  write no sub-agent file (#1451).
- **`purlis persona stats` and the persona skill count and describe dispatches.** `DISP` is
  read from the committed dispatch log and stops moving until it is counted from each
  dispatch's record. `purlis persona lint` no longer checks declared skills against the
  installed ones, and warns when a charter still teaches the sub-agent route (#1451).

### Added

- **`purlis persona approve-mcp`** approves the MCP servers of a persona that take a
  credential from its vault: it shows the line each would run and asks, on a terminal.
  `--persona` narrows it to one, `--yes` approves without asking, `--dry-run` records nothing.
  It is refused inside a chat (#1451).

### Security

- **A persona's deny-list fails closed.** Where purlis cannot deny a chat the tools a persona's
  `disallowed-tools:` names, the chat is not started. And a persona's credentialed MCP server
  is never started without a person's approval on that machine, which a chat cannot give
  (#1451).
