# Audit — user experience, 2026-08-10

A multi-agent audit of charter, run over seven lenses (first contact, the daily loop,
personas and memory, in-session feedback, secrets, hooks/plugin, module shape). Fifty
findings; the six highest-impact were each handed to an agent told to *refute* them, and
all six survived.

**Kept for the reasoning, not the list.** The "What I would not do" section near the end
is the most durable part — it records what was considered and rejected, and why, which is
the thing that otherwise gets re-proposed every six months.

## Status

| Finding | State |
| --- | --- |
| Lead: a worktree with no `charter.toml` resolved to itself | fixed — `find_root` falls back to the main working tree |
| doctor reported a missing plane as `ok` | fixed — WARN, and it names the bound plane |
| 1.1 `version bump --push` ran `git git add` | fixed — plus a guard in `commit_push` |
| 2.1 `op` called once per persona per render | fixed — TTL cache off the render path |
| 2.2 credential guard fired outside any plane | fixed — gated on `HAS_CONTROL_PLANE` |
| 1.7 `recall` dropped tokens of ≤2 characters | fixed — floor of 2, stopwords, word-boundary matching |
| 2.3 leak guard substring-matched prose | fixed — inspects argv; `--rev` abbreviation closed |
| 1.3 `charter save` claimed success in a non-git plane | fixed — refuses, and a failed commit is a failure |
| 1.4 repo table dropped rows alphabetically | fixed — ranked selection, stable display order |
| 1.6 `secret set` stored `""` on empty stdin, exit 0 | fixed — refuses; `--allow-empty` is the override |
| 1.5 plaintext vault could land in git | fixed — an unignored path is refused at `vault add` |
| 1.8 `worktree list`/`status` blind to the root tree | fixed — both use `repo_trees` |
| 1.9 version-skew warning reached nobody | fixed — `systemMessage` at session start; doctor FAILs |
| 2.5 `workspace use <typo>` created and locked it | fixed — refused with a did-you-mean; `--create` to make one |
| 2.6 permanent `⚠ reinit` chip on new workspaces | fixed — `ensure` scaffolds |
| 2.11 SessionStart rewrote a tracked README.md | fixed — left to `charter docs` |
| 3.1 a second committer pushing over SSH | fixed — `charter/planegit.py` is the one committer |
| (not in the audit) two sessions in one window shared a workspace | fixed — `WINDOWID` is not a pane id |
| 3.4 no timeout on `util.run`; doctor printed nothing on a stall | fixed — `ProcTimeout`, per-check budgets, streamed results |
| everything else | open |

Numbers in sections 2 and 3 that were not adversarially re-checked are flagged in
"Evidence caveats" at the end. Two of the report's own claims are discounted there.

---

# What to fix in charter, and in what order

## Start here: make a worktree resolve to its plane even when `charter.toml` isn't checked out

**One edit, in `charter/root.py:49-53`.** When the upward walk for the marker finds nothing, ask `main_worktree_of(start)` (already in the file, at `charter/root.py:85`, pure path arithmetic, no subprocess) whether you are standing in a linked worktree — and if so, restart the walk from the main working tree.

Today the redirect in `_plane_of` (`charter/root.py:89-110`) only fires when a copy of `charter.toml` happens to be checked out in the worktree. `cmd_init` (`charter/commands.py:800-824`) writes `charter.toml` and never stages it, so in the documented embedded flow it never is. The result, reproduced end to end:

```
$ git init && charter init --forge github --owner acme
✓ Initialized control plane (schema 1) → charter.toml, personas/, workspaces/, .gitignore, ...
$ git ls-files
README.md
$ charter worktree add webapp task1
✓ webapp · task1 → …/webapp.worktrees/default/webapp/task1
  enter:  cd …/webapp.worktrees/default/webapp/task1 && claude
$ cd …/webapp/task1 && charter doctor
  ✓  charter.toml     no control plane found
  ✓  personas         none defined
```

