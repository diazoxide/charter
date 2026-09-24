# A harness plugin is chosen per project, through one adapter per harness

On 2026-09-24 the operator asked for a harness's own plugins to be chosen per project, as
extensions are (ADR 0048), and set one constraint on the design: *"we are going to be
harness-agnostic - so we now supporting opencode or codex - and going to support other
harnesses - so we need to be ready other harnesses plugins."* This record is the model that
constraint produced and what each harness's adapter can and cannot do today (charter-app#274).

"Plugin" here means **the harness's own plugin**: a Claude Code plugin such as
`figma@claude-plugins-official`, or a Codex plugin such as `charter@charter`. It is not charter's
*extension*. ADR 0041 kept the two words apart, and this record keeps them apart too.

## The decision

**A harness-neutral model, and one adapter per harness.** A harness plugin is
`{ id, harness, name, source }`. `id` is the one the harness itself uses. `harness` is the
plane's word for the harness, which is a profile's `kind` (`claude`, `opencode`, `codex`).
`source` says where charter read it from. An adapter
(`charter_core::harness_plugin::Adapter`) knows one harness:

- **where it records what it has installed** (`record`), and **what that is**, read from the
  harness's own files and never written;
- **whether it can apply** a set of plugins to one chat that charter starts (`Support::PerChat`)
  or cannot yet (`Support::NotYet(why)`);
- **which plugins charter fixes** whatever a project says (`pinned`).

`ADAPTERS` holds one per harness charter knows, in the registry's order, and the settings tab
draws a group for every one of them. A harness whose adapter cannot apply shows
**"plugins for <harness> are not supported yet — <why>"**, lists what it has installed, and
says that every choice a file makes for it is ignored. No harness is left out without a word.

**Keys.** Both `charter.toml` (Shared) and `charter.local.toml` (Local) may hold:

```toml
[harness_plugins.claude]
"superpowers@claude-plugins-official" = true
"figma@claude-plugins-official" = false
```

**Precedence**, which is ADR 0048's, per plugin: Local over Shared. With neither file naming a
plugin it is **not set**, and the harness decides as it always did, from its own user and
project settings. One function, `harness_plugin::resolve`, answers it for the settings tab and
for a chat's start.

| Installed here | Shared | Local | In this project | A chat is handed |
|---|---|---|---|---|
| yes | — | — | not set | nothing: the harness decides |
| yes | `false` | — | off, from Shared | `false` |
| yes | `false` | `true` | on, from Local | `true` |
| yes | `true` | `false` | off, from Local | `false` |
| no | `true` | — | not installed here | nothing |
| pinned | anything | anything | the pin | the pin |

**Pins win over both files.** Claude Code has two, and they are the two values every chat the
app starts already carried. `charter-app@inline` is always on: it is charter's own plugin, and
it carries the hooks and the Bash guard, so a file that a chat can write must never be able to
switch the guard off. `charter@charter` is always off, by the operator's ruling of 2026-09-23.
The settings tab's save refuses a file that contradicts a pin. A file that contradicts one by
another route is ignored with a sentence, and the builder writes the pins last, so no input
can move them.

**Where it is applied.** `start::ready` resolves the set for the profile's declared kind and
carries it on `Ready::plugins`. `Harness::state_hooks` hands it to the harness. The listing is
read from **the chat's own environment**: a profile that sets `CLAUDE_CONFIG_DIR` or
`CODEX_HOME` for another account is listed against that account. The settings tab lists
against this process's environment, since it has no profile in front of it, and it names the
file it listed from, with a line saying that a profile that points the harness elsewhere is
listed against its own directory when its chat starts. **The harness's record is read at a
start only when a project file names one of that harness's plugins.** With nothing chosen, the
answer is the pins whatever is installed, so a project that says nothing reads nobody's home
directory. A chat on no profile, the operator's shell, is handed the pins alone, as before: it
has no declared kind to choose an adapter by, and `Harness::of_command` may not be relied on to
pick one.

## The adapters

### Claude Code: applies per chat

