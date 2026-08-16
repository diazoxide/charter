# Grilled — the fallout from 0.30.0's plane-root guard

**Status:** decided with the operator, then built. Against `main` @ 0.30.0, issues #167 and #168.

Both issues are consequences of #157, which shipped hours earlier in the same session that
now had to answer for it. That is the useful thing about them: they are not a backlog, they
are a design under load.

Two facts, established by scouting before any question was put, because either one would
have produced a confident brief for the wrong job:

- **charter cannot open pull requests.** No PR creation exists in any forge adapter, so any
  option phrased as "open a PR" is a new capability, not an addition.
- **`_ensure_statusline` is a worn precedent** for touching `.claude/settings.json`: one key,
  only if absent, never repairs a malformed file, reports which of those happened.

---

## Round 1 — the frontier

**Q1. Does charter support a plane whose own repo is PR-gated?** → **Yes, and that is the
actual bug under #167.** charter says control-plane content is committed with `charter save`,
directly to the default branch. Where a direct push cannot land, the only route is to branch
the plane root — which 0.30.0 now refuses. The workspace-clone advice does not apply: clones
are for product repos, not the plane's own personas, ADRs and `charter.toml`. #167 is the
first symptom of charter having no sanctioned path there at all.

**Q2. Was #157's command set too narrow?** → **The command set is right; the carve-out set was
incomplete.** Denying branch moves still matches the evidence — two agents sharing one HEAD.
What was missing was a sanctioned way to do the legitimate thing, and the answer is a
charter-mediated path rather than a `--force`-shaped hole: an escape hatch an agent can pass
is one an agent will pass.

**Q3. Should the check report packaging or whether the guard fires?** → **The guard.** Whether
charter runs as a plugin is an implementation detail. `check_plugin_skew` keeps its own row,
answering its own separate question about version skew.

**Q4. May charter write hook wiring into `.claude/settings.json`?** → **Yes.** The precedent
settles it rather than contradicting it: `init` already writes `statusLine` into that exact
file. The stance was never "don't touch the file", it was "don't touch keys that aren't yours".

**Q5. Is green-when-absent a class worth auditing?** → **Fix these two now; audit separately.**
Filed as #171. An audit riding along with a bugfix gets the least attention in the PR.

## Round 2 — the mechanics

**Q6. What is the sanctioned path on a PR-gated plane?** → **`save` lands the change on a
branch and hands back a compare URL.** Chosen over "detect and instruct" (which leaves the
work) and over teaching the adapters to open PRs (a new capability in every one). A compare
URL is a plain HTTPS link, so this needs no API.

**Q7. What counts as "the guard is reachable"?** → **Any of four routes** — the plugin, the
plane's `settings.json`, its `settings.local.json`, the user's `~/.claude/settings.json` —
and it asserts the `pretooluse` handler specifically. A plane wiring only `sessionstart` is
unprotected while looking configured, which is #168 one level down.

**Q8. What exactly gets auto-wired?** → **`pretooluse` only.** If the plugin is installed
later, everything wired here fires twice: harmless for a denial, but `sessionstart` would
render the persona briefing, memory digest and todo list twice every session.

---

## One deviation, found in the building

Q6 was agreed as a **throwaway worktree**: create one, commit there, push, remove it. That
was machinery for a problem that does not exist. `git push HEAD:refs/heads/<new>` needs no
checkout, no branch creation and no second working tree — only a different *remote* ref for a
commit that already exists locally.

Same guarantee, less code, and the property that mattered is unchanged and now asserted
directly: **the plane root's HEAD never moves.** Recorded here because the agreed design and
the shipped one differ, and a reader comparing them deserves the reason rather than a
discrepancy.

## What was deliberately not built

`version bump --print`. It was one of #167's suggested closers, and Q6's answer makes it
unnecessary — the change can now be landed rather than merely emitted. Adding surface that
nothing needs has its own cost.