You followed charter's own instructions and landed in a session with an empty persona roster, an empty vault, and memory writing into `…/webapp/task1/workspaces/default/memory/` — a directory `git worktree remove --force` deletes. `charter doctor` says all green.

This is the change I would make first because (a) it is charter's flagship embedded workflow, (b) the failure is total and silent — wrong personas, wrong vault, forked memory — and (c) the alternative fixes only police an invariant, while this one removes it. `_plane_of` already encodes the rule "plane identity follows the main working tree"; this applies it where it currently cannot reach. It also fixes a variant nobody noticed: a worktree cut from a branch that predates the charter commit resolves `HAS_CONTROL_PLANE False` even when `charter.toml` *is* tracked.

`$CHARTER_ROOT` still wins outright, so the documented escape hatch for a deliberately separate plane in a worktree is untouched.

Land two companions in the same PR, both one-liners:

- `charter/commands_worktree.py:87-92` — after `git worktree add` succeeds, `if not (path / _root.MARKER).is_file()` warn loudly next to the `enter:` line. Cheap invariant assertion; keep it even after the root fix.
- `charter/doctor.py:234-235` and `:247-248` — stop returning `OK` with detail `"no control plane found"`. That green check is what converts a broken session into a confidently-green one, and it recurs in four other findings below.

**Cost:** half a day including tests. Do *not* implement the "check `git ls-files --error-unmatch charter.toml` in the clone" guard that looks obvious here — it passes for the pre-charter-branch case while the worktree is still plane-less. The clone's index is the wrong thing to interrogate.

---

## 1. Silent or misleading failures

Ranked by (damage × frequency) ÷ cost.

**1.1 `charter version bump --push` never commits, and says it did.** `charter/commands.py:1174` passes `["git", "add", rel]` to `commit_push`, whose `_git` helper (`charter/commands.py:16`) already prepends `git` — so the command run is `git git add charter.toml`. It fails, `git diff --cached --quiet` finds nothing, `commit_push` returns 0 via the "Nothing to save" branch, and line 1178 prints `✓ committed + pushed — teammates conform on their next session.` Every other caller in the tree passes `["add", ...]`. **Fix:** delete the word `git`. Then make `commit_push` reject an `add_cmd` starting with `"git"` outright, and add a test that bumps with `--push` against a tmp git plane. **Cost: five minutes.** I re-verified this one in source.

**1.2 A stale `$CHARTER_ROOT` binds silently to a different plane.** `find_root` honours its own docstring ("a bad value raises rather than falling back… silently operating on a *different* control plane is worse than failing", `charter/root.py:32-37`), but `find_root_or_cwd` swallows the exception and returns cwd, and `config.ROOT` is built from that. When cwd is itself a plane root — which in the embedded shape is the repo you work in — you get someone else's personas, memories and vault registry, and the SessionStart hook injects them into the model with no warning. `charter doctor` reports `✓ charter.toml parsed cleanly`. Nothing anywhere prints which plane is bound.

**Fix, in order of value:** (a) print the bound path on doctor's `charter.toml` row unconditionally — `✓ charter.toml parsed cleanly (/path/to/plane)` — this one string kills stale-env-var, nested-plane and rootless-cwd at once, without enumerating any of them; (b) record `config.ROOT_ERROR` when `$CHARTER_ROOT` was set and did not resolve, and have the CLI dispatcher refuse every command except `--version` and `init` with root.py's own `_explain` text; (c) lift the `HAS_CONTROL_PLANE` guard already in `cmd_reinit` (`charter/commands.py:911-914`) into a shared `require_plane()` and apply it to everything that writes under `config.ROOT` — today `charter persona create` happily scaffolds `personas/backend/{persona.md,memory,refs}` into a bare temp dir, or into a gitignored workspace that `workspace rm` deletes. **Cost: (a) an hour; (b)+(c) a day.**

