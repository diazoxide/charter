# The operator installs charter's plugin for chats outside the app

**Accepted 2026-09-26**, by the operator's ruling on charter#374.

Every chat the app starts is armed for that session alone: Claude Code loads the bundled plugin
with `--plugin-dir`, and Codex gets `-c hooks.*` flags. A `claude` or `codex` the operator types
into a terminal got none of that. The only thing that guarded one was the retired Python
charter's `charter@charter` plugin, and only on a machine that still had it cached. The
operator ruled that terminal chats are supported: the app's own plugin is made installable for
them, one adapter per harness.

## The decision

**`charter plugin install` writes the harness's user configuration, because the operator ran
it.** [ADR 0050](0050-a-harness-plugin-is-chosen-per-project-through-one-adapter-per-harness.md)'s
rule, "never write the harness's config", still governs everything a project file or a chat
launch does. This ADR carves out one exception to it: a command the operator types, which
names every file it will change, shows them without writing under `--dry-run`, changes nothing
on a second run, and has `charter plugin uninstall` to take its writes back.

- **Claude Code** gets a copy of the bundled plugin in charter's machine directory
  (`~/.config/charter/plugin/`). The copy's hooks name the installing `charter` by its absolute
  path. The bundled hooks read `$CHARTER_HOOK_BINARY`, which only the app sets, and Claude Code
  treats a hook that cannot start as a non-blocking error, so an unchanged copy would let every
  tool call through. The copy is registered in the user `settings.json` as the `charter-app`
  directory marketplace, and `charter@charter-app` is enabled there. An app chat's
  `--plugin-dir` still wins the name `charter` (measured, 2.1.283).
- **Codex** gets only the Bash guard, as one `[[hooks.PreToolUse]]` group in
  `$CODEX_HOME/config.toml`. Codex runs config hooks beside the app's session flags, so a full
  install would run every state hook twice in an app chat. A doubled guard only refuses twice.
  Codex asks the operator to trust the hook, and it does not run until they do.
- **The retired plugin is never enabled.** Where a file the command writes enables
  `charter@charter`, the same write turns it off.

## What this costs

- The hooks run in every session on the machine, not only in a plane. Outside a plane the
  state hooks and the tool hooks answer nothing, so the cost is a process per hook.
- The copy does not follow an app update by itself: it holds the hooks and skills of the
  charter that installed it. Re-running the command refreshes it, and `charter doctor` is
  where a stale or broken copy is reported (charter#373).
- Turning `charter@charter` off in the user settings does not beat a plane whose project
  settings turn it on. Measured on 2.1.283, both plugins named `charter` then load, and both
  sets of hooks run. The plane's file is the operator's, so `charter doctor` reports it rather
  than this command rewriting it.

## What was rejected

- **Registering the bundle's own directory.** Its hooks need a variable only the app sets.
  Putting that variable in the user settings' `env` would override the app's own value, because
  settings `env` wins over the process environment.
- **Writing `enabledPlugins` into each plane at `init`/`reinit`.** That only covers planes, and
  a committed file would name a marketplace that exists only on machines that ran the install.
- **Codex's own plugin install** (`codex plugin add`). It copies the plugin into Codex's cache
  and brings every hook, which is exactly the doubling above.
