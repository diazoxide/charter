# Harnesses

You use Claude Code. A teammate uses opencode. CI runs Codex.

Same repos, same rules — or three sets of habits that drift until nobody knows which guard
is actually running. charter runs inside all three and enforces the same invariants in
each: the plane-root guard, the one-credential rule, the secret-leak check, and the
persona's declared tools.

```bash
charter harness list          # every harness, what it can't carry, and which one you're in
charter harness install codex # Codex only — see below
```

```
  claude-code
* opencode
      ↳ status-bar: no status-bar socket: opencode has no `statusLine` config …
          → charter statusline --watch
      ↳ prompt-hook: no per-turn prompt hook: charter's mid-session nudges ride …
      ↳ ask-decisions: no ask channel at tool time: opencode's `tool.execute.before` …
  codex
      ↳ status-bar: `tui.status_line` takes a list of built-in segments, not a command …
          → charter statusline --watch
      ↳ session-lock: `shell_environment_policy.set` holds constants, so no per-session …
      ↳ wiring-scope: no project-level config: hooks live only in `~/.codex/config.toml` …
```

The `*` is the harness this session is in, and those names are what `$CHARTER_HARNESS`
holds. A harness charter has no record of is reported too, as a warning rather than a clean
row — an unverified integration and a complete one must not read the same.

## What each harness lets charter offer

**What differs is not what charter enforces — it is what each harness lets charter
offer**, and `charter doctor` prints the gap rather than leaving you to find it:

| | how it is installed | how it updates | what it cannot carry | what to do about it |
| --- | --- | --- | --- | --- |
| Claude Code | the plugin (`claude plugin install charter@charter`) | `claude plugin update charter@charter` | — | — |
| opencode | `charter init` — one plugin under opencode's config dir, read by every project | charter moves it — its own file, compared byte for byte (`read_bytes`) with the one charter generates; anything else in that plugin directory is named too, and nothing charter did not write is ever overwritten | no status bar; no per-turn prompt hook; no ask at tool time; no per-workspace config; **no isolation from other plugins** | `charter statusline --watch`; mid-session notes ride tool output already; charter's own tool-time asks allow and are not shown — denials are unaffected; a second plugin in that directory shares charter's globals and can disable its guards, so `doctor` names it — charter reports the realm, it cannot contain it |
| Codex | the same plugin (`codex plugin`), plus `charter harness install codex` to name the harness | `codex plugin marketplace upgrade charter && codex plugin add charter@charter` | no status bar; no command-pattern permissions; no project-level config at all, so no per-workspace config | `charter statusline --watch`; `guard ask` rules stay in charter's own hook |

You never have to remember that third column — `charter update` asks the harness you are in
and names its command (or, for opencode, just moves it). It is written down because a
column charter fills in from one place is a column that cannot quietly go stale in three.

One artifact per harness, installed once — nothing is written into the repos you work in.
`charter doctor` and `charter harness list` print that last column against whichever
harness you are in, each ceiling carrying its own answer. Where it is empty it stays empty:
charter cannot conjure opencode a per-turn prompt hook, and a workaround that does not
exist costs more to chase than an honest gap.

## Wiring, and when it happens

`charter init` writes each harness's wiring into the plane. Nothing to install per harness,
with one exception (Codex, below).

**Plus one generated file per workspace, and only for Claude Code.** Claude Code reads
project settings from the session's working directory and does not walk up, so a chat
launched in `workspaces/<ws>/` — which is where the `+` and every workspace tab put it —
would otherwise get no plugin, no status line and no `$CHARTER_HARNESS`. Charter mirrors
the plane's `enabledPlugins`, `statusLine` and `env` into
`workspaces/<ws>/.claude/settings.json` at launch, records what it wrote in a
`.charter-generated` sidecar, and never touches a file whose hash it cannot vouch for.
`charter doctor`'s `workspace layer` row reports staleness; `charter workspace reinit` is
the repair.

Beside it, the **`session layer`** row answers the other half — *can a session started in
this directory see any of that?* Three artefacts, three discovery rules, all measured on
Claude Code 2.1.259: `.claude/settings.json` is read from the session's own directory with
no walk-up, `.claude/agents/` and `.claude/skills/` walk up but stop at the git root, and
`CLAUDE.md` walks up and is not git-bounded. So "charter is set up here" was never one
fact, and the row names which part is missing and which rule decided. The rules live on
each harness (`Harness.layer`), so a harness added to the registry is answered for the day
it is registered rather than reported under Claude Code's rules by default.

opencode and Codex get nothing here and say why: their config is machine-global, so
charter's layer is already live in every workspace and two workspaces on one machine
cannot be made to differ. That ceiling is in `charter harness list` beside the others.

Nothing is written into a **clone** — `workspaces/<ws>/<repo>/` is a repo charter does not
own, and `git add -A` there would stage it. So a session inside a clone gets charter from
the plugin, and **cannot delegate to a persona**: the plugin ships no `agents/`, and a
clone's own git root stops the walk-up that would have found the plane's.

## `charter statusline --watch`

The one worth knowing. It repaints the plane state in place in any spare terminal — no
status-bar socket, no multiplexer, the same render on every harness including the one that
has a bar. It shows the plane, not the session, so the token and context columns are blank
and it says so.

## The one exception: Codex

Codex needs the extra command for one reason: its hooks arrive with the plugin, but nothing
in a plugin can tell a shell which harness it is, so `charter harness install codex` writes
that single line.

If it finds hooks declared in `~/.codex/config.toml` it refuses and says so — those would
run alongside the plugin's, and charter would fire twice a turn.

Why the boundary sits where it does:
[ADR 0015](adr/0015-the-boundary-moves-with-the-harness.md).
