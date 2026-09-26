# A cross-repo change is a record of intent, and charter lands it one member at a time

**Accepted 2026-09-26**, by the operator's rulings D1–D7 on the split of charter#360 (#466).
The design is the Python charter's Phase 4 spec (charter-plane,
`docs/superpowers/specs/2026-08-28-phase4-cross-repo-change.md`, §3.1–3.7, §6 and §8), carried
over in the app's terms. Where the operator's rulings changed a Python answer, this record says
so. `docs/plane-format.md` already records the files; nothing here changes their format.

One piece of work often spans several repos: a core API bump, and the two repos that consume
it. Each repo gets its own branch and its own pull request, and nothing says the three are one
piece of work, which must land first, or how far the landing got. ADR 0051's per-repo `pr` mode
opens a PR in each repo, but it does not know that they belong together.

## Words

"Change" already means one pull request in parts of the forge code. In everything this record
covers:

- a **change** is the cross-repo object: one intent across N repos;
- a **member** is one repo's part of a change;
- a member's pull or merge request is a **request**, never a change.

## The decision

**1. A change is a record of intent, per workspace (§3.1).** It is one JSON file,
`workspaces/<ws>/changes/<slug>.json`, with six keys: `change`, `why`, `created`, `by`,
`members` and `excluded`. Each member has `repo`, `branch` and `needs`. The key set is closed
at both ends, so an unknown or a missing key is refused by name. A misspelt `need` must not
become an ordering that silently ceased to exist. Every string is one contained line.
Serialisation is canonical, so a record read and written back is byte-identical.

- **Nothing derivable is stored.** There is no state, no request number, no check result and no
  "landed" flag. Git and the forge know those, and a stored copy would disagree with them.
- **A change belongs to its workspace for life.** Its members are clones, and clones live in a
  workspace.
- **It is committed exactly when its workspace is LIVE.** The LIVE block already un-ignores
  `changes/**` and re-ignores `changes/log/`. A LOCAL workspace's change is a file on one
  machine.
- **A change ends.** `charter change forget <slug>` deletes the record. Its name lives on in
  branch names, request bodies and commit trailers.

**2. Landing is a declaration, read against git (§3.1).** `charter change land` appends one line
to `workspaces/<ws>/changes/log/<host>.jsonl`, which is never committed and is unioned by
`.gitattributes`. The line records that charter merged this commit for this change. A member
is *landed* when the forge reports its request merged **and** the default branch contains the
sha the log recorded. The forge alone cannot see a revert, and the log alone cannot see a
browser merge.

**3. Ordering is declared, derived on each read, and enforced where charter acts (§3.2).**

- `needs` names the members that must land first. A cycle is refused at write time, with both
  members named.
- "Blocked" is computed on each read and never written.
- `land` refuses a member whose blockers have not landed. It cannot stop a person merging in the
  browser. When that happens, doctor names it as a divergence at FAIL.

**4. There is no cross-repo merge loop (§3.3).** `charter change land <slug> --repo <name>`
lands one member. There is no `--all`, and a test asserts that it does not exist. A flag an
agent can pass is a flag it will pass. `--all` would also have to guess what to do when member 3
of 5 is rejected. There is no atomicity. What replaces it:

- the slug is on every artifact charter makes: branch, request title, cross-link block and a
  `Charter-Change: <slug>` trailer;
- a partial landing is shown as `PARTIALLY LANDED (n of m)` with the outstanding members named;
- a change is never shown greener than its worst member.

A rebase merge is refused for charter's own landing, because it leaves no commit to carry the
trailer and no single sha to revert.

**5. The work happens in the clones the workspace already has (§3.4).** A member must resolve
to a clone in this workspace, and `add` names `charter clone` as the fix when it does not.
Each member's branch is stored in the record, with `change/<slug>` as the default offered.
Charter builds no symlink farm, mount or synthetic monorepo. The workspace directory already
holds the clones side by side, and `charter change show` says which of them are one change.

**6. CI is read at the exact head sha, as one of five closed values (§3.5).**

- `PASSED`: at least one check at this head, and each one concluded success, neutral or
  skipped.
