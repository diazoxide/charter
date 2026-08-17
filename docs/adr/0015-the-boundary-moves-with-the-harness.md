# The boundary moves with the harness

Charter runs inside Claude Code, and every artifact says so: `plugin.json` describes
"agents for Claude Code", `session.py` reads `$CLAUDE_CODE_SESSION_ID`, `doctor` proves the
guard is live by looking for `CLAUDE_PLUGIN_ROOT`. Adding a second agent runtime — opencode
— breaks one word and one decision.

The word is *host*. The decision is ADR 0014's: **"Policy that can be written as a command
pattern belongs to Claude Code's `permissions`."** Two problems now. `Claude Code's` names
one runtime out of several, and — the larger one — *the set of policy that can be written
as a pattern is different for each runtime*.

**Charter targets harnesses, not one host. It keeps only the policy the current harness
cannot express — plus the policy the harness can express only silently. Where a harness's
ceiling is lower, `charter doctor` names the deficit by name; charter never levels the
other harness down to match.**

## The word

*Host* is already taken and already ambiguous. `hooks.py:399` reads "``host -> a Forge
instance`` for every host the one-credential-PER-FORGE rule must…" and `glstate.py:213`
parses `git@host:group/repo` — while ADR 0014 and `doctor.py:1251` use the same word for
Claude Code. Both senses become load-bearing the moment there are two runtimes.

**Harness** is the agent runtime charter runs inside — Claude Code, opencode. **Host** stays
the forge. Both go in CONTEXT.md's glossary with the other listed under *_Avoid_*, because
this is a distinction that will otherwise be re-argued in every review.

## Why the boundary is conditional, not fixed

ADR 0014's table asks one question of each guard: can the rule be stated without knowing
where you are standing or who you are? That table has a hidden column — it is a *Claude
Code* table.

| guard | Claude Code | opencode | Codex |
| --- | --- | --- | --- |
| `_leak_reason` | no | no | no |
| `_plane_root_branch_reason` | no | no | no |
| `_clone_commit_reason` | no | no | no |
| `toolgate.decide` | no | **yes** | no |
| `_single_credential_reason` | yes | yes | yes |

The row that moved: opencode scopes permissions per agent — *"Agent permissions are merged
with the global config, and agent rules take precedence"* — and charter already generates
one agent definition per persona. A persona's declared tools, which Claude Code cannot
express because its rules are project-scoped, are directly expressible there.

## Expressible is not the same as should-move

ADR 0014 already refused that trade once, for `_single_credential_reason`: *"A native
`deny` rule prints no reason, and the reason is most of what that guard is for: a developer
who reads 'one credential — each forge's token over HTTPS; no SSH, no signing' learns the
rule, while one who reads a bare refusal files a bug."*

The toolgate is the same shape. Moving it into `opencode.json` buys correct per-agent
scoping and pays for it with `permission denied` where a persona is currently told *which*
persona owns that binary. So the rule carries a second clause, and the toolgate stays in the
hook on both harnesses — which has the side effect of keeping the two identical here.

## What parity means, and what it does not

The contract is **guarantee parity**: every invariant charter enforces on one harness is
enforced on the other, and the delivery channel may differ. It is not channel parity.
Holding out for channel parity would make charter's roadmap hostage to another project's
issue tracker: the request for a Claude-Code-style `statusLine` in opencode was closed by a
compliance bot in under two hours, and charter has no lever on that.

State the limit at full volume. **opencode has no status-bar socket and no per-turn prompt
hook.** Charter's status line renders there on demand — `/charter` — and not ambiently, and
its mid-session nudges ride the output of effectful tools instead of arriving beside them.
Charter under opencode is smaller in *surfaces* and identical in *guarantees*, and `doctor`
prints that deficit in those words rather than leaving it to be discovered.

## Harnesses are registered, and why that is not the shape ADR 0007 deleted

`charter/harness/registry.py` holds `KINDS: dict[str, type]`, mirroring
`charter/forge/registry.py` — which records the reason in its own docstring: iterating the
registry means a kind is *"covered automatically the day it's registered — never a hardcoded
literal"*. `init` wires every registered harness without naming one; `doctor` reports
whichever is live. Adding Codex is adding a class.

That is an interface with implementations behind it, which ADR 0007 deleted for plane
shapes — so the difference has to be stated rather than assumed. ADR 0007's objection was
never abstraction as such; it was this: *"only one of them was ever exercised by any given
plane — so the other was carried on trust."* Here **every registered harness is exercised
on every `init`**, because charter writes all of their wiring into every plane rather than
only the runtime it happens to be running in. There is no path carried on trust, which is
the entire load-bearing half of that argument.

The unregistered case is the one that would otherwise rot. `deficits()` returns nothing both
for a harness with no gaps and for a harness charter has never met — one sentence for two
opposite facts. So `doctor` WARNs on an unregistered `$CHARTER_HARNESS` and names the fix,
rather than printing a clean row over an integration nobody has verified.

## Codex, and the second axis nobody expected

Codex CLI 0.147.0 implements **Claude Code's hook contract near-verbatim** — the same
events, the same `hookSpecificOutput` wire, and a `PostToolUse` payload whose required
fields are Claude Code's field names plus `turn_id`/`agent_id`/`agent_type`. So
`charter/hooks.py` very likely speaks Codex already. The harness that looked furthest away
is the one needing least code, and opencode — which looked like ordinary adoption — is the
one that needed a generated plugin and a corrected design.

What Codex does not have is a **project-level config**. A `.codex/config.toml` or
`codex.toml` in a project directory is ignored (checked by planting a type error in each
and watching the config load anyway); hooks live only in `~/.codex/config.toml`. That is a
second axis this ADR did not anticipate, and it is not a capability ceiling — it is a
question of *scope*:

| | where charter writes its wiring | committed | blast radius |
| --- | --- | --- | --- |
| Claude Code | `.claude/settings.json` | yes | the plane |
| opencode | `.opencode/` in **every work tree** | locally excluded | one tree |
| Codex | `~/.codex/config.toml` | no | **every repo on the machine** |

opencode's row is per-tree for a reason discovered the hard way, after a plane-root-only
version shipped: **it does not search parent directories for plugins**, checked by putting
one in a parent and booting from a nested directory, where it never loaded. Work happens in
a clone or a worktree (ADR 0008), so `clone` and `worktree add` arm each tree as it comes
into being, `reinit` backfills, and `doctor check_harness_trees` names any still missing.
Charter's generated files are hidden through `.git/info/exclude` — per-checkout and
untracked, so charter never edits a `.gitignore` the repo's owners maintain — and only the
files charter itself created are hidden.

So `CodexHarness.wire()` deliberately writes nothing, and `doctor` reports the gap rather
than an integration that looks armed. `init` reaching outside the plane to arm hooks in
repos that have no control plane is the failure ADR 0014 already paid for once — its
credential guard "needs `config.HAS_CONTROL_PLANE` to stay silent outside a plane — a gate
added after it fired in unrelated repos and explained a control plane that did not exist
there." Codex also trusts hooks by hash, so an entry written without approval is inert
while looking wired, which is the shape #177 and #197 already cost this repo.

## Both were run, and one of them needed nothing

Verified in live sessions, because everything above is a prediction until it is:

* **opencode** denies and the refusal reads as the rule working — `✗ ssh git@github.com
  failed / Error: charter guard: The control plane is token-only …`, relayed by the model
  as a policy interception rather than a crash. CONTEXT.md requires that of a deliberate
  denial, and it was the one thing about opencode that could not be checked for free.
* **Codex needed no charter code at all.** `charter hook pretooluse` spoke its contract
  as shipped: `hook: SessionStart Completed`, `hook: UserPromptSubmit Completed`,
  `hook: PreToolUse Blocked`, carrying charter's own reason verbatim into the transcript.

Both runs also earned the fail-open branch its scepticism. The opencode shim called
`.stdin()` on Bun's shell, which does not exist — so it threw on every tool call and the
catch failed it open, leaving no guard running while `doctor` reported the tree wired. It
parsed, it loaded, it did nothing. Bun takes stdin by redirection (`$`cmd < ${blob}`), and
the test that now pins it drives the hook through a `$` stub implementing only what Bun
implements. **Asserting that generated code parses is not the same as asserting it works.**

