# Can a running Claude Code chat take a new sandbox grant?

Spike for #1347 (parent spec #1330). The answer decides how the grant flow (#1342)
applies "Allow for this chat" and "Allow in this project" to a Claude Code chat that is
already running.

**Version:** Claude Code 2.1.291 (`claude --version`), the macOS arm64 native build.

**How this was found.** Everything below comes from reading the code in the 2.1.291
binary (`strings` on the bundle). A live run was not possible in the session that wrote
this note, for three reasons:

- macOS refuses a nested Seatbelt profile (`sandbox-exec: sandbox_apply: Operation not
  permitted`), so a nested chat with `failIfUnavailable` cannot start its sandbox.
- Binding a local port or a unix socket for a stand-in model server is refused.
- No API key was available without reading the keychain, which the spike was told not to
  touch.

Each claim below says how sure it is. The recipe at the end turns the "read" claims into
measured ones. Run it outside an agent's sandbox.

## How purlis hands Claude Code its sandbox today

`harness/claude.rs` passes one `--settings '<inline JSON>'` on the argument line. The
JSON carries the `sandbox` object from `sandbox/claude.rs::settings()`, with
`allowUnsandboxedCommands: false` and `network.strictAllowlist: true`, plus the
`permissions.deny` twins. There is no file, so a chat has no file it can rewrite.

## What reloads mid-chat (2.1.291)

Claude Code has a settings watcher. When a watched source changes, the sandbox manager
subscribes to the change signal and rebuilds its config from the merged settings. Its
debug log says `Sandbox configuration updated from settings change`. Claude Code's own
guidance to the model says a sandbox key "applies without a restart". The rebuilt config
is used by the **next** Bash command. A command that is already running keeps its
profile.

The question is therefore which **source** can change while a chat runs, and which
sandbox keys that source may set. Under purlis's settings, `strictAllowlist: true` and
`allowUnsandboxedCommands: false` limit allowed hosts and filesystem grants to the trusted
tiers: managed policy, `flagSettings` (`--settings` and the SDK's flag layer) and user
settings. Hosts from project or local settings are dropped, and the log says so:
`[sandbox] network.strictAllowlist: ignoring N sandbox.network.allowedDomains … from
project/local settings`.

| Source a grant could ride in | Reloads mid-chat? | Hosts honoured under purlis's settings? | Write paths honoured? | Confidence | Fit for #1342 |
|---|---|---|---|---|---|
| `--settings '<inline JSON>'` (what purlis uses) | **No.** It is fixed on argv. Claude Code writes it to a hashed temp file and pins that content. | yes | yes | High (read) | Only by restarting |
| `--settings <file path>` | **No.** The file is read once at launch and its content pinned (`flagSettingsFilePinnedContent`). Every later read of `flagSettings` uses the pinned text, not the disk. | yes | yes | High (read) | No |
| SDK control request `apply_flag_settings` (`Query.applyFlagSettings`) | **Yes.** It shallow-merges into the flag layer, then `notifyChange("flagSettings")`, and the sandbox rebuilds. | yes (flag tier) | yes | High (read) | Only for a chat driven over the SDK's stream-json control channel. A TUI chat in a terminal has no such channel. |
| SDK control request `register_repo_root` | **Yes.** It adds a working directory for the session and calls `refreshConfig()`. | n/a | yes (adds a whole working directory) | High (read) | SDK only. It is coarse: a working directory, not a write grant. |
| User settings `~/.claude/settings.json` | **Yes** (watched) | yes (user tier) | yes | High (read) | **No.** It is machine-wide and affects every chat and the person's own sessions. It is also the person's file. |
| Project or local `.claude/settings*.json` | **Yes** (watched) | **No.** Dropped under `strictAllowlist`. | yes, unless the path is under a denied read path | High (read) | **No.** It is shared by every chat in that folder and lives in the repo. The chat's own sandbox already denies writing it. |
| Claude Code's own network prompt (`sessionAllowedHosts`) | Yes, for the session | yes | n/a | Medium (read) | **No.** `strictAllowlist: true` denies an unlisted host outright instead of asking, so the prompt never appears. |
| `/add-dir` typed into the TUI | Yes (refreshes the sandbox) | n/a | yes (a working directory) | High (read) | **No.** purlis would be typing a user command into the chat, and it covers write paths only. |

Answer per key, for the TUI chat purlis runs today:

- **`sandbox.network.allowedDomains` (a new host): does not reload.** No source purlis
  can use for one chat is both watched and trusted for hosts.
- **`sandbox.filesystem.allowWrite` (a new write path): does not reload.** The only
  live, chat-scoped routes are `/add-dir` and the SDK's `register_repo_root`. Both grant a
  whole working directory, and neither is reachable without SDK control or typing into
  the TUI.

## Hooks: can PreToolUse change the next command's sandbox?

**No.** Read from the Bash tool's input schema and the hook output handling (high
confidence):

