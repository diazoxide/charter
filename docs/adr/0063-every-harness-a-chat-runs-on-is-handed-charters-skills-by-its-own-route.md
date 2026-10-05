# Every harness a chat runs on is handed purlis's skills, by its own route

**Accepted 2026-09-26**, closing the gap [ADR 0061](0061-a-curation-action-is-a-chat-with-its-prompt-typed-and-never-sent.md)
recorded (SI-2c).

purlis ships nine skills in its plugin (`app/src-tauri/plugin/skills/`): `browser`, `handoff`,
`persona`, `secrets`, `update`, `working-in-a-clone`, `safe-remove`, `compact` and
`add-curation-action`. Until this, only a Claude Code chat had them, because only Claude Code
loads the bundled plugin (`--plugin-dir`, ADR 0050, ADR 0057). A Codex chat got hooks and an
opencode chat got the shim (ADR 0058), and neither got a skill. The curation actions' prompts
name skills, so a Codex or opencode chat opened on "Safe remove" was told to use a skill it had
never heard of.

The operator's standing rule is that purlis is harness-agnostic: a feature that reaches one
harness is one neutral model and one adapter per harness.

## The decision

**One source, one model, one adapter per harness.** The skills are the bundle's `skills/`
directory and nothing else: no harness gets a copy. `charter_core::skills` reads it into
`Skill { name, description, path }`, one per folder whose `SKILL.md` names both. Each harness
has a `skills::Route` (`Harness::skills`), and `Harness::state_hooks` takes it for the chat it
arms, for that chat alone. Nothing is written into any harness's config or into the
operator's tree.

| Harness | Route | How |
|---|---|---|
| Claude Code | `Plugin` | `--plugin-dir <bundle>` loads the plugin, and its `skills/` comes with it (unchanged) |
| opencode | `Config` | the shim is named in `OPENCODE_CONFIG_CONTENT` as `[<shim>, {"skills": "<bundle>/skills"}]`, and its `config` hook appends that directory to `skills.paths` |
| Codex | `Briefing` | the chat is started with `PURLIS_SKILLS_DIR=<bundle>/skills`, and its `SessionStart` briefing lists each skill: name, description and the path to its `SKILL.md` |

The briefing route is the neutral fallback: a harness with no skill mechanism purlis can reach
for one session gets the list, and reads a `SKILL.md` when a request matches it, with its own
tools. It is exactly what a harness with skills shows its model, so a prompt that names
`safe-remove` means the same thing on all three. **Codex uses the fallback**. Claude Code and
opencode load the skills natively, and are never started with `PURLIS_SKILLS_DIR`, so neither is
told about a skill twice.

## Why each route

### opencode: the shim's option and its `config` hook

Read at `v1.18.32` (`anomalyco/opencode`, the version installed here), and measured.

- `packages/opencode/src/skill/index.ts` discovers skills under `~/.claude/skills`,
  `~/.agents/skills`, the project's `.claude/skills` and `.agents/skills`, every config
  directory's `skill/` and `skills/`, and **every path in the config's `skills.paths`**.
- A plugin in the config can be `[spec, options]`, and the options are the factory's second
  argument (`plugin/index.ts`, `config/plugin.ts`). opencode's own built-in
  `customize-opencode` skill documents both, and says of the `config` hook: "cfg is the live
  merged config; mutate fields here".
- **A `skills` key in `OPENCODE_CONFIG_CONTENT` would replace the operator's own.** opencode
  merges its configs with remeda's `mergeDeep`, which replaces arrays; only `instructions`
  (and `plugin`, through its own path) is concatenated. Measured: with a global
  `opencode.json` naming one skills path, `OPENCODE_CONFIG_CONTENT='{"skills":{"paths":[…]}}'`
  made `opencode debug skill` list purlis's skills and drop the operator's.
- The shim's `config` hook appends instead. Measured, the same setup with the shim named
  `[<shim>, {"skills": …}]`: `opencode debug skill` listed purlis's nine and the operator's
  own. A path already there is not added twice.

The installed guard-only variant (`purlis plugin install --harness opencode`) has no `config`
hook: it is for opencode outside the app, which purlis does not arm with skills (ADR 0057's
scope).

### Codex: no switch for one session, so the briefing

Read at `rust-v0.147.0` (`openai/codex`), the version installed here.