**1.3 `charter save` prints `✓ Committed` in a plane that isn't a git repo.** `charter init` in a fresh directory does not run `git init`, which is exactly the README's 60-second path. Then `_git(add_cmd)` fails silently (`check=False`), `git diff --cached --quiet` returns 128 so the "Nothing to save" branch is skipped, the commit fails, and `rev-parse --short HEAD` yields empty, so `charter/commands.py:391` prints `✓ Committed : charter save: 0 file(s)` and exits 0. Personas and memory the tool told you to commit have no history. **Fix:** check `git rev-parse --git-dir` once at the top of `commit_push` and fail with the real reason; treat a non-zero `git commit` as failure. Add a doctor check that the plane is a git repo with an origin. **Cost: small.**

**1.4 The status-line repo table drops rows in alphabetical order, so the repos with something to say vanish first.** `charter/statusline.py:595` is a bare positional slice of `_MAX_REPO_LINES - 1`. With 18 clones, 13 rows went to clean `aaa-svc-NN` repos on main and `…(+5 more)` swallowed every dirty repo, the only off-main branch, and both red pipelines. Worse: the repo you are standing in can be among the hidden, so the bold "you are here" marker attaches to nothing.

**Fix, and mind the ordering trap:** rank for *selection* only — current repo unconditionally first, then dirty/ahead/behind, CI failed or running, open change — then **re-sort the chosen set back to alphabetical before drawing**, and index `_PALETTE` by the repo's position in the full `dirs` list rather than by `clone_i` counting within `show` (`charter/statusline.py:601-607`). Without those last two, a repo's colour and row position change turn-to-turn as its state flips, and the table stops being a stable spatial index. `states` and `gl` are already computed for every directory at `charter/statusline.py:1069-1072`, so this is a key function, not new I/O. Then `…(+5 more, all clean)` becomes a claim the code can back. **Cost: small.** No test asserts *which* repos are shown, so selection is free to change.

**1.5 A plain-file vault outside `.charter/` silently disables the in-session leak guard.** `charter/hooks.py:90` hardcodes `_VAULT_READ_RE` on the literal path `\.charter/(?:vaults|browser|active-)`. Register a vault with `--file secrets/team.json` and the agent can simply `cat` the credential — the PreToolUse guard allows it, while the same read under `.charter/` is denied. The plaintext is also unignored (`charter init` writes a `.gitignore` covering only `/.charter/`) so it lands in git, and `charter doctor` reports `✓ vaults 2 configured, all healthy`.

Note the causal knob is `--file`, not `--share`: the same damage occurs with no `--share` at all. **Fix:** (a) build the hook's deny set from `registry.vaults()` — every registered vault's `config["file"]` plus its `.meta.json` sibling — keeping the `.charter/` literal as a fallback; (b) in `cmd_vault_add`, for `plain-file` only, run `git check-ignore -q` on the resolved path when it lands inside `ROOT` and hard-error in the style of `registry.py:187-195`, naming `--file .charter/vaults/team.json` and `--provider reference` as the two ways out. **Cost: small.** Skip the proposed `PlainFileProvider.health()` change — `charter/secrets/plain_file.py:147-148` already returns `"not created yet ({path})"`; what's missing is that `secret list`/`secret get` never consult it, so a dead registration reports "no secrets" instead of "the file isn't on this machine."

**1.6 `charter secret set <vault> <key>` with no stdin overwrites the credential with an empty string, exit 0.** `charter/commands_secrets.py:208-210` treats any non-tty stdin as "read it" — which includes an agent's Bash tool and `< /dev/null`. Afterwards `get` says "present", `vault list` counts it, `doctor` says healthy; the failure surfaces hours later as a 401. **Fix:** refuse rather than warn — require `--allow-empty` to store `""`, and when stdin is not a tty and none of `--stdin`/`--from-file`/`--value` was passed, error naming the three ways to supply a value. Report empty as its own state. **Cost: small.**

