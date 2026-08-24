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
| opencode | `charter init` — one plugin under opencode's config dir, read by every project | charter moves it — its own file, stamped | no status bar; no per-turn prompt hook; no ask at tool time | `charter statusline --watch`; mid-session notes ride tool output already; charter's own tool-time asks allow and are not shown — denials are unaffected |
| Codex | the same plugin (`codex plugin`), plus `charter harness install codex` to name the harness | `codex plugin marketplace upgrade charter && codex plugin add charter@charter` | no status bar; no command-pattern permissions | `charter statusline --watch`; `guard ask` rules stay in charter's own hook |

You never have to remember that third column — `charter update` asks the harness you are in
and names its command (or, for opencode, just moves it). It is written down because a
column charter fills in from one place is a column that cannot quietly go stale in three.

One artifact per harness, installed once — nothing is written into the repos you work in.
`charter doctor` and `charter harness list` print that last column against whichever
harness you are in, each ceiling carrying its own answer. Where it is empty it stays empty:
charter cannot conjure opencode a per-turn prompt hook, and a workaround that does not
exist costs more to chase than an honest gap.

## Wiring, and when it happens

`charter init` writes each harness's wiring into the plane, and `charter clone` /
`charter worktree add` arm every tree as it is created, because a session starts in a clone
and not in the plane root. Nothing to install per harness, with one exception.

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