- **Lists** `plugins/installed_plugins.json` under `CLAUDE_CONFIG_DIR`, else `~/.claude`. This
  is the record `claude plugin list` reads: on the operator's machine, `claude plugin list
  --json` answered one row per install in that file, each with the same `id`, `scope`,
  `projectPath` and `installPath`. The only field it added was `enabled`, which comes from
  settings, and charter does not need it to list what is installed. A plugin loaded with
  `--plugin-dir` is not installed and is not listed. charter's own plugin is loaded that way
  and is shown as a pin. Read from the operator's 2.1.x install: `version`
  2, and `plugins` keyed by `<name>@<marketplace>`, each holding a list of installs with a
  `scope` (`user`, `project`, `local`). A plugin installed in several scopes is listed once,
  with its scopes as its source.
- **Applies** through `enabledPlugins` in the `--settings` JSON that each Claude Code chat
  already starts with. `--settings` merges with the settings already in force, and a key it
  names wins over the project's (measured on 2.1.276 and 2.1.280, `crate::harness`). A plugin
  the project leaves not set is not named, so Claude Code's own answer stands.
- **Not measured:** whether a plugin installed only at `project` scope for some other directory
  loads when `enabledPlugins` names it in a chat elsewhere. It is listed, because it is
  installed. Measuring it needs a live Claude Code session, which writes into `~/.claude`.

### Codex: not supported yet

- **Lists** `[plugins."<name>@<marketplace>"]` in `$CODEX_HOME/config.toml`, else
  `~/.codex/config.toml`. Codex's plugin docs and the operator's 0.147.0 install agree that
  this is where Codex records an installed plugin and whether it is on.
- **Cannot apply per chat**, measured on codex-cli 0.147.0 in a throwaway `CODEX_HOME` that
  held copies of a real `config.toml` and plugin cache. `codex debug prompt-input` showed which
  of the plugin's skills reached the model. `enabled = false` in `config.toml` removed them.
  `-c` did not, in any shape its parser takes: `plugins.<id>.enabled=false`,
  `plugins.<id>={enabled=false}`, `plugins={"<id>"={enabled=false}}`, and the reverse over a
  file set to `false`. Codex has two per-session switches near this. `--disable plugins` turns
  every plugin off, not one. `skills.config` does disable a single skill for one session
  (measured), but a plugin can also carry hooks and MCP servers, so disabling its skills is not
  turning the plugin off. The only per-plugin switch is `config.toml`, and charter never
  writes the operator's harness config. So this adapter reports *not supported yet*.

### opencode: not supported yet

- **Lists** the npm packages in the `plugin` array of `$XDG_CONFIG_HOME/opencode/opencode.json`
  (else `~/.config/opencode/opencode.json`), and every script in its `plugin/` and `plugins/`
  directories. opencode's docs name `plugins/`, and the operator's install has `plugin/`.
  `opencode.jsonc` is not read, because charter has no JSONC reader, so a plugin named only
  there is not listed.
- **Cannot apply per chat**, for two reasons. charter does not start opencode chats yet (spec
  decision 6). And opencode's plugin docs name no switch that turns one plugin off: it loads
  every plugin from every config file and plugin directory, together.

## Why this shape

**An adapter per harness, not a table per harness in one function.** The operator's constraint
is that the next harness should be one new adapter. A new harness implements `Adapter` and is
added to `ADAPTERS`. The model, the precedence, the keys, the settings tab and the start all
stay as they are.

**Not set is the default, not on.** An extension is on by default because the machine-wide list
is what the operator already has (ADR 0048). A harness plugin is the operator's own install,
and the harness already has an answer for it. charter says nothing about a plugin until a
project does, so a project that says nothing starts its chats exactly as before.

**Installed first.** A project file travels with every clone. It can choose among what this
machine installed, and it cannot install anything. Only the harness could honour a name it has
not installed anyway.

**Never write the harness's config.** Every value here is handed to one chat on its command
line, or it is not handed at all. Writing `~/.codex/config.toml` would change every Codex the
operator runs, outside charter as well. That is why Codex is *not supported yet* and not
supported by editing its file.

## What was rejected

- **Writing Codex's `config.toml`**, or a project `.codex/config.toml`. The first changes every
  Codex session on the machine. The second is a file in the repository that Codex reads only
  for trusted projects, and charter would be writing into the operator's tree.
- **Turning Codex plugins off through `skills.config`.** It reaches only the skills. A plugin
  that is "off" while its hooks still run is worse than one charter says it cannot switch.
- **Passing through a plugin this machine has not installed.** The chat would be handed a name
  the harness cannot load, and the tab would show it as in force.
- **One resolver shared with extensions.** ADR 0048's `resolve` puts this machine's approval
  first and defaults to on. Harness plugins default to not set and have pins. They share the
  order (Local over Shared, per key) and the `Source` type. The one function would have needed
  a flag for every difference.