## A ceiling that carries what to do about it

`doctor` earned its deficit list by refusing to render a lower ceiling as health. The next
failure is subtler: **a limit stated and left there reads as "nothing can be done"**, and
the operator stops looking. So a `Deficit` carries a `remedy` — a command that closes the
gap — printed on the ceiling it answers rather than in a footnote.

`charter statusline --watch` is the first: the plane state repainting in place in any
spare terminal, needing no status-bar socket and no multiplexer, and identical on every
harness including the one that does have a bar. It says what it cannot show — there is no
session payload, so the token and context columns are blank — because a render that looks
like the real thing while silently omitting a column teaches the reader to trust a number
that is not on the screen.

An empty remedy is a claim too, and stays empty. charter cannot conjure opencode a
per-turn prompt hook, and inventing one would send somebody off to configure a thing that
does not exist — which costs more than an honest gap.

## What this is NOT

It is not two implementations of charter. The opencode integration is written concretely and
its seams extracted where duplication actually drew blood — the registry exists because two
harnesses already needed the same three answers, not in anticipation of a third. Everything
past those three members (`name`, `deficits`, `wire`) stays out of the base class until a
second harness demonstrably needs it.

One abstraction is built up front, because it *removes* branching rather than adding it: the
harness injects `CHARTER_HARNESS` and `CHARTER_SESSION_ID` into every shell it spawns.
"Which harness am I?" and "which session is this?" are then answered once, at the edge,
instead of inside every function that would otherwise sniff runtime-specific variables.