**1.7 `charter recall` discards every query token of two characters or fewer, then declares "No memories match".** `charter/memstore.py:159`: `if len(t) > 2`. So `recall "S3"`, `"CI"`, `"db"`, `"TZ"`, `"v2"`, `"PR"` all return a confident negative against a corpus that contains them. The SessionStart briefing shows 10 titles and tells the agent to search for the rest, so search is the only route to everything else. Compounding it, `charter/memstore.py:174` scores stopwords as terms with no IDF, so "for" can outvote the one word that mattered. **Fix:** lower to `>= 2`, add a ~40-word stopword set, and never print "No memories match 'X'" when every token of X was dropped. **Cost: small** — see the cuts section for where to stop.

**1.8 `charter worktree list` and `charter status` are blind to an embedded plane's root tree.** `charter/commands_worktree.py:107` and `charter/commands.py:534` both use `workspace.clones(ws)` instead of `workspace.repo_trees(ws)` — the list documented as "the one list anything asking 'which repos am I on?' should use", which `gl-refresh` already uses at `charter/commands.py:565`. So `worktree list` says "No worktrees" while the status line one line above draws them, and `charter status` suggests `charter clone` in a shape with nothing to clone. **Fix: one line each,** plus a shape-aware empty-state hint. **Cost: trivial.**

**1.9 The version-skew warning reaches nobody.** `charter/hooks.py:1249-1251` prints it to stderr and returns 0 — Claude Code sends stderr from a zero-exit hook to the debug log only, so neither the user nor the model ever sees it. The second surface, `check_plugin_skew`, returns WARN, and `cmd_doctor` returns 0 on WARN, so the `||` in `hooks/hooks.json:23` never prints it either. `README.md:94-95` promises "A plugin newer than the CLI says so loudly at session start." **Fix:** emit it as `systemMessage` (a universal hook output field that renders at exit 0 and does not block) from `dispatch`, folded into whatever dict the handler already prints, gated once per session via the `config.SESSIONS_DIR` marker pattern the file already uses. Raise `check_plugin_skew` to FAIL. Do **not** reach for `exit 2` — on `UserPromptSubmit` that erases the user's prompt. **Cost: small.**

---

## 2. Repeated friction

**2.1 The status line shells out to `op` once per persona, per render, uncached.** `charter/statusline.py:809` → `_vault_dot` → `registry.provider_for(vault).health()` → `op item list --vault … --format json`. There is a second copy of the same call at `charter/statusline.py:709` (`_vault_glyph`). Profiled at 96% of render time with 10 personas; with a realistic 250ms `op` round trip, one render measured 3.0s wall. Every turn. And with the desktop-app integration each call is a chance for an unprompted biometric dialog. The payoff is one character per chip. **Fix:** cache vault health per name under `STATE_DIR/cache` with a TTL — the exact pattern `_repo_states` already uses at `charter/statusline.py:371-401` — or give the render path a filesystem-only `health_hint()` and leave the network answer to `charter vault list`/`doctor`. **Cost: small. Highest ratio in this section.**

**2.2 The one-credential guard denies SSH git in directories with no control plane.** `charter/hooks.py:416-438` never consults `HAS_CONTROL_PLANE`. Install charter to try it, and `git clone git@github.com:…`, `git commit -S`, `ssh -T git@github.com` and `GIT_SSH_COMMAND=… git fetch` are denied *in every other repo on the machine*, with a message explaining a control plane that doesn't exist there — and `README.md:229` pre-emptively tells the user "that is the rule working, not a bug." **Fix:** gate `_single_credential_reason` and `_clone_commit_reason` on `config.HAS_CONTROL_PLANE`. Leave `_leak_reason` unconditional; it is a safety invariant, not a plane policy. **Cost: small.**

**2.3 The leak guard is a substring scan over the whole command line.** `charter/hooks.py:89-90` matches `--reveal` and `.charter/vault*` anywhere in the string, so `git commit -m "docs: document the --reveal flag"`, `rg -n -- --reveal charter/` and `grep -rn "vaults" .charter/vaults.json` are all hard-denied, with a reason that misdescribes what happened. The sibling SSH guard already does this correctly via `_segments()` and `_invocation()` (`charter/hooks.py:237-256`) for the explicitly stated reason that "a commit message may legitimately *mention* an SSH URL." **Fix:** route `_leak_reason` through the same machinery — deny only when `--reveal` is a real argv token of a `charter` invocation, or a reader program's argv genuinely contains a vault path. Downgrade residual ambiguity to `_ask`. While there, set `allow_abbrev=False` on the secret parsers (`--rev` currently expands to `--reveal` and sails past the guard) and refuse a non-regular-file destination in `cmd_secret_cp` (`charter secret cp team AWS_KEY /dev/stdout` prints the credential, then prints "Value not shown."). **Cost: small.**

