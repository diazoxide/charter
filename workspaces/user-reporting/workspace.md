# user-reporting

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

Let charter's own users report bugs and feature gaps back to the project as GitHub issues, from inside a running session. When a user's agent hits a charter bug or a missing capability, it can file it — automatically, or after the user confirms. Before filing, it searches existing issues so a known problem becomes a comment or a +1 rather than a duplicate.

## Context & decisions

<!-- Key facts, constraints, and design/architecture decisions found while working —
     the durable "why", not a chronological log. Grow this as you learn. -->

**Identity: the Reporter's own GitHub credentials.** No hosted service, no bot account.
charter already reaches GitHub through the Reporter's `gh` CLI
(`charter/forge/github.py` → `util.run([self.cli, "api", …])`, `check_auth` → `gh auth
status`); the vault backs the *git credential helper*, not the API. A hosted relay was
rejected outright: it would make the maintainer the operator of an anonymous write
endpoint into their own tracker, with the moderation burden that implies.

**Nothing publishes without a human "yes" in that moment.** Detection and drafting may
be automatic; the send never is. An upstream issue is outward-facing, attributed to a
real person, and awkward to retract — an unattended agent filing under the Reporter's
name is the kind of thing that gets charter uninstalled once. The automation worth
having relieves the Reporter of *writing* the report, not of *approving* it.

**Redaction is an allowlist, never a blocklist.** A session holds absolute paths with
the Reporter's username, private repo and org names, workspace and persona names, and
vault references. charter is sold on keeping secrets out of transcripts, so publishing
session context to a public tracker cuts against its own premise. Only explicitly
safe-listed fields travel, and the Reporter sees the exact payload before it sends. A
blocklist leaks eventually; an allowlist fails closed.

**A duplicate is worth a comment, not a drop.** Search always runs first, but silently
discarding a match throws away the most valuable thing a second report carries: another
environment where the bug reproduces. Default to commenting on the match.

**Triggers ship in order: manual → crash → gap.** Manual has no detection problem and
validates the whole filing/dedupe/redaction path on its own. Crash detection is the
clearest signal (a traceback is unambiguous). Agent-noticed gaps are the fuzziest and
noisiest, and should wait until real reports show what good looks like.

### Settled design

**Two commands, because the second one *is* the consent.** `charter report bug|gap`
drafts, redacts, prints the exact payload and returns an id; `charter report send <id>`
publishes. charter has no interactive prompt anywhere — `util.py` carries only
`info/ok/warn/err`, deliberately, because it runs inside hooks and agent sessions where
blocking on stdin would hang. So consent cannot be a y/n prompt. The two-command split
turns that constraint into the feature: the agent drafts, shows the Reporter the payload
in conversation, and can only publish after the Reporter says yes. A `--yes` flag was
rejected — a flag the agent can pass is a flag the agent will pass unprompted.

**Bug and gap are separate subcommands sharing only transport.** Their payloads, dedupe
strategies and noise profiles all differ, and two names force the reporting agent to
*declare which it is* rather than letting one vague verb launder a speculative gap into
the same channel as a hard crash.

**Detection defaults on; filing needs one-time consent, stored per-human.** Detection
only writes to the Reporter's own disk, so default-on costs them nothing and means the
report already exists when they're asked. Consent to publish under your own GitHub
identity is a property of the person, not of a directory — so it lives in user-level
config, never in `charter.toml`, which is committed and would enrol a whole team on one
person's say-so.

**The capability is advertised at the moment of failure, nowhere else.** charter's own
error output ends with a pointer to `charter report`. This costs zero prompt real estate
— and the SessionStart injection is already crowded and explicitly bounded
(`charter/hooks.py`, `_memory_digest` is "a BOUNDED digest, not the whole index") — while
being perfectly targeted: the prompt appears exactly when a bug just happened. A session
-context line would tax every session forever for a rare event; a plugin skill is net-new
surface to do what a printed string does.

**An unknown subcommand is the gap signal.** Because the capability only surfaces on
error, a feature gap — which prints nothing — would otherwise have no delivery
mechanism at all. But argparse's invalid-choice error *is* a mechanical expression of a
missing capability: someone typing a command charter doesn't have has told us exactly
that. Anything vaguer falls back to the human asking. Implementation note: `parse_args`
runs *above* the `try` in `cli.py:main`, so this needs an `ArgumentParser.error` override
or a `SystemExit` catch.

**Filing is a flat `gh` call in the reporting module, not a `Forge` method.** The target
is always one specific repo on github.com, so it isn't polymorphic over forges and does
not belong in the protocol that abstracts *the Reporter's* forges — putting
`create_issue` on `Forge` would imply GitLab needs an implementation it will never need.
It also must not reuse `_api`, whose contract is "best-effort GET, returns `None` on any
failure" so the status line can never crash; applied to filing, that means the report
vanishes while the Reporter is told nothing.