How each harness supplies them differs, and only the second is interesting. opencode's hook
is `shell.env: async (input, output)`, and against opencode 1.18.18 `input` is
`{cwd, sessionID, callID}` — the session id arrives **per invocation**, so the shim reads it
from `input` and caches nothing. That is not fastidiousness: one opencode server hosts many
sessions, so a module-level "current session" would have no single correct value. Claude
Code has no per-shell hook, but `CHARTER_HARNESS` is a constant there and `settings.json`'s
`env` *"sets environment variables that apply to every session"*; its session id keeps
arriving as `$CLAUDE_CODE_SESSION_ID`, which `session.current` still reads so that nothing
already running regresses.

The published opencode documentation shows `shell.env` receiving `cwd` alone. That is what
this ADR claimed in its first draft, and the deficit it produced — *two sessions in one
directory cannot hold different workspace locks* — was wrong. It was caught by running the
binary, which is the whole argument for the manual per-harness gate: a deficit charter
reports that is not real argues against a capability it actually has.

## Consequences

* `charter guard ask "git push *"` writes the harness's own file in the harness's own
  syntax — `Bash(git push *)` into `.claude/settings.json`, or
  `{"bash": {"git push *": "ask"}}` into `opencode.json`. The operator types the same thing.
* `charter init` is opencode's **only** install path. There is no marketplace and no
  published package: the plugin is generated from a template inside the Python package, so
  charter still ships one version number rather than three. This repo has paid for version
  skew four times in its last five releases (`caaa8c0`, `d71fa09`, `e50e712`, `987cd66`).
* That generated TypeScript is code charter ships and never executes — the failure shape
  this repo keeps paying for (#177, #197). CI must run it, or a syntax error reaches every
  user with the suite green.
* `_load_settings`'s restraint carries over unchanged: a missing file reads as `{}`, a
  malformed one refuses rather than repairs. It matters more on `opencode.json`, which holds
  the user's provider and model config — charter clobbering that is a different order of
  damage than clobbering a settings file it half-owns.
* `doctor` gains a harness line and a deficit list, because a lower ceiling that says
  nothing is indistinguishable from a broken integration (`tests/test_doctor_absent_is_not_health.py`).
* ADR 0014 stands as written. It records why the principle exists; this extends it, which
  its own closing line invites — *"should be revisited rather than defended."*