**2.4 Below 102 columns, CI and change cells truncate away entirely.** `_LEFT_W` is fixed at 95 (`charter/statusline.py:78`) and rows are clamped to `COLUMNS - 8`. At 80 columns a failing pipeline and a passing one render identically as `…`; at 100 columns `!1387` renders as `!1…` — an MR number silently truncated into a different valid-looking number. The 66 columns spent on name and branch survive; the 18 that answer "is anything broken" do not. **Fix:** make the plan width-aware — shrink `_NAME_W`/`_BRANCH_W` below ~102 usable columns, or collapse CI+change into one 4-column cell (`✗!412`). Never emit a truncated change number; drop the cell whole. **Cost: medium.**

**2.5 `charter workspace use <typo>` creates the typo, locks the session to it, then refuses the correction.** `charter/commands_workspace.py:104-116` validates name *shape*, then `workspace.ensure()` (which mkdirs) and `set_active()` (which takes the session lock). Correcting it hits `✗ Workspace is 🔒 locked to 'fature-x' for this session`. **Fix:** fail on a name that does not exist with a did-you-mean from `workspace.list_workspaces()`, require `--create`, and never take the session lock for a workspace just invented. **Cost: small.**

**2.6 Workspaces born via `clone` or `use` are never scaffolded, so the README quickstart ends with a permanent `⚠ reinit` chip.** `workspace.ensure()` only mkdirs; `workspace.scaffold()` is called by create/live/restore/fork but not by `charter/commands.py:283` or `charter/commands_workspace.py:108`. A correct first-time setup shows a repair warning every turn, phrased as post-upgrade drift. **Fix:** call `scaffold` from inside `ensure`. Reserve the chip for real `structure_version` drift. **Cost: trivial.**

**2.7 `charter workspace remember <ws> "…"` records the workspace name as a memory in the wrong workspace.** Here the workspace is `-w`, but it is positional on `workspace save`, `snapshot`, `restore`, and on `persona remember` (`charter/cli.py:250-252` vs `:301`, `:282`, `:512`). Reusing the shape you just typed writes junk and reports `✓`. The failing variant prints the *top-level* usage line, which never mentions `-w`. **Fix:** accept the workspace positionally when the first token names an existing workspace; at minimum reject a lone argument that exactly matches one with "did you mean `-w <name>`?". Give `workspace recall` a positional query like top-level `recall`. **Cost: medium.**

**2.8 Worktree ergonomics.** `charter worktree add feature-x` in an embedded plane with one repo errors with "the following arguments are required: piece" (argparse ate the piece as the repo). There is no `charter worktree path`, and `worktree list` prints no path, so re-entering yesterday's piece means re-running `add` and reading the path off the error. **Fix:** default `repo` to the sole entry of `repo_trees(ws)`; add a path column to `list`; add `charter worktree path [repo] <piece>` so `cd "$(charter wt path slice-1)"` works. **Cost: medium.**

**2.9 First-contact papercuts, all small, worth one batch commit.** A GitHub user who follows the README install prompt gets `charter preflight failed - fix before working:` with two red `glab` blockers on every session, because `declared_or_default_forges` (`charter/doctor.py:110`) falls back to GitLab when there is no plane — and `✓ charter.toml no control plane found` sits two lines below. `charter --help` names GitLab or `glab` in five places (`charter/cli.py:51, 66, 91, 111, 300`). `charter init` without `--owner` passes doctor clean and then fails with a raw 404 on `groups//projects?…`. `charter discover` against an empty-but-valid org exits 0 telling you to run `charter discover`. In an embedded plane, init's own "Next:" line recommends `charter discover`, which doctor says the shape never uses and which writes an untracked `inventory/` into the user's codebase. **Fix:** skip forge checks when there is no plane; branch the "Next:" line on shape; guard `cmd_discover` on an empty owner and on the embedded shape; neutralize the help strings to "your forge's CLI (gh/glab)". **Cost: small each.**

