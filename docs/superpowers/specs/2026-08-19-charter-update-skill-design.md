# `charter update` — one command that moves charter, then tells you what moved

_2026-08-19 · steward · workspace `charter-update-skill`_

## The ask

A user says "update charter to the latest version" and the harness does it: upgrade what
can be upgraded, then say what the new version brings, offer to adopt it where adoption is
mechanical, and write a guide where it is not.

## What already exists

charter is not starting from nothing here, and the design is mostly about not duplicating
what is already right:

- `charter version` — installed / locked / latest, and drift as a non-zero exit.
- `charter version sync [--cli]` — conform this plane (plugin) or this machine (binary).
- `charter version bump [--to] [--push]` — install → verify → write the pin → commit.
- `charter/update.py` — the cached, non-blocking PyPI check the status line rides on,
  plus `plugin_version_here()` and `SHARED_INSTALL_NOTE`.

What does **not** exist: anything that tells a harness to use them, and any notion of
"here is what this version brings *you*".

## Decisions

### 1. The CLI decides and acts; the skill converses

Deterministic behaviour lands in the CLI, where it is tested and where every harness
reaches it. The skill carries only dialogue: which question to ask, in what order, and what
never to do without a yes.

This is not a style preference. `skills/` ships in the Claude Code plugin; `charter/`
ships on PyPI to every harness. Behaviour placed in the skill is behaviour opencode does
not get. Therefore **`charter update`'s output must be self-sufficient** — an agent with no
skill at all, reading only the command's output, must be able to finish the job. The skill
is an accelerator, never the carrier.

### 1b. The upgrade path is the installer that owns the install

`sync_to()` shells out to `uv tool install` and errors when uv is absent — but
`docs/install.md:15` documents pipx and pip as supported fallbacks, so "maximum automatic"
would die at the first move for anyone who took a documented path.

`charter update` detects which tool owns the running install (`sys.prefix` /
`sys.executable` — a `uv/tools/` path, a `pipx/venvs/` path, or neither) and uses the
matching command. **Where detection is ambiguous, it names the command instead of guessing**
— the same restraint it keeps for the plugin.

Two places it refuses or degrades rather than proceeding:

- **Inside a charter checkout** (`doctor._is_charter_checkout`) it refuses. `CONTRIBUTING.md`
  tells contributors to run `python3 -m charter …` from the clone; installing over that is
  never what "test the update command" meant, and the recovery is annoying. It names
  `charter version` as the read-only thing they probably wanted.
- **Outside a control plane** it runs degraded: the CLI moves, the pin is skipped (there is
  nothing to conform to), and the news *range* prints without probes — "no control plane
  here, so charter cannot tell which of these you have adopted", pointing at
  `charter news --pending` from inside one.

### 2. There is no single "the plugin" — ask the harness

Each harness relates differently to the artifact that serves it:

| harness | artifact | how it moves |
| --- | --- | --- |
| Claude Code | the plugin, host-installed into a versioned cache | `claude plugin update charter@charter` — charter *names* it, will not run it |
| Codex | the same plugin artifact, via `codex plugin` | **not pinned** — see below |
| opencode | a shim charter writes itself, version-stamped | charter rewrites it — it is charter's own file |

So the `Harness` contract gains one member:

```python
def upgrade(self, root: Path) -> tuple[str, str]:
    """Move THIS harness's installed charter artifact to the running CLI's version.
    ("moved", detail) | ("current", version) | ("manual", command) | ("absent", why)
    """
```

Registering a harness then covers `update` the day it is registered — the stated reason
`registry.KINDS` exists.

**Codex's own config block never needs moving.** `_block()` (`codex.py:62`) writes only
`shell_environment_policy` to name the harness; the hooks come from the plugin. So Codex's
artifact is the plugin, and the command that moves it is a fact charter has not pinned.
`upgrade()` therefore returns `("absent", "charter has not pinned how a Codex plugin
updates")` rather than a plausible-looking `codex plugin update`. `base.py` is explicit that
"an empty remedy is a claim too — inventing one sends somebody off to configure something
that does not exist", and `codex.py`'s entire discipline is facts pinned against the binary
rather than its documentation. Pinning it is a follow-up errand, not a blocker: Codex users
are no worse off than today, where nothing tells them anything.

