# A project chooses among the extensions this machine approved

The operator asked on 2026-09-24 for per-project extensions: *"each project can have his own
extensions/themes/plugins enabled/disabled/configured"*, in a Project settings tab with a
**Shared** section (`charter.toml`) and a **Local** one (`charter.local.toml`). Grilled the same
day, he settled three things: Local overrides Shared; approval stays per machine (ADR 0041); and
the machine-wide extension list keeps working, with a project's choices taking precedence for
that project. This record is the precedence those three add up to, and why it is in the order it
is (charter-app#246, #253).

"Plugin" in the request is charter's **extension** (ADR 0041 kept the words apart:
`enabledPlugins` is Claude Code's list). A theme is enabled and disabled with the extension that
contributes it; charter keeps no separate theme choice per project.

## The decision

For one project, each extension's state is resolved in this order, by one function,
`charter_core::extension::project::resolve`, which every consumer asks:

1. **This machine's approval.** An extension this machine has not approved, or that changed
   since it was approved, contributes nothing in any project. Nothing a project file says can
   change that.
2. **Shared**, `[extensions.<id>]` in `charter.toml`.
3. **Local**, `[extensions.<id>]` in `charter.local.toml`, which overrides Shared **key by
   key**: `enabled` on its own, and each setting on its own.

With neither file naming it, an approved extension is **on**. So a project that says nothing
behaves exactly as every project did before this, and the machine-wide list keeps working.

| This machine | Shared `enabled` | Local `enabled` | State in this project | Decided by |
|---|---|---|---|---|
| approved | — | — | on | default |
| approved | `false` | — | off | Shared |
| approved | `false` | `true` | on | Local |
| approved | `true` | `false` | off | Local |
| not approved (or changed) | — | — | needs approval here | default |
| not approved (or changed) | `true` | — | **needs approval here, and off** | Shared |
| not approved (or changed) | `true` | `false` | off | Local |
| not installed | `true` | — | not installed here | Shared |

Off wins over *needs approval*: there is nothing to approve for a project that does not want it.

**Settings.** An extension's manifest may declare settings (`bool`, `text`, or `choice`), each
with a default. A project sets values in `[extensions.<id>.settings]`. Per key: Local's value if
the declaration accepts it, else Shared's if it accepts that, else the default. A value it would
not accept, or a key it does not declare, is ignored and said in the settings tab, never guessed
at. The resolved values are handed to the extension's program with each question, as `settings`,
**only when it declares any**, so every program approved before this is asked exactly the
question it was approved under. A manifest that declares settings must declare a program — a
setting nothing reads is a control that does nothing — and its settings are listed in the
consent prompt, because a committed file choosing a value a program acts on is a channel the
operator is owed a line about.

## Where each consumer asks

- **The Project settings tab** shows, in both sections, every extension this machine has
  installed and every one either file names, with its state and the file that decided it, and a
  control per key writing to the section it is in. It asks with a survey, so an extension that
  changed since its yes reads as needing approval.
- **The window's panels, views and themes.** The window still surveys the machine's extensions
  once (ADR 0041's named cost), and each project it holds asks `extensions_on` — the record and
  the project's two files, no directory read — and keeps of the survey only what it has on. The
  theme is the window's, so it follows the project in front; with no project in front, it is
  drawn from every approved extension as before.
- **The executor's gate** (ADR 0041 stage 2) reads the project's two files at the press, after
  this machine's record and before the fingerprint: a project that turned the extension off is
  refused with a sentence naming the file.

## Why this order

**Approval first, because a project's files travel.** `charter.toml` arrives with every clone.
If a project's `true` could stand in for this machine's yes, cloning a repository would be
enough to run a stranger's program on a click, which is the attack ADR 0041 exists to stop and
ADR 0022 refused for harness profiles. So a project may *want* an extension; only the operator
of this machine may *approve* one. That keeps ADR 0041's "an extension does not travel in a
plane": what travels is a preference over extensions the machine already trusts, never an
extension.

**Local over Shared, key by key,** because that is what the two files already mean: Shared is
the team's default and Local is this operator's say on this machine (ADR 0022's split, and the
harness `default`'s precedence). Key by key rather than table by table, so turning an extension
off locally does not silently drop the team's settings for it, and setting one value locally does
not turn it on.

**On by default,** because the machine-wide list is what an operator already has, and a
project that wants nothing different should not have to say so. The alternative, off unless a
project says on, would turn off every extension in every project on the day this shipped.

**`[extensions]` in `charter.local.toml` is allowed**, as the one table besides `[harness]`. The
rule it is an exception to says an ignored file must not change plane policy with no trace in
git. Turning an approved extension on or off for oneself is not plane policy: it changes nothing
a teammate's clone does, and it cannot reach past this machine's approval.

## What was rejected

- **A project approving an extension.** Rejected for the reason above.
- **A second list of extensions per project.** An extension is installed and approved per
  machine; a project that could name a directory to load one from would be a plane bringing an
  extension with it.
- **Choosing a theme per project.** Out of scope: a theme is turned on and off with its
  extension, and choosing among several on at once is still a preference charter does not keep.
- **Resolving in each consumer.** Four consumers with four copies of the order would drift; one
  function answers, and a precedence matrix in its tests pins it
  (`crates/charter-core/src/extension/project/tests.rs`).