**2.10 The commitment gate fires on one-line cleanups.** `charter/hooks.py:1011-1020`: `more\s+\w+` in `_FUZZY_RE` and bare `remove` in `_DESTRUCTIVE_RE` classify "add more tests for the parser" as open-ended and "remove the unused import and add a docstring" as destructive/irreversible, then inject 274 tokens instructing the agent to put an `AskUserQuestion` modal in front of the engineer. The module's own comment says it must be "deliberately narrow… or the nudge becomes wallpaper." **Fix:** drop the bare `more\s+\w+` alternative, and require a destructive verb to co-occur with `_SCOPE_RE` or an unnamed/plural object. Re-run against the 1,867-prompt corpus the comment at `charter/hooks.py:1030-1035` says already exists. **Cost: small.**

**2.11 SessionStart rewrites a tracked `README.md`.** `charter/hooks.py:639-643` calls `refresh_readme_personas()`, which splices per-persona *dispatch counts* into a committed file. Opening a session dirties your tree; `_uncommitted_memory_nudge` then complains about uncommitted files. On a shared plane this produces recurring merge conflicts in a block marked "do not edit by hand." **Fix:** drop the call from `sessionstart` and leave it to `charter docs` / `make docs`, which is where the marker already points. **Cost: trivial.**

**2.12 `charter persona optimize` can dump a megabyte into the agent's context.** `charter/curate.py:130-131` emits one uncapped line per near-duplicate *pair*, and pairs are O(n²); every sibling in the same function is capped. Measured 10,596 pair lines / 1.78 MB on a 200-memory templated corpus — and `charter/doctor.py:407` is what tells you to run it. **Fix:** cap at ~10 by Jaccard and summarise the rest; add `--limit`. Also add `charter persona archive` — `memstore.archive()` (`charter/memstore.py:243-259`) is the reversible retire the curation report keeps recommending, and it has no CLI surface, so following the advice degrades to `forget`, which unlinks. **Cost: small.**

---

## 3. Structural work that pays off later

**3.1 Extract `charter/planegit.py` and delete the second committer.** `cmd_persona_memory_sync` (`charter/commands_persona.py:737-790`) reimplements `commit_push` and pushes with `git push origin HEAD` — over SSH, violating charter's headline invariant, on the one memory path the SessionStart hook explicitly tells the agent to use (`charter/hooks.py:478`). The traced difference: `charter save` runs `push -c credential.helper=!gh auth git-credential https://github.com/...`; `memory-sync` runs `push origin HEAD` and reports "Committed locally, but push failed (check git auth)" while `gh auth status` is green. The structural cause is that `commands_workspace.py:20-21` already reaches into a sibling *command* module for `_cred_flag`, `_git`, `_origin_https` — so writing a second implementation looked cheaper than reusing the first. **Move `commit_push`, `_git`, `_cred_flag`, `_origin_https`, `commit_memory_reactive` into `charter/planegit.py`, and make memory-sync one line: `return planegit.commit_push(config.ROOT, ["add", "--", *changed], msg, no_push=args.no_push)`.** Add a test asserting the credential-helper flag appears in the push argv for every command that commits plane memory — `tests/test_persona_memory_sync.py` only ever passes `no_push=True`, which is why this survived. **Cost: small, high value.**

