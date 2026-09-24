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
contributes it. **Amended 2026-09-24 (charter-app#273):** a project also *picks* its theme, by the
same precedence — see [A project's theme](#a-projects-theme) below.

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

### How a manifest declares settings

A top-level `settings` array in `charter-extension.json`, at most 16 entries, each an object with
exactly these keys:

| Key | Required | Meaning |
|---|---|---|
| `key` | yes | Letters, digits, `-` and `_`, starting with a letter or digit; unique in the manifest. The name under `[extensions.<id>.settings]`. |
| `title` | no | What the form calls it; the key when absent. At most 200 bytes, nothing invisible. |
| `type` | yes | `bool`, `text` (one line, at most 200 bytes), or `choice`. |
| `choices` | for `choice` | The non-empty words it may be. |
| `default` | no | A value the setting accepts; else `false`, empty text, or the first choice. |

Any other key, a bad key or type, or a default the setting would not accept refuses the whole
extension, as every other manifest refusal does. Being in the manifest, the declarations are
inside the fingerprint.

## A project's theme

Added by charter-app#273, after the operator asked to *"pick a theme per project"*. A project picks
in `[theme] use`, in either file: `charter-dark`, `charter-light`, `system`, or
`<extension-id>/<theme name>`. It is resolved by one function beside `resolve`,
`charter_core::extension::project::theme::resolve`, which is handed `resolve`'s answer for the same
project, so the order above is kept and extended rather than copied:

1. **This machine's approval**, through `resolve`: an extension's theme is drawn only while its
   extension is **on** in this project — approved here, and not turned off by either file.
2. **Shared**'s pick, `[theme] use` in `charter.toml`.
3. **Local**'s pick, in `charter.local.toml`, which overrides Shared's. A value that is none of
   the four shapes is ignored with a sentence and Shared's is used, as a setting's is.

| Pick in force | Its extension in this project | Drawn |
|---|---|---|
| none | — | the window's own: `theme.json`, else the first theme from an extension it has on, else `charter-dark` |
| `charter-dark` / `charter-light` | — | that built-in |
| `system` | — | the built-in matching the operating system, followed live |
| `<id>/<name>` | on, and contributes `<name>` | that theme |
| `<id>/<name>` | off, needs approval, not installed, or no such theme | **`charter-dark`, and a sentence in the settings tab saying why** |

**The fallback is the built-in, not the next file down.** A pick that names a theme the project
cannot draw is a pick the operator made on purpose; drawing Shared's instead would look like
Local was ignored. Drawing the window's own starting theme and saying why in the tab under the
file that made the pick puts the reason where the choice is changed.

**A project's pick wins over the operator's `theme.json`.** It is the more specific answer —
this project, rather than this machine — and Local is also the operator's own say. A project that
picks nothing leaves `theme.json` in force exactly as before.

**`[theme]` in `charter.local.toml` is allowed**, as `[extensions]` is and for the same reason: a
theme is not plane policy, and a pick cannot reach past this machine's approval.

**Two inputs, one function, again.** The settings tab asks with a survey (`project_theme`), so a
pick an extension no longer contributes is said there. The window asks the record alone
(`project_theme_drawn`) and draws the built-in when the pick is not among the themes its one
survey found — so it can never draw what the tab would refuse. The window draws the answer for
the project in front; `theme.onDrawn` hands it to every terminal, so a project switch switches
both live (#216).

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
  **Two inputs, one function.** The settings tab gives `resolve` what a survey found, so an
  extension that changed since its yes counts as not approved; `extensions_on` gives it the
  record alone, where a yes in the record counts. They can differ only for a changed extension,
  and for that one the window holds nothing to filter — the survey left its panels, views and
  themes out — and the executor re-takes the fingerprint at the press. So the cheap answer can
  never put anything on screen or start anything the thorough one would not.
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
- **Choosing a theme per project** was out of scope when this was first written; charter-app#273
  added it, above, on the same precedence.
- **Resolving in each consumer.** Four consumers with four copies of the order would drift; one
  function answers, and a precedence matrix in its tests pins it
  (`crates/charter-core/src/extension/project/tests.rs`).