While in that file: `_WIRING` (`codex.py:44`) is dead — defined, referenced nowhere, left
from when charter wrote Codex's hooks itself. It goes with this change.

**opencode's `upgrade()` calls the existing writer.** `refresh_shim()` (`opencode.py:227`)
already re-stamps the shim and `wire()` already calls it; `upgrade()` reports what it
returns and learns nothing about the file's shape. Two code paths answering one question
must call one function.

**Charter moves what it authored, and names what it does not.** opencode's shim is
charter's own file, stamped by charter, already rewritten by `init`/`reinit`; moving it is
not a new liberty. The plugin belongs to the host, and the existing restraint stays exactly
where its comment says it was earned.

### 3. This fixes a defect rather than building beside it

`cmd_version_sync` prints `PLUGIN_SYNC_CMD` — the literal `claude plugin update
charter@charter` — unconditionally (`commands.py:1886`). Under opencode,
`plugin_version_here()` reads `$CLAUDE_PLUGIN_ROOT`, gets `None`, and the user is told to
run a Claude Code command that has nothing to do with their install.

Two harness-blind code paths already answer "is the artifact current?" without knowing
about each other: `update.plugin_version_here()` (Claude Code) and `Harness.stale_wiring()`
(opencode). `Harness.upgrade()` makes it one question with one answer, and the defect
disappears as a consequence rather than as a patch. `version sync` and `charter update`
both route through it. The issue is filed for the record; the fix lands here, first.

### 4. "What's new" ships inside the wheel

A per-item entry file, force-included the way `docs/*.md` already is. It works offline, on
any plane, with no forge auth — and it is the same text CI publishes as the GitHub Release
body, so the two cannot drift.

**Entries are written by the feature's own PR, not reconstructed at release time.** The
person who built the thing is the only one who knows why it matters and what probe proves
adoption; rebuilding that from commit titles at bump time is how release notes become a
changelog nobody reads.

That means a PR must name a version that does not exist yet — so it does not. Entries are
staged as `docs/news/unreleased-<slug>.md` with `version: unreleased`, and the bump PR runs
**`charter news stamp <version>`**, which renames each and stamps its frontmatter. This
removes the only way an entry can be *silently wrong*: a real version number that was never
true. `charter news` treats `unreleased` as not shipped, so a staged entry never surfaces to
a user.

No CI check blocks a feature PR that ships no entry — most PRs are refactors, and a required
entry per PR manufactures filler, which is the failure this design exists to avoid. The PR
template prompts; the bump PR's checklist is "read the merged PRs since the last tag, every
user-visible one has an entry"; the gate bites where being wrong is most expensive.

`docs/news/<version>-<slug>.md`, one file per item:

```markdown
---
version: 0.44.0
headline: A persona says when it should hand work away
check: persona lint --only delegate-when
adopt: persona sync-agents
---

Every persona now declares `delegate-when:` — the sentence that becomes its sub-agent
description, so routing stops being a guess. Personas written before 0.44 have no such
line and are invisible to the router until they get one.
```

- **Flat frontmatter, parsed by `persona.parse`.** charter has no dependencies, so there is
  no YAML to nest with, and the repo teaches one frontmatter idiom. One file per *item*
  rather than per version follows from that: a flat parser cannot express several adopt
  blocks in one file.
- **Force-included as a directory** — `"docs/news" = "charter/_news"`. `docs/*.md` is listed
  page by page to keep `adr/`, `audits/` and `superpowers/` out of site-packages; news has
  no such exclusion, so a directory mapping removes a per-release `pyproject.toml` edit.
- **`check:` and `adopt:` are charter subcommands only** — parsed as argv, **dispatched
  in-process** through the CLI parser with output captured, never spawned through a shell.
  In-process is both faster (`doctor` may run a dozen) and makes "charter subcommands only" a
  *structural* guarantee rather than a rule a validator enforces: there is no shell and no
  argv to escape into. The exit code is the entire contract; stdout/stderr are shown only
  when a probe reports `unknown`. Same restraint `docsrc._TOPIC` keeps for `docs show`.