**3.2 Extract `charter/guards.py` — the pure predicates out of `hooks.py`.** About 410 lines of `hooks.py` are total functions (string in, reason-or-`None` out): `_secret_kind`, `_leak_reason`, the whole git-command analyser at `charter/hooks.py:123-407`, `_commitment_signals`, `skew_message`. Two command modules already import a private name across the boundary (`from .hooks import _secret_kind` at `charter/commands.py:367` and `charter/commands_persona.py:742`) and ~13 private names are pinned by tests as if they were the interface.

This is the soil two other findings grew in. Because the predicate lived behind a private import, `commit_push` re-derived "is this a memory file" inline as `if "/memory/" in p or "/refs/" in p:` (`charter/commands.py:370`) — a substring test that, in an embedded plane, makes `charter save` refuse to commit `src/memory/cache.py` because it contains `api_key = os.environ["APP_API_KEY"]`, with `✗ Refusing to save — a secret-shaped value in a memory/ref file`, rc=1, no override flag, blocking every other file in the commit. Three implementations of one idea disagree: `charter/commands.py:370` (substring), `charter/commands_persona.py:717` (personas only), `charter/hooks.py:659` `_MEM_PATH_RE` (correct). **Promote `_MEM_PATH_RE` into `guards.is_committed_memory_path()` and have all three use it; add `charter save --no-secret-scan`.** **Cost: medium; the predicate bodies are already pure and already tested, so it is a move, not a rewrite.**

**3.3 Give `config.py` a `derive(root)` seam.** 25 constants are computed at import from one input with no way to recompute them, so `tests/_isolation.py:22-100` re-implements the derivation line for line and lists all 25 names in `_PATCH`. That duplication has already failed once in production — `git show fa0b365`: four constants missing from `_PATCH` meant the suite wrote fixture data into a contributor's real `.charter/vaults.json`, "orphan[ing] every vault they have." The guard added in response only inspects `Path`-typed constants, and seven non-`Path` ones of exactly the leaking shape already exist. **Fix:** `derive(root) -> Config` (frozen dataclass), `_active = derive(...)`, `__getattr__` delegating so every `config.PERSONAS_DIR` caller is unchanged, and `config.use(root)` as the seam. `PersonaIso.setUp` collapses to one `addCleanup`; `_PATCH` and the guard both become unnecessary. **Cost: medium.**

**3.4 Add `timeout` to `util.run`, and stream doctor's results.** `util.run` has no timeout, so six call sites bypass it with raw `subprocess.run` and their own literal (3, 3, 5, 10, 10 seconds). The un-timeouted paths include `gh api`/`glab api` and every doctor check. A 1Password session needing re-auth hangs the SessionStart preflight for its full 20s budget and then prints *nothing* — `cmd_doctor` (`charter/commands.py:950-962`) collects every `Result` before printing a line, so a killed run emits zero diagnosis, not even the checks that passed. **Fix:** `timeout: float | None` on `util.run` with a `ProcTimeout(ProcError)`, convert the six raw sites, pass 3-5s from forge/vault checks and render `WARN "timed out after Ns"`, and stream each `Result` as it completes. The streaming half alone turns a mystery stall into "got as far as `vaults`, then stopped." While there, `cli.main` catches only `KeyboardInterrupt`, so a `TimeoutExpired` from `charter/commands_persona.py:724` reaches the user as a traceback. **Cost: medium.**

**3.5 Give per-session state an owner.** `workspace._prune` (`charter/workspace.py:235-246`) globs five filename suffixes belonging to three other modules, runs only from `workspace.set_active`, and misses three marker directories entirely (`ws-edit-nudge`, `commit-gate`, `ws-autosave` — grep finds only their write sites). `hooks.py` alone has four hand-rolled implementations of "per-session counter" with two different sanitisation policies, and three modules have three different `_session_id` functions that disagree on what absence means — `persona._session_id` returns the sentinel `"nosession"`, which collides with the GC's live-session test at `charter/persona.py:755`, so the shared `nosession` ephemeral bucket is never collected and accumulates across every session forever. **Fix:** `charter/session.py` with one `current()`/`bucket()` contract, and `charter/sessionstate.py` owning `counter`/`once`/`sweep`, called from the SessionStart hook. **Cost: medium.**