- `FAILED`: failure, cancelled, timed out, startup failure or `action_required`.
- `RUNNING`: queued or in progress.
- `NOT RUN`: zero checks at this head. A stale run does not count.
- `UNKNOWN`: charter could not ask, or got an answer it does not recognise.

Precedence is `UNKNOWN` > `FAILED` > `RUNNING` > `NOT RUN` > `PASSED`. `gh pr checks`,
`mergeStateStatus` and the status line's `ci_status` are forbidden inputs, because each one
turns "nothing ran" into green (charter-plane #561). A pushed fixup returns its member to
`NOT RUN` at once, because checks at another sha are not checks on this head. Charter never
waits or polls for checks to appear. Where it cannot see every check a person would see at that
head, the answer is `UNKNOWN`, never `NOT RUN`.

**7. The surface is a view tab (§3.6, ADR 0043 amended 2026-09-23).** The Python frame's
component, picker and toggle key become a view tab opened from the palette, as every new
surface is. Its blocks come from the core. Forge state is read when the tab opens and on a
Refresh press, never on a render or a workspace switch, and the age of the read is shown.
Every value from the record goes through containment before it is laid out.

**8. A revert is a new change (§3.7).** `charter change revert <slug>` seeds `revert-<slug>`.
Each landed member gets a branch carrying `git revert` of the logged sha, with `-m 1` only when
git says the sha has more than one parent. The new change is then pushed, checked and landed
like any other. Charter never force-pushes, deletes a branch, resets a default branch, or closes
a request it did not open. A member merged outside charter has no log line, and revert names it
as needing a person.

## The operator's rulings

- **D1: the change is wanted, declare-and-read first.** T1–T5 (#466–#470) ship first: this
  record, the local verbs, the doctor row, `show`, and the view tab. `push`, `land`, `revert`
  and the view's actions (#471–#474) are filed but not built until the operator has used the
  first five.
- **D2: it lives in `charter-core`, not in a built-in extension.** The floor and the land gate
  must hold the same way for the app and the CLI. An extension's writes are only detected, not
  confined (ADR 0053). The plumbing it builds on is already core: the LIVE block, `planegit`,
  doctor, `floorguard` and `forge::pr`.
- **D3: `land` merges directly, and only at each member's verified head sha.** One member per
  invocation, through the merge API's `sha` guard, after both gates pass: every blocker has
  landed, and CI is `PASSED` at that same sha. Requesting auto-merge would not work here:
  GitHub refuses to queue it on a PR that is already mergeable, and the gates exist only if
  charter is the one merging. `floorguard::PUBLISH_FORGE` already keeps `charter change land`
  attended-only, and an agent still never merges. This makes `forge::pr`'s "Nothing here
  merges" false once T7 lands, and T7 updates that module's doc.
- **D4: `charter change push` ignores a repo's `mode = "off"`.** ADR 0051's `off` governs
  *saves*, which commit a developer's work nobody asked to commit. `change push` is an explicit
  verb over repos someone named by hand, and it commits nothing. It prints every repo, branch and
  destination before it pushes, and the app asks first with the *Save all* confirmation. ADR
  0051 is amended to say so.
- **D5: `revert` is in scope** (#473), as the last ticket. Without it, the only cross-repo
  rollback left is a force-push.
- **D6: cross-plane search and per-workspace layout persistence are dropped** from #360, with no
  tickets. Neither has been asked for since the app existed. If one is wanted, it is filed on its
  own.
- **D7: Rust tests replace "recorded against the frozen oracle".** The frozen oracle
  (`tests/fixtures/recorded/behaviour.jsonl`) has no `change` rows, and ADR 0046 forbids
  recording new ones from Python. The contract is `docs/plane-format.md` §"a cross-repo
  change", tested field by field, plus behaviour ported from `cli-final`'s
  `tests/test_change*.py`.

## Consequences

- `charter change` is a new CLI command group in `charter-core` and `charter-cli`.
- Doctor's deferred `changes` row is replaced by a real check that reads every workspace from
  disk and never fetches (#468).
- `forge` gains two reads: a request looked up by head branch (number, state, head sha, and the
  merge commit when merged) and the checks at one exact sha (#469). T7 adds its first merge.
- The view tab's command goes on the window's allow-list (ADR 0052).
- ADR 0051 is amended: `off` does not cover `charter change push`.
