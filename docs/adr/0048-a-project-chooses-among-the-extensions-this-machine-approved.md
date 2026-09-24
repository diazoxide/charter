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
same precedence — see [A project's theme](#a-projects-theme) below. **Amended 2026-09-24
(charter-app#280):** a workspace is a layer of the same order, between Shared and Local — see
[A workspace refines its project](#a-workspace-refines-its-project) below. **Amended 2026-09-24
(charter-app#281):** a workspace picks a theme in that order too, and has a colour of its own —
see [A workspace's theme and colour](#a-workspaces-theme-and-colour) below. **Amended 2026-09-24
(charter-app#308):** a `charter.local.toml` git would carry is no layer at all — see
[A Local file git would carry decides nothing](#a-local-file-git-would-carry-decides-nothing)
below.

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

## A workspace refines its project

Added by charter-app#280, the first of three (#279: extensions, then a workspace's theme and
colour, #281, then its harness plugins, #282). The operator, grilled 2026-09-24: *"per workspace
also configuration — as we already have workspace.json files"*; and the ruling on the order:
**Shared, then the workspace, then Local** — a workspace refines its project for the team, and
this machine has the last word.

For an extension in a workspace, the order above becomes:

1. **This machine's approval** — unchanged, and for the same reason: `workspace.json` travels
   with a LIVE workspace exactly as `charter.toml` travels with the plane.
2. **Shared**, `charter.toml`.
3. **The workspace**, `settings.extensions.<id>` in `workspaces/<ws>/workspace.json`.
4. **Local**, `charter.local.toml`.

Key by key, as between the two files: `enabled` on its own and each setting on its own, the
first layer that says (and, for a setting, says a value it accepts) deciding. With none saying,
an approved extension is on.

| This machine | Shared | Workspace | Local | State in this workspace | Decided by |
|---|---|---|---|---|---|
| approved | — | `false` | — | off | workspace |
| approved | `false` | `true` | — | on | workspace |
| approved | `true` | `false` | — | off | workspace |
| approved | — | `false` | `true` | on | Local |
| not approved | — | `true` | — | **needs approval here, and off** | workspace |
| approved | `false` | — (or an old manifest with no `settings`) | — | off | Shared |

**Why between the two, and not above Local.** A workspace is a team's arrangement of one task —
its manifest is committed when it is LIVE — so it is more specific than the project and still a
shared file. Local is the operator's own say on this machine (ADR 0022's split); letting a
committed workspace file overrule it would make the ignored file the only one that can lose to
something a teammate pushed. So a workspace narrows or names what the project said, and
`charter.local.toml` still has the last word.

**One resolver, one more input.** `Choices` carries the workspace's layer
(`Choices::read_in(root, workspace)`, `Choices::in_workspace`), and `resolve` reads the three
layers in order; nothing else changed about it, and no consumer has an order of its own. The
precedence matrix in `crates/charter-core/src/extension/project/tests.rs` has the rows above.

**The file's shape mirrors the TOML tables.** `settings` in `workspace.json` holds
`extensions.<id>.enabled` and `extensions.<id>.settings.<key>`, which is `[extensions.<id>]` in
JSON. It is read as the TOML table it mirrors and handed to the same reader, so a workspace's
value is refused, and ignored, in the same words as a file's. `null` reads as not set. #281 and
#282 add their tables (`theme`, `harness_plugins`) the same way: one more name the workspace's
settings may hold (`settings::workspace::READ`), one more reader handed the same table. #282
did so for `harness_plugins`: `harness_plugin::Choices::read_in` takes the layer, and ADR 0050
says what it means for a chat started in the workspace.

**Old manifests read as before.** A `workspace.json` with no `settings` — every one written
before this — is a workspace that says nothing, so its answer is the project's. Every writer of
the manifest mutates the document it read, so `snapshot`, `fork` and a clone's record keep the
settings (a fork inherits them).

**A save keeps the manifest's owner.** `charter_generated` is how the automatic writers tell
charter's manifest from a hand's (`crate::manifest`). The Workspace settings tab's save changes
only `settings` and re-stamps a manifest charter wrote, and writes one a hand wrote unstamped, so
saving a setting never hands an operator's manifest to the automatic writers.

**Where it is edited: a Workspace settings view tab, not a section of Project settings.**
charter-app#252 made Project settings one tab for one holder of settings — a plane — with one
section per file. A plane has as many `workspace.json` files as it has workspaces, so a section
per workspace would grow the tab without bound, and one section with a picker would be a tab
whose content depends on a control rather than on what it is. A view keyed by the workspace is
#252's own shape one level down: one tab per holder, one section for its one file, opened by the
same verb, deduplicated by the same `viewKey`, filed on that workspace's strip, and reached from
the workspace tab's menu and the palette (`workspace.settings:<ws>`), as Project settings is from
the project tab's. It holds the same Extensions group, asked for that workspace, so each
extension says which layer decided it — Shared, the workspace, Local, or the default.

**What asks for the workspace.** The window's filter on extension panels and views asks
`extensions_on` for the project **and the focused workspace**, so the side region and the view
buttons follow the workspace in front. The executor's gate is handed the workspace of the strip
the view is on, read at the press; a view the workspace turned off is refused with a sentence
naming `workspaces/<ws>/workspace.json` and the Workspace settings tab. The Project settings tab
still asks for the project alone. The theme stayed the project's until #281, below.

## A workspace's theme and colour

Added by charter-app#281. The operator: *"if user selected theme in project or local config —
workspace collor can add some filter and make same theme but with different collor"*.

**A workspace picks a theme in the same order.** `settings.theme.use` in its `workspace.json` is
one more layer of `theme::resolve`, in the place #280 gave the workspace: this machine's approval,
Shared, the workspace, Local. `theme::Said` carries it the way `Choices` carries the extensions'
(`Said::read_in`, `Said::in_workspace`), and the extension a pick names must be on **in that
workspace** — `resolve` is handed `extension::project::resolve`'s answer for the same project and
workspace. The fallback, the sentence and `theme.json`'s place are #273's, unchanged. `theme` is
the second name in `settings::workspace::READ`.

**The window's theme follows the workspace in front.** Each project reports its focused workspace
to the window, and the window asks `project_theme_drawn` and `extensions_on` for the pair. So a
workspace switch is a theme switch, live, and `theme.onDrawn` hands it to every terminal, as a
project switch does. #280 left the theme following the project alone; this is that fixed.

**A colour is a workspace's alone.** `settings.theme.colour` — one of eight palette names, or
`#rrggbb` whose hue is taken — is read from the workspace's layer and from nowhere else: a
project's `[theme] colour` is refused, because a colour exists to tell one workspace from another,
and one set for the whole project would tell nothing apart. It picks no theme; it has no
precedence to resolve. `theme::resolve` returns it with the pick, and `theme::colour_of` reads it
for a workspace that is not in front.

**The colour is a hue shift of the theme in force, and only of its accent and tab shades.** The
window turns `accent.base`, `accent.surface`, `focus.ring` and `tab.active` to the hue of the
workspace in front, on the whole window, and `layer.workspace`, `layer.chat` and `layer.selected`
as well on that workspace's own tab and on the chat strip, which holds its chats. Each workspace
tab carries its own tint whether or not it is in front: a small mark in its accent and its own
shade. The title bar's workspace carries the same mark. Text and the terminal are never touched.

The arithmetic (`app/src/theme/tint.ts`) turns the hue in OKLCH and then searches the lightness
for **the relative luminance the original had**, so every contrast ratio the theme cleared, its
tinted shades clear too, at every hue; `contrast.test.ts` runs every palette hue on both built-in
themes to hold it. A neutral grey, which has no hue to turn, is given a small fixed chroma. White
stays white, so charter-light's selected tab is white in every colour and says it with the mark.

Rejected:

- **A colour as a second theme, or a palette of accent tokens per colour.** Eight colours times
  every theme is a theme author's work multiplied, and an extension theme would have no colours
  at all. A hue shift works on any theme, including one charter has never seen.
- **Tinting the text or the terminal.** It is the only way a colour could break contrast, and
  the terminal is where the operator reads all day.
- **HSL.** Its lightness is not perceived lightness, so a yellow and a blue tint of one shade
  would read as two different shades. OKLCH keeps the step; the luminance search keeps WCAG.
- **A colour in the project's files, or overridable by Local.** See above: a colour is identity,
  not preference. A machine that wants another look picks another theme in Local.

## A Local file git would carry decides nothing

Added by charter-app#308. Everything above lets `charter.local.toml` hold `[extensions]` and
`[theme]` (and, by ADR 0050, `[harness_plugins]`) on one condition: the file stays on this
machine. That is the file's own rule — an ignored file must not change plane policy with no trace
in git — and the profiles loader has enforced it since ADR 0022 with one check,
`profiles::ignore_check`: the file is refused while git tracks it, would commit it, or cannot say.
The readers of the other tables did not ask it, so a `charter.local.toml` committed by mistake
still turned extensions on and off, picked a theme and chose plugins for every clone.

**The whole file, or nothing.** While the check refuses, no reader takes anything from the Local
file, and the order runs without that layer: this machine's approval, Shared, the workspace. The
check is per file, not per table, because what makes the file unsafe — it travels — is true of
every table in it.

**One way in.** Every reader of the two files reads them through
`charter_core::settings::layer_text(root, which)`, which hands a Local file the check refuses to
no reader — `LayerText::LeftOut`, carrying the check's sentence (charter-app#319, below); `extension::project::Choices::read`, `theme::Said::read` and
`harness_plugin::Choices::read` each call it, and so does every `read_in` through them. #308 was
three readers that each read the file themselves and so each forgot the check, so a test
(`crates/charter-core/tests/the_local_layer_has_one_reader.rs`) fails on production code that
reads the file by name anywhere but `settings.rs` and `profiles.rs`. A reader added later — the
plane's own save settings (ADR 0051) are the next — goes through `layer_text` or turns that test
red.

**What the tab shows.** The Project settings tab's Local section still shows the file, as a form
and as raw TOML, because that is where it is mended. Its standing refusals (_"charter does not take
this from the file as it stands:"_) carry the check's own sentence, which names the file and the
fix: _"git would commit charter.local.toml, so charter reads nothing in it until it is ignored —
charter reinit adds /charter.local.toml to .gitignore."_, or, for a tracked file, the same with
`git rm --cached` first. The sentence is the profiles loader's, widened from "the profiles in it
are refused" to "charter reads nothing in it", so `charter harness list`, `charter doctor` and the
tab say one thing about one state. Each extension, theme and plugin in the tab says which layer
decided it, and none says Local.

**And every group that shows what is in force says why** (charter-app#319). A value set in Local
and not applied would otherwise read "decided by charter.toml" with no reason beside it — worst in
Workspace settings, which has no Local section. So each reader keeps the check's sentence with
what it read (`Choices::local_left_out`, `theme::Said::local_left_out`), each answer the settings
tabs ask for carries it (`project_extensions`, `project_theme`, and each harness's group from
`project_harness_plugins`), and the Extensions group, the Theme group and each Harness plugins
group say it once, in Project settings' Shared section and in every Workspace settings tab. It is
the same sentence, from the same `git status`, as the Local section's refusal: there is one
wording, and the window writes none of it. The Local section's own groups leave it to that
section's head, which already says it once. **Only where the file would have decided something:**
a reader keeps the sentence only when the left-out file says something in its own table — an
extension, a theme pick, a plugin of that harness (`LayerText::left_out_where`) — so a Local file
that holds only profiles does not put the sentence in five groups that had nothing to lose.

**The cost** is one `git status` of one path per read, and only when the file exists; a plane with
no `charter.local.toml` runs no git. Rejected: caching the answer, since a file is ignored or
committed by an edit outside charter and a stale pass is the bug this closes.

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
a teammate's clone does, and it cannot reach past this machine's approval. That holds only while
the file stays on this machine, which is why a Local file git would carry is not read at all
(charter-app#308, above).

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