---

## What I would not do

- **Don't route doctor's WARNs into SessionStart context.** The obvious fix for "the `||` wrapper hides everything short of a blocker" is a `--hook` mode that always prints. Don't. That stdout becomes the *agent's* context, and several WARNs are permanently yellow by design — the memory-index nudge fires at ≥150 entries where growth "is not a defect", and the unmanaged-forge hint says `--apply` deliberately no-ops. You would ship an agent that runs `charter discover` and `charter persona optimize --all --apply` before the user's actual task, every session. `charter/doctor.py:265-267` already warns against exactly this: "a permanently-yellow preflight teaches people to stop reading preflight." Five of the nine suppressed checks already reach the user elsewhere (version lock auto-fixes itself, skew prints before every handler, persona and vault problems render as chips every turn, non-token-only git is hard-denied at use). If you want the rest visible, write the non-OK names to `STATE_DIR` and render a count chip (`⚕3`) in the status line. Promote exactly two WARNs to a loud channel — the `[[forge]]`-block failure, because it silently narrows the credential guard, and dangling memory index links, because they make `recall` surface a hit nobody can read.
- **Don't refuse `plain-file --share`.** A team that provisions the file out of band has a legitimate use for a shared pointer. Warn about what it means; hard-error on the *unignored path*, which is the actual defect and also catches the no-`--share` case.
- **Don't build stemming or synonyms into `memstore`.** Word-boundary matching (`\b{term}\w*`) plus stopwords fixes the observed failures and the `version`/`versioned` misrank. A Porter-lite suffix stripper is 20 lines you will maintain forever for the remaining tail; a stdlib search engine is not what charter is for. Stop after the boundary match.
- **Don't make `_MAX_REPO_LINES` configurable, lower the two-column threshold, or make the box frame opt-in.** Cosmetic. Fix the *selection* (1.4) and the narrow-width truncation (2.4); leave the layout alone.
- **Don't rework `gl-refresh` into a thread pool yet.** Cap the sweep, skip cache entries under a minute old, pass a timeout, and write a `partial: true` marker so the status line can show stale data honestly. Concurrency is a bigger change for a background process nothing is waiting on.
- **Don't fix `charter/commands_persona.py:121` (absolute vault path) as its own piece of work.** Real — `persona create --with-vault` bypasses `_portable_file` and re-introduces issue #21's bug — but local-only today. Batch it with 1.5.
- **Don't reduce the 90-day `stale_days` default on evidence available.** 159 of 200 memories proposed for archival is a striking number, but it came from one synthetic corpus. Cap the output first (2.12) and see whether the proposal is still unusable.

---

## Evidence caveats

Everything under "Start here" and items 1.2, 1.4, 1.5, 1.9 survived a deliberate attempt to refute them, including full end-to-end reproduction in scratch planes.

Items 1.1, 1.3, 1.6, 1.7, 1.8 and everything in sections 2 and 3 come from a single investigative pass that was **not** adversarially re-checked. I re-read the source for the ones I lean on hardest and confirm the code is as described: `charter/commands.py:1174` (`["git", "add", rel]`, the only such caller in the tree), `charter/commands.py:370` (substring test), `charter/hooks.py:89-90` (both regexes over the raw command string), `charter/memstore.py:159` and `:174` (`len(t) > 2`, `count()` scoring with no stopwords), `charter/statusline.py:709` and `:809` (two uncached `health()` call sites on the render path). I did not independently re-run the profiling for 2.1, the false-positive probe for 2.3, or the timing measurements for 3.4 — the mechanisms are visible in source but the numbers are second-hand.

Two claims I would specifically discount as stated: the "20 seconds of every session start" figure for `charter doctor` is the hooks.json timeout ceiling, not a cost — measured 0.5-1.1s. And "you find out when the notes are gone" in the lead finding is softer than it sounds: `charter worktree remove` refuses on a dirty worktree, so only `--force` or raw `rm -rf` destroys the orphaned memory. The durable harm is the silently forked plane, not deletion.