- `updatedInput` can set `dangerouslyDisableSandbox: true`. With
  `allowUnsandboxedCommands: false` that is refused, which is the fail-closed behaviour
  ADR 0067 wants.
- `updatedInput` can set the Bash tool's per-command `allowed_domains`. The tool applies
  that field only in auto mode, and it refuses it outright under `strictAllowlist` ("the
  configured sandbox allowlist is the whole allowlist here").
- No input field widens write paths.
- `permissionDecision` decides whether a tool call runs. It has no say in what sandbox the
  call runs under.
- A PermissionRequest hook's `updatedPermissions` can carry `addDirectories` with
  destination `session`. We did not confirm that this path refreshes the sandbox. Even if
  it does, it grants a working directory, not a host. So hooks give no idiomatic route.

## The fallback: restart and resume

The chat stops and starts again on the same conversation with
`claude --resume <id> --name <name>`. That is the argv purlis already builds in
`Harness::resume_argv` for reopen. The new `--settings` JSON carries the grant.
**Not measured live** (see above). What follows comes from the code and from purlis's
existing reopen behaviour.

What the person sees:

- The chat's terminal redraws.
- The resumed conversation renders from the transcript, and their messages and the
  agent's replies are all there.
- A pause of about one start-up. The start-up time is not measured here.

What is lost:

- **The turn in flight.** The restart has to wait for the turn to end, which it does once
  the blocked command has failed and the agent has answered, or else interrupt it.
- **Background Bash tasks and monitors.** Their processes end with the old process.
- **Session-only answers.** "Yes, don't ask again this session", directories added with
  `/add-dir`, and a permission mode switched mid-chat all go unless purlis passes them
  again.
- **MCP connections.** They reconnect, purlis's own server included.
- **Read-before-edit state.** Claude Code may ask the agent to read a file again before
  editing it. Not confirmed.

Prompt cache: the sandbox section of the Bash tool's description lists what is allowed.
Any grant changes that text, so the first request after a grant misses the cache whether
the grant arrives live or by restart. The restart adds no cache cost of its own.

## Recommendation for #1342

1. **For a Claude Code chat as purlis runs it today (a TUI in a terminal), apply a grant
   by restart and resume.**
   - `purlisd` holds the chat's grants and recompiles the inline `--settings` with them.
   - Once the chat's turn has ended, `purlisd` restarts the chat with
     `--resume <id> --name <name>`.
   - It then tells the chat the outcome, so the agent retries.

   This is Claude Code's own documented resume. The grant never sits in a file the chat
   could reach. And it can be observed: the Notice names the grant, and the restart is a
   run boundary under ADR 0066.
2. **Offer the two choices the spec names, "Allow for this chat" and "Allow in this
   project", and no "Allow once".** A grant cannot be scoped to one command without a
   second restart to take it away. "In this project" goes through the project flow, and
   the running chat takes it by the same restart.
3. **When a Claude Code chat runs over the SDK** (the ACP level, ADR 0080), send the
   recompiled whole `sandbox` object with `apply_flag_settings`. That route is live and
   needs no restart. Send the whole object because the merge is shallow. Re-check the
   merge with the `--settings` layer before relying on it.
4. **Do not use** a `--settings` file, user settings, project or local settings, Claude
   Code's network prompt, `/add-dir`, or a PreToolUse rewrite. The table above gives the
   reason for each.

Re-check this note when Claude Code's version moves: the pinned `--settings` file is a
recent hardening and may change again.

## Recipe to measure it (run outside an agent's sandbox)

1. Make a scratch project and a scratch settings file with the sandbox on:
   `{"sandbox":{"enabled":true,"failIfUnavailable":true,"allowUnsandboxedCommands":false,"network":{"allowedDomains":["example.com"],"strictAllowlist":true}}}`.
2. Start the chat with `claude --debug --settings <file>`. Run `curl -sI https://example.org`
   and `touch <outside-dir>/x`, and expect both to be refused.
3. Edit the file to add `example.org` and `filesystem.allowWrite: ["<outside-dir>"]`, wait
   3 seconds, and run both commands again. The expected result is that both are still
   refused, and the debug log shows no `Sandbox configuration updated` line.
4. Repeat steps 1 to 3 with the same keys in `~/.claude/settings.json` of a throwaway
   `CLAUDE_CONFIG_DIR`. Expect `Sandbox configuration updated from settings change`, and
   both commands to succeed.
5. Exit the chat, then run `claude --resume <id> --settings <file with the grant>`. Note
   the start-up time, check that the conversation is shown, check that both commands now
   succeed, and list what is gone.