- **`check:` exit 0 means adopted here**, non-zero means pending. Omitting it makes the
  entry informational. An entry with no *narrow* probe available ships without one rather
  than with a sloppy one — `charter persona lint` reports many findings under one exit code,
  so probes will need narrow flags (`persona lint --only <key>` is the likely first). That
  is work to do, not an assumption to make.
- **A probe that cannot run is `unknown`, never `pending`.** A `check:` naming a
  subcommand this CLI does not have (an entry read by an older binary, a flag not yet
  added) exits non-zero for a reason that has nothing to do with adoption. Charter reports
  it as unchecked and says why, the way `doctor` refuses to paint a green glyph over a
  check that did not run (`_NOT_CHECKED_HINT`). Silence is not evidence of health.
- **`adopt:` present means automatable.** Absent means the body *is* the guide, and the
  skill turns that prose into steps.

Three views:

| command | answers |
| --- | --- |
| `charter news` | what changed between your baseline and now — the post-upgrade report |
| `charter news --pending` | every entry, any version, whose probe says you have not adopted it |
| `charter news --for <v>` | one version's entries — what CI turns into the Release body |

`--pending` is why no dismissal state is needed: the range view moves past a declined
suggestion on the next update, and `--pending` exists for when you want to be asked again.

**The baseline** — the version a range is measured from — is written under `STATE_DIR`
(per-developer, gitignored, beside the update cache it is read with). It is not committed:
which version *this laptop* last updated from is not a fact about the plane, and ADR 0011
keeps the record to what git cannot know.

**With no baseline recorded** — first run after adopting this feature — there is no range,
and charter says so rather than inventing one. It prints the running version's entries and
points at `--pending` for everything else. It must not silently replay every entry ever
written as though it were news.

**Where entries are read from** mirrors `docsrc`: the packaged `charter/_news/` wins, the
repo's `docs/news/` is the checkout fallback. This is what lets CI generate the Release
body with `python -m charter news --for <v>` from the tree it just tagged.

**Backfill is selective.** Forty-four versions of reconstructed prose is exactly the
low-value text this design avoids. Entries go back only where there is a *probe worth
running* — a handful of adoptable items from 0.4x, `delegate-when` and the front door among
them — so `--pending` earns its keep on day one instead of being empty until the next
release.

**Where pending adoptions surface.** `charter update` reports them, and `charter doctor`
gains one row: a count plus "run `charter news --pending`". The session-start hook stays out
of it — probes are real work, and N of them on every session start is precisely the cost
`update.py` exists to keep off the status line's clock. The status line keeps its bare
`↑<version>` (`statusline.py:1546`); the session-start nudge is where the command gets
named, and that change belongs to the `statusline` persona.

**Output is prose, not JSON.** One stable line per pending item
(`slug · headline · adopt: <cmd>` or `adopt: manual`). A `--json` mode would immediately
become the thing the skill parses and the prose the thing nobody checks — and the output has
to be self-sufficient for an opencode agent with no skill at all (decision 1). One surface,
kept readable.

**"News" enters charter's vocabulary deliberately.** `CONTEXT.md`'s Language section is a
glossary with `_Avoid_` lists, and this design adds a noun to it:

> **News**: a shipped, per-item note that a version introduced something, carrying an
> optional probe for whether this plane has adopted it. Not a changelog — an entry exists to
> be *acted on*, and one with nothing to adopt is one line.
> _Avoid_: changelog, release notes, announcement

### 5. The order `charter update` runs in

```
1. resolve target      live PyPI fetch (an explicit command, not the status line's cached
                       read); fall back to cache when offline and say it is a fallback
2. stamp baseline      record the version in use NOW, before anything moves
3. move the CLI        via the installer that owns this install (uv / pipx / pip),
                       SHARED_INSTALL_NOTE printed BEFORE the install
4. move the artifact   harness.upgrade(root)
5. hand off            re-invoke the newly installed binary for the news phase
6. news + probes       entries baseline→target; run each check:; print only what is pending
7. the pin             proposed, never written
```

**CLI before the artifact.** Plugin-newer-than-CLI is the one direction that breaks — the
plugin dispatches `charter hook <name>` for handlers an older CLI does not have, and
`skew_message` is one-directional, so it stays quiet the other way. The release charter
already says this; `update` obeys the same order for the same reason.

