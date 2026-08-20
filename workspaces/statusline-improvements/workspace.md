# statusline-improvements

> **Living charter** for this workspace — its north star and shared context.
> Keep it current as the work evolves (edit this file, or `charter workspace vision "…"`).
> It's committed + shared for LIVE workspaces, and a fork inherits it — so anyone
> can pick up the task with full context. Never put secrets here (vault only).

## Vision

Improve the charter status line's UX — how the plane's state reads at a glance: what the line shows, in what order, and how it renders.

## Context & decisions

<!-- Key facts, constraints, and design/architecture decisions found while working —
     the durable "why", not a chronological log. Grow this as you learn. -->

### Scope: three asks, two shipping here

1. **Vault state on the persona rows** — ship here.
2. **Running-persona indicator on the persona rows** — ship here.
3. **Render cadence / animation** — *deferred by decision*, see "Cadence" below.

### Facts found while scoping (2026-08-20)

- The persona **chips** (`statusline._persona_chips`, right column) are the real surface.
  `statusline._persona_line()` is the **fallback** renderer — it draws only when column
  layout fails, so work there is invisible in normal use.
- A vault dot already exists (`statusline._vault_dot`) with four states. The dishonest one:
  a persona declaring `vault: X` where `X` is not registered on this machine renders as
  dim `·` — identical to a persona that needs no vault at all.
- Running personas are already tracked. `inflight.live()` returns agent names with
  duplicates preserved, plane-wide (state dir, not session-scoped). It renders only on the
  session strip as `⚡in flight 2 · devops, devops`.
- **The status line never touches the forge.** `glstate.read_for()` reads a disk cache;
  `glstate.maybe_spawn()` kicks a *detached* `charter gl-refresh` at most once per 120s
  when entries exceed a 300s TTL. Zero network on the render path, by design.
- The 10s cadence is `refreshInterval: 10` in `.claude/settings.json`, justified in
  `commands.py:693` and enforced by `tests/test_statusline_refresh.py` (`5 ≤ n ≤ 30`).
  A render is ~190ms wall (~50ms of it Python startup); 1Hz costs ~20% of a core forever.
- Glyphs already spoken for on a chip: `⚑` yellow = draft charter, `✗` red = broken config
  (`_health_mark`). `×` is East-Asian *Ambiguous* width — this layout has been broken twice
  by character width, so it is banned.

### Decisions

- **Vault: silence means fine.** Three states, two glyphs: connected → render *nothing*;
  required but not usable → dim `◦`; registered but broken → yellow `!`. "Not registered
  here" and "registered, file missing" collapse into one `◦` — a chip cannot carry the fix
  and the two fixes differ; `charter persona list` already spells both out in words.
  This applies charter's existing rule (`_health_mark`, `_session_news`): a row of ✓s is
  furniture within a day, and then a real fault inside it reads like a zero.
- **Running: the count lives next to what it counts.** Chip reads `▸ devops ⚡2 4m` —
  count only when >1, age always, reusing `pieces._presence_age`'s vocabulary (`4m`, `2h`,
  `3d`) so it ages in the same units as `silent 12m` two rows away. At a 10s refresh,
  seconds would be a lie.
- **"Running" = charter-dispatched sub-agents, plane-wide.** Not "adopted in another live
  session" — that is session presence wearing a statusline costume and needs its own store.
- **The session strip keeps a bare `⚡ 2`**, names dropped. The persona column truncates at
  14 chips and vanishes on a narrow pane, so the aggregate is what survives cropping —
  the same contract `_repo_rows` keeps with its "(+N more)" line.
- **No colour threshold for a long-running dispatch.** The number is the signal. The
  related real bug — `inflight` prunes at 30 min, so a *stuck* agent disappears rather than
  escalating — was filed upstream as diazoxide/charter#308 and deliberately not fixed here:
  it changes `inflight` semantics, not what the line displays.

### Cadence (deferred — re-grill after the display changes ship)

The premise "we cannot render faster because of third-party forge calls" is **false** — see
the facts above. The real blocker on 1Hz is the render's own cost plus Python startup, and
Claude Code's floor is 1 second, so the best achievable "animation" is one frame per second:
a blinking light, not a spinner. The question a human asks the line is *"has this been stuck
for 4 minutes?"*, which the elapsed age answers and a spinner does not.

If faster cadence is still wanted afterwards, the only honest build is: a background renderer
writes the finished line to a file and the statusLine command becomes a near-instant read
(still ~30-50ms of Python startup unless the command stops being Python). That is a separate
workspace-sized change to *how the line is produced*, not to what it says.

## Glossary

<!-- Task/domain vocabulary so a teammate or a fork isn't lost: `term` — definition. -->

_Nothing yet._

## Log

Chronological "what was done" lives in the task memo — `memory/notes.md`
(append with `charter workspace note "…"`).