- `codex-rs/ext/skills/src/host_roots.rs` builds Codex's skill roots from the config layers
  that have a folder: the user layer (`$CODEX_HOME/skills`, `~/.agents/skills`, and the
  bundled system skills), a trusted project's `.codex/skills`, `/etc/codex/skills`; then the
  `.agents/skills` folders between the project root and the working directory, and installed
  plugins' skill roots. The session-flags layer (`-c`) has no folder, so a `-c` flag adds no
  root.
- `skills` in the config schema holds only `bundled`, `include_instructions` and `config`, and
  `skills.config` turns a skill Codex already found on or off by path or name
  (`core-skills/src/config_rules.rs`). No key names a directory to scan.
- The extra roots `set_extra_roots` takes come from an app-server request
  (`app-server/src/request_processors/catalog_processor.rs`), not from anything a TUI chat is
  started with.

Every root is the operator's home, their repository, `/etc`, or Codex's plugin cache, and
ADR 0050 keeps chat launches from writing any of them. Pointing `HOME` or `CODEX_HOME`
elsewhere for one chat would move the operator's login, sessions and every shell's home with
it. So Codex is briefed.

The variable reaches the hook because a Codex hook runs with Codex's own environment
(`hooks/src/engine/command_runner.rs` adds the handler's variables and clears nothing). It is
not part of the hook's command, so the hooks' trust hashes are unchanged and the operator is
not asked to trust them again.

## Verified live

Each harness was started in a pseudo-terminal with exactly the arguments and environment
`Harness::state_hooks` gives the app, against a stand-in model server that logged every
request, with `HOME`, `CODEX_HOME` and opencode's XDG directories in a scratch folder.

- **Codex 0.147.0.** After the hooks were trusted, the first prompt's `SessionStart` hook
  briefed the chat, and the model's first request carried the listing with each `SKILL.md`
  path. The stand-in asked Codex to read `safe-remove`'s file, Codex ran it with its own
  `exec_command`, and the file came back.
- **opencode 1.18.32.** The model's system prompt listed `safe-remove` and the other eight in
  opencode's own `<available_skills>` block, beside a skill from the scratch global config's
  `skills.paths`. The stand-in called opencode's `skill` tool with `safe-remove`, and the
  tool returned the skill. No request carried purlis's listing.
- A listing of `~/.codex` and `~/.config/opencode` before and after the runs was identical.

## What this cannot guarantee

- **The briefing route is only as good as the briefing.** Codex fires `SessionStart` inside
  the first turn, and only once its hooks are trusted (ADR 0050's measurements), so an
  untrusted Codex chat has neither purlis's state nor its skills. And the briefing speaks only
  in a plane: outside one, `charter hook sessionstart` stays silent, as it always has.
- **opencode's `config` hook is the mechanism opencode documents, not a promise.** If a later
  opencode handed plugins a copy of its config, the append would be lost and a chat would have
  no purlis skills, with nothing refused. The shim's tests pin what the shim does, and this
  record says what was measured.
- **The variable is inherited.** A `claude` or `opencode` started by hand inside a Codex chat's
  shell inherits `PURLIS_SKILLS_DIR`, and its own `SessionStart` lists the skills that harness
  may already load. ADR 0062 already warns about a harness started that way.

## What was rejected

- **`skills.paths` in `OPENCODE_CONFIG_CONTENT`.** Measured to replace the operator's own
  skills paths for the chat, which is a change to their setup, not an addition to it.
- **`OPENCODE_CONFIG_DIR` pointing at the bundle.** opencode would scan `<dir>/skills`, but it
  also writes into every config directory it loads (a `.gitignore`, and an npm install of
  `@opencode-ai/plugin`), and the bundle is the app's own signed tree.
- **Copying the skills into `.codex/skills` or `.agents/skills` of each checkout**, as purlis
  mirrors a plane's own `.codex/skills`. Those are the operator's repositories, and a copy is a
  second source that goes stale.
- **Installing purlis's plugin into Codex's plugin cache.** ADR 0057 rejected it for the hooks
  it would double, and it would still be a write into `~/.codex`.
- **`-c developer_instructions=…` for Codex.** A session flag wins over the operator's own
  `developer_instructions` rather than adding to them.
- **Listing the skills in every chat's briefing.** Claude Code and opencode already show them,
  so they would be told twice.