**Step 5 is load-bearing.** The process running steps 1–4 is still the old build —
`cmd_version_sync` says so already (`commands.py:1904`). The new version's entries and
probes exist only in the new wheel, so the news phase must run in a fresh process of the
installed binary. If charter cannot locate or launch it, it names the exact command instead
of reporting a phase it did not run (ADR 0009, ADR 0013).

**Step 2 before step 3** so an interrupted update still knows where it started.

**The handoff is also the verification.** If the new binary cannot run, or reports a version
other than the target, `update` says the install did not take and names the manual command
— rather than printing news and implying success. It costs nothing, because that subprocess
has to happen anyway, and it closes the window the release charter warns about: an upgrade
that "succeeds against a cached index and leaves you on the old version reporting success".

**Stale plane wiring is named, never repaired.** A new version may want wiring the old one
never wrote. `update` finishes by reporting each harness's `stale_wiring()` and pointing at
`charter reinit`. Running it would make `update` write into the plane as a side effect of a
version change — a different act from moving a binary, and the kind of quiet scope creep
that makes a command people run casually stop being trusted.

### 6. The pin decides the target, and only one case needs a human

Moving the machine past a pin manufactures the drift `charter version` reports as an error.

| this plane | target | unattended? |
| --- | --- | --- |
| pins nothing | latest | yes — end to end |
| pins a version you are behind | the pin | yes — conforming to an existing pin affects nobody |
| pin == installed, latest is newer | latest | no — moving means moving the team; proposes `charter update --bump` |

`charter update` becomes the front door that subsumes `version sync` without replacing it,
and it is **idempotent**: run it when already current and it goes straight to the news
phase. That is what makes it safe for the skill to call unconditionally.