**The target repo is configurable, not hardcoded** — so a fork can point reports at its
own tracker and an internal deployment can keep them private. It doubles as the
development affordance, alongside a dry-run that prints the `gh` invocation without
running it.

**Reports land in the same tracker under a `via-charter-report` label**, plus `bug`/`gap`.
A separate repo splits the conversation and doubles triage for a project this young. The
label is what earns its keep: it separates machine-drafted from hand-written reports, and
it is how you measure whether this feature works at all.

**Stale versions warn but never block.** `update.newer_than()` already knows. A bug that
survives on an old version is still worth having, and "update first, then report" is a
reliable way to never get the report — so put the version gap in the payload and let the
maintainer close it in five seconds.

**Local state collapses by fingerprint and outlives the send.** Same fingerprint bumps an
occurrence counter rather than writing a new file, which caps a crash loop and turns
repetition into signal ("hit 47 times"). A filed report is kept, stamped with its upstream
issue URL, so the next identical crash points at the existing issue instead of re-drafting
— local dedupe at zero API cost. Unsent drafts age out, matching the existing
`charter persona _gc` pattern.

### Constraints found in the code

- **The forge layer is read-only today.** `charter/forge/base.py` exposes `check_auth`,
  `list_repos`, `repo_tree`, `open_change`, `ci_status` — no issue API at all. Both
  filing and searching are net-new surface, and filing is charter's first forge *write*.
- **No hook observes charter failing** — `hooks/hooks.json` matches `PostToolUse` on
  `Write|Edit|MultiEdit` and `Task|Agent` only, and `PreToolUse` runs *before* the
  command. Hence detection attaches inside `cli.py:main` instead: charter is the only
  thing that reliably observes its own crash, holding the exception, subcommand and
  version already, where a hook would reconstruct all three from a string. This also
  keeps the feature working for CLI-only installs with no plugin.
- **`cli.py:main` already draws the line the reporter needs.** It catches exactly
  `KeyboardInterrupt` and `util.ProcTimeout`, the latter with the comment *"A child that
  outlived its budget is a condition, not a bug."* So: conditions never generate a
  report, uncaught exceptions always do. The rule comes for free from code that predates
  this feature.
- **Some Reporters have no GitHub at all.** charter supports GitLab forges, so a
  GitLab-only Reporter has no `gh` identity to file under — hence the prefilled
  `issues/new?…` URL fallback, which needs no service and reuses the existing
  `util.urlenc`. It doubles as the escape hatch when `gh` is broken or rate-limited.
- **`.github/` holds only `workflows/`** — no issue templates exist. An
  `ISSUE_TEMPLATE/` form is the wrong tool regardless, since charter composes the body
  itself and only sends a human to the web form on the fallback path.

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

- `Reporter` — the **human** developer who installed charter and in whose session the
  problem surfaced. The one who confirms the send and whose GitHub identity it goes
  under. Distinct from the agent: "user" was doing both jobs and had to be split.
- `reporting agent` — the Claude Code agent running in the Reporter's session, which
  detects the problem, drafts the report, and judges duplicate candidates. It never
  publishes on its own.
- `report` — the **private draft** of a bug or gap, held locally. A report is not yet
  public and may never become public.
- `upstream issue` — the public artifact at `diazoxide/charter`. Qualified deliberately:
  charter also works with issues in the *Reporter's own* repos, so a bare "issue" is
  ambiguous in this codebase.
- `upstream` — `diazoxide/charter` itself, as opposed to the repos a Reporter uses
  charter to work on.
- `bug` — charter did something wrong: an uncaught exception, a wrong result. Structured,
  fingerprintable, mechanically redactable.
- `gap` — charter can't do something it should. Free prose, no fingerprint, no mechanical
  dedupe, and a far worse noise profile — which is why it's a separate subcommand.
- `condition` — a failure that is **not** a bug: a timeout, an interrupt, a child process
  refusing to answer. charter's own vocabulary, from the comment on `util.ProcTimeout` in
  `cli.py:main` — "a condition, not a bug". Conditions never produce a report. The
  distinction predates this work and is now load-bearing for it.
- `fingerprint` — exception type + charter stack frame, identifying a repeat of the same
  `bug`. Used three ways: collapsing a crash loop into one report with a counter, matching
  an already-filed report locally, and as the free fast path before agent-judged dedupe.
- `send` — the second, separate command that publishes a `report`. Named as its own step
  because it is the point where consent is expressed and the only point that touches the
  network.

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
