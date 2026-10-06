# A plane is untrusted until the operator opens it

[ADR 0033](0033-a-plane-is-a-project-and-a-window-may-hold-several.md) turns opening a plane into
picking a directory in a file dialog. Until now a plane was a directory the operator was already
standing in, having typed `purlis init` there themselves or cloned it deliberately. A file
dialog will happily point at a directory that arrived in a tarball, a shared volume, a colleague's
checkout or a repository cloned to see what it does.

So the question "what does opening this plane put in force?" has to have an answer before the
first open, and the answer has to be shown to the operator.

## What a plane contributes, measured

**Its committed settings choose your plugins and set your environment.**
`crates/purlis-core/src/layer.rs` reads the plane's own `.claude/settings.json`
(`layer::plane_settings(plane, SETTINGS)`, where `SETTINGS = ".claude/settings.json"`) and copies
two keys out of it:

```rust
const WORKSPACE_KEYS: [&str; 2] = ["enabledPlugins", "env"];
```

into the `.claude/settings.json` that purlis generates in every directory a harness reads
configuration from but the plane's own files do not reach — a checkout with a git root of its own
(`guest.rs`) and each `workspaces/<ws>/` (`wslayer.rs`).

That source file is committed by design. `purlis init`'s baseline `.gitignore` ignores
`/.claude/settings.local.json` with the comment *"Its committed sibling `.claude/settings.json`
is deliberately NOT ignored — that one is the team's."* So it clones. A stranger's plane therefore
decides which plugins are enabled in every chat the operator starts under it, and sets environment
variables on every harness process it starts — and ADR 0022 measured where those land: *"A
variable set on the harness process reaches the shell the model runs"*, verified on Claude Code
(the grill's own tool shell saw `CLAUDE_CODE_MESSAGING_TOKEN`) and on Codex.

**And opening a plane does not only read it — it starts programs out of it.** `setup` calls
`chats.put_back(&record, root, STARTING)` with the record read from that plane's
`.charter/app/reopen.json` (`reopen::IN_PLANE`), synchronously, before there is a window — the
code's own comment says *"Every chat here starts a program, synchronously, before there is a
window."* For a chat that was on a harness profile the profile is looked up again in the plane's
local file; for a chat that was not, `Chats::start` takes what runs from `chat.launch()`, and its
own doc comment is explicit: *"what runs is decided from the record alone."* `.charter/` is
gitignored, so this does not arrive through `git clone` — but the opener opens a **directory**,
not a clone, and a directory that arrived any other way brings its `.charter/` with it. The ask
therefore has to come before the reopen, not after it.

## Two limits already exist, and both belong in the record

Neither is new and neither is enough on its own, but a reader who does not know them will
over-state what this decision is for.

1. **A plane can restrict, never grant.** Beside `WORKSPACE_KEYS` in the same file:

   ```rust
   const RESTRICTIVE: [&str; 2] = ["ask", "deny"];
   ```

   `permissions` travels only as `ask` and `deny`. `allow` never travels, and the docstring says
   why: copying a grant sideways *"puts a permission in force where no one clicked for it"*. So a
   stranger's plane can add prompts and refusals to your harness; it cannot pre-approve anything.

   *Amended 2026-10-06 (spec #1330):* one grant now travels, by decision. The hosts a sandboxed
   chat may reach are chosen by the project's committed settings, and everyone who opens the
   project follows them with no per-person approval. A one-time Notice, a chat never writing
   sandbox settings, and policy locks stand in for the approval. The sandbox's `off` and its
   denial classes still never travel ([ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
   §1, as amended).

2. **A committed file cannot decide how a chat launches.** ADR 0022 puts harness profiles in
   `charter.local.toml`, which `purlis init` gitignores and `doctor` warns about while git would
   commit it, and a `[harness.<name>]` table in the committed `charter.toml` is refused by name.
   Beside it, `profiletrust` shows a new or changed profile command and asks `run this? [y/N]`
   before it runs.

So the exposure is narrower than "a stranger's plane runs whatever it likes on your machine". It
is: a stranger's plane chooses your plugins, sets your environment, tightens your permissions, and
— where a `.charter/` came along with the directory — names programs that a launch starts.

**And its personas grant programs.** A persona's `tools:` is committed text, and the persona tool
gate (`personagate.rs`) answers `allow` for a program a persona declares, so the harness does not
prompt for it. A tool can also be a script the persona ships in its own `bin/`. That makes a
persona's grant a contribution of the same kind as `enabledPlugins`: it decides what runs without
anyone being asked. (Amended 2026-09-29, see *Grants* below.)

## The decision

**A plane is untrusted until the operator has opened it once and approved it.** The first open of
a path shows what that plane will contribute and asks. The answer is remembered in the
machine-level record ([ADR 0034](0034-charter-keeps-a-little-state-outside-every-plane.md)), so it
is asked once per human per machine rather than once per window.

**`profiletrust` is the pattern, and the pattern is a fingerprint, not a path.** That record stores
each profile's `kind`, `command` and `env` as last launched and asks again when any of them
differs. The plane record does the same: the approval names the plane's `enabledPlugins`, the
names and values of its `env`, its restrictive rules, the programs its reopen record would
start, and each persona's tool grant. A plane that changes what it contributes asks again, and a path whose directory has been
replaced by a different one does not inherit the old yes. Keyed on the path alone it would.

**Every unreadable state means ask.** A missing entry, a malformed one, an entry that is not a
fingerprint: each reads as "not approved". `profiletrust.rs` opens with this rule and the reason
holds unchanged here — *"Treating silence as a yes is the one state this record exists to keep
out."*

**The ask is a dialog, not a printed command.** ADR 0003 and `test_init_first_clone.py` both
record that purlis's consent is a second command because `util.py` has nothing that reads stdin
and blocking a hook on stdin hangs a turn. That constraint is about the CLI. The app has a window
and a person looking at it, so here the prompt is the prompt. The CLI is unchanged and keeps the
two-command shape where it needs it.

## Grants

*Amended 2026-09-29.* The approval covers each persona's tool grant: every tool it may run without
a prompt, and for a tool that is a script in the persona's own `bin/`, a digest of that file's
bytes (`personagrant::grant`). A grant that is new, or differs from the approved one in any way,
asks again, exactly as a new plugin does. A persona that stops granting is reported, not asked
about.

**The gate answers from the approval, not from the file.** The approval is asked at plane open,
but a plane that is already open keeps changing under it: a pull lands, a session edits a file.
So the persona tool gate's last rule is that the tool, and the program behind it, is the one this
machine approved for that persona in that plane. A tool that differs declines and meets the
harness's ordinary prompt until the operator approves the plane again, which closing and reopening
the project asks for. The per-session ceiling (charter#432) still applies beneath this: it can
narrow the approved grant, never widen it.

**purlis vouches only for what it writes.** purlis rewrites the reopen record and refreshes the
fingerprint to match (`Store::vouch`). It does not write the plane's settings or its personas, so
a vouch now refreshes only the record's half and keeps the rest as approved. Before this, a
change to the settings made while the plane was open was taken into the approval at the next chat
purlis opened.

**Where no approval can be kept, nothing is smoothed.** A plane this machine never approved, and a
platform with no machine store (ADR 0031), have no approved grant, so every persona tool meets the
harness's prompt there. That is the direction ADR 0031 asks for: a guard that cannot be expressed
declines rather than guesses.

A store written before this amendment has no grants in it, so the first open of each plane after
it asks once.

## `purlis init` on an existing repo adopts it, rather than colonising it

**Today**, `purlis init` scaffolds the plane in whatever directory it was run in. Run inside a
repository it writes `charter.toml`, `personas/`, `workspaces/`, `.charter/` and a block of
`.gitignore` rules (`_GITIGNORE_BASELINE`, `_ensure_gitignore`) into that repository, and then
*offers* — as a printed command, because purlis cannot prompt — `purlis init --clone-this-repo`,
which clones the repo into `workspaces/default/<name>/`. The offer exists already, as a
consequence of [ADR 0007](https://github.com/diazoxide/charter-plane/blob/0ae0961d8a6a8e59b48ba43b10d28de8fd87afb7/docs/adr/0007-one-plane-shape.md) removing the embedded shape; it is pinned by
`tests/test_init_first_clone.py` and ported to Rust in `crates/purlis-core/src/scaffold/`.

**The default reverses.** `purlis init` on an existing repo adopts that repo as the plane's first
clone and makes the plane beside it. "Make this repo itself the plane" stays available — it is how
purlis's own plane exists — and becomes the non-default.

The reason is this record's subject. `_is_repo_top_level`'s docstring already draws the line the
old default rests on: the offer is refused for a repo the plane merely sits inside, because *"the
offer exists for one person standing in one project, which is the equality case."* The opener
destroys that equality case. A directory chosen in a file dialog has no person standing in it, and
writing a plane's scaffolding — including an edit to a tracked `.gitignore` — into a repository
somebody picked from a list is a write they did not type. The default has to be the one that
writes nothing into it.

## What this is not

**Not a boundary.** `profiletrust.rs` says it at full volume about its own record and the same
sentence applies here: a chat that can edit the plane can edit whatever purlis reads from it.
Once a plane is approved, everything in it is in force, and this record adds no guard inside an
approved plane — with the one exception *Grants* describes, where the tool gate compares against
the approval rather than the file. What the ask closes is the accident and the stranger's directory — the plane
opened to see what it was, the archive that came with a `.charter/`, the checkout borrowed for an
afternoon. It is the difference between a plugin that was enabled unseen and one that was read out
loud first. It is not a defence against an agent that set out to forge the fingerprint, and
nothing here should be built as though it were.

**Not a complete inventory of what a plane does.** The ask shows what purlis can enumerate: the
two `WORKSPACE_KEYS`, the restrictive rules, the programs the reopen record names, and the
personas' tool grants. It does not
and cannot summarise a plane's persona charters, its memory or its todos, which are text a model
will read and act on. purlis *"has no model and makes no judgements about the content of work"*
(`CONTEXT.md`), and that is exactly the sentence that limits this ask. Named here rather than left
to be discovered by whoever first assumes the dialog covered everything.

## What was rejected

- **Trusting any plane the operator picked, on the grounds that picking it is consent.** Picking a
  directory in a dialog is consent to *look*; nothing in that gesture says which plugins the
  operator agreed to enable or which program a record may start.
- **Refusing to open an unapproved plane at all.** The operator would have no way to see what they
  were refusing, which is the failure ADR 0022 fixed by *showing the command* rather than refusing
  the profile.
- **Remembering the approval in the plane.** It would travel with a copy of the plane to every
  other machine, where nobody approved anything — and it would be written by the very directory it
  is a judgement about.
- **Keying the approval on the path.** A directory that has been replaced would inherit the yes.
- **Asking once per window rather than once per machine.** `report.py` already measured where that
  ends: *"a Reporter with several planes would be asked repeatedly until the safeguard became a
  reflex."*
- **Keeping the old `purlis init` default and warning instead.** The warning would arrive after
  the scaffolding and the `.gitignore` edit had been written into somebody's repository.

## Consequences

- The first open of every plane costs a dialog, including the operator's own plane on a new
  machine. Bought deliberately, and it is one dialog per plane per machine for the whole life of
  that plane.
- An approved plane that changes its `env`, its `enabledPlugins` or a persona's tool grant asks
  again, which will happen on an ordinary `git pull` of a plane the team shares. That is the
  fingerprint working, and it will read as noise until somebody reads the diff it is showing.
- The operator's own edit to a persona's `tools:`, or to a script in its `bin/`, prompts until the
  plane is approved again, the same as a teammate's. purlis cannot tell the two apart, and a
  rule that trusted "edits made on this machine" would trust every chat on it.
- `purlis init`'s new default is a **deliberate divergence from the Python oracle**. `init` is
  ported (`scaffold/mod.rs`) and spec decision 15 requires every ported command to give the same
  result as Python on the same input; this one now will not, and Python is frozen (decision 17), so
  it does not follow. The differential scenario for `init` inside a repository must be recorded as
  an *intended* difference with this record named, never normalised until it stops failing.
- An operator who wants purlis's own plane shape — the repo *is* the plane — now types a flag.
  Every purlis developer will meet this, since purlis develops itself through a clone of itself.
- The machine-level record grows a third kind of entry (ADR 0034's list). Its rule was written to
  admit this one and to make the fourth argue for itself.