Flags: `--to <version>` (explicit target) and `--bump`. There is no separate `--push`:
`--bump` writes the pin and lands it through `charter save`, which already knows to open a
pull request on a plane whose repo requires one (#167). Splitting them would recreate the
trap where the pin is written, nothing is shared, and the plane reports drift against a lock
only its author can see. No dry-run flag — `charter version` is already the read-only view.

**`charter version sync` survives, undeprecated.** It answers a narrower question — conform
me to the lock, no PyPI call, no news phase — which is what a CI job or an offline machine
wants. `update` gets the discoverability; `sync` keeps the precision. Both route through
`Harness.upgrade()`, which is the point.

**Outside a harness** — a plain terminal, where `harness.current()` is `None` and both
opencode and Codex `detect()` return `False` by design — `update` moves the CLI and then says
the harness artifact was **not checked**, pointing at `charter harness list`. Never "nothing
to do": absence of information is not evidence of health.

### 7. A release without notes does not publish

Two catches, deliberately at different times:

- `tests/test_news.py` asserts an entry exists whose `version:` equals
  `charter.__version__`. This fails **in the bump PR**, before any tag exists.
- `release.yml`'s `guard` job re-asserts it against the tag — covering a tag that skipped
  the PR, and failing **before** the irreversible publish.

**Every published version needs an entry, including patches** — and a patch's entry may be
one line with no `check:`/`adopt:`. A loose gate is worse than none here: "no entry" and
"forgot the entry" are indistinguishable from CI, and 0.44.1 shipping silently is what
trains people to skip it.

The bump PR's sequence gains one mechanical step, `charter news stamp <version>`, beside the
four version files it already moves. That file's own warning — "never work from a remembered
count" — is why this is a tested command rather than a fifth thing to remember; CI's guard
then checks the result rather than the intent.

After `publish` succeeds, a new job runs `gh release create v<X.Y.Z>` with the body from
`charter news --for <v>`. The shipped entry is the single source for both the offline
suggestion and the public notes. That job carries `contents: write` on its own; the
workflow's top-level `permissions: contents: read` stays. It must tolerate the
`workflow_dispatch` retry path, where the release may already exist.

This is **not** a fifth file for `TestVersionsMoveInLockstep`, which pins four files that
carry a version *number*. "A release has notes" is a different obligation and gets its own
test rather than being bolted onto one that means something else.

### 8. The skill

`skills/update/SKILL.md` — triggered by "update charter", "is there a new charter",
"what's new in charter":

1. Run `charter update`. Read its output; do not re-derive what it reported.
2. Walk pending adoptions **one at a time**. `adopt:` present → explain, ask, run on a yes.
   Absent → turn the entry body into a step-by-step guide.
3. Never move the pin without an explicit yes. Never run a host plugin command charter
   deliberately declined to run.
4. The skill driving the upgrade is the **old** version's copy; the new skill text arrives
   with the plugin, next session.

**No persona declares this skill in `skills:`.** Declaring preloads the full text on every
dispatch of that persona — `skilluse.py` says it outright: "cheap to write and expensive to
keep". Updating charter is user-triggered, not something a persona does mid-task, and the
steward would pay that cost on every dispatch for a skill used a few times a year.

**Named `update`**, invoked as `charter:update`. The collision with this harness's built-in
`update-config` is cosmetic, and any longer name reads worse beside `persona`, `secrets`,
`browser` and `working-in-a-clone` — each the plain noun for what it is about. The
description does the disambiguating.

Skills reach Claude Code, and Codex insofar as it installs the same plugin artifact.
opencode gets none — and charter's existing four skills are already Claude-Code-only with
no `Deficit` recording it. Verify what opencode and Codex actually carry, then declare the
gap where it is real. An unverified integration and a complete one must not read the same.

## Proof

| piece | test |
| --- | --- |
| `Harness.upgrade()` | every registered harness answers it; Claude Code → `("manual", PLUGIN_SYNC_CMD)`; opencode → `refresh_shim` moves and re-stamps; Codex → `("absent", …)`, never a guessed command; **`version sync` under opencode no longer prints a Claude Code command** |
| installer detection | uv / pipx / pip each resolve to the right command; ambiguous resolves to *named, not run* |
| news parsing | flat frontmatter via `persona.parse`; ordering via `update._parse` (the same comparator, not a second one); `check:`/`adopt:` reject anything that is not charter argv |
| packaging | `docs/news/` readable from the packaged path — the `test_docs_show.py` pattern |
| `charter update` | the three pin cases; baseline stamped before anything moves; handoff names the command when it cannot launch the new binary |
| entry validity | every shipped `check:`/`adopt:` resolves against the **live argparse parser** — not run, just proved to parse, so a removed flag fails the suite of the PR that removed it |
| release gate | an entry exists for `__version__`; the guard fails a tag with no entry; `news stamp` renames and stamps every staged entry |
| degraded paths | refuses inside a charter checkout; outside a harness and outside a plane it reports **not checked**, never a clean bill |

## Order of work

Five independently landable changes, each green on its own:

1. `Harness.upgrade()` + route `version sync` through it — fixes the defect, adds no
   surface. Deletes dead `_WIRING`. **steward**
2. `charter/news.py` + `charter news` + `CONTEXT.md` glossary + the backfilled entries.
   **steward** — independent of step 1, so the two can land in parallel.
3. `charter update` (+ the `doctor` row). Depends on 1 and 2. **steward**
4. Release gate: `release.yml`, `charter news stamp`, the release charter, `CONTRIBUTING.md`
   and the PR template. Depends on 2. **release persona** — it changes a workflow that
   publishes irreversibly, which is precisely what that persona exists to get right.
5. The skill. Depends on 3. **steward**, written with `mattpocock-skills:writing-for-agents`.

Alongside, not blocking: **forge** files both defects now — `PLUGIN_SYNC_CMD`'s
harness-blind advice and Codex's unpinned update command — so they are on record whether or
not this ships. **statusline** takes the session-start nudge as a one-line change once step 3
lands.

## Out of scope

- Dismissal state for declined suggestions — `--pending` covers the need without a store.
- A dry-run flag — `charter version` is the read-only view.
- Downgrades. `--to` accepts any version; nothing special-cases going backwards.
- Teaching opencode to carry skills. Declare the gap; do not invent a workaround.
- A `--json` output mode. One surface, kept readable.
- Blocking feature PRs that ship no entry. The gate is the bump.
- Full historical backfill. Only entries with a probe worth running.
- Widening the status line to carry a command. The nudge does that.
