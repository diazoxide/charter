# The window may invoke only the commands an allow-list grants it

**Accepted 2026-09-24**, closing charter-app#276, the gap ADR 0047 left open.

Until this record, the app registered its commands with no app manifest. Tauri 2 checks its
access-control list (ACL) on every plugin command, but it checks the app's own commands only
once the app declares a manifest. Without one, any script running in the app's own page could
invoke any command the app registers, from any window. That includes the two commands that put
a secret's value where the window can reach it: a vault's reveal (30 seconds on screen) and its
copy (the clipboard, cleared a minute later). The Content-Security-Policy was the only guard in
front of them. A cross-site-scripting bug in any surface the window renders would have reached
them directly. ADR 0047 otherwise keeps every value out of the window, so this was the weak
point. No such bug is known. This record is hardening, and it describes the weakness by class
only.

## The decision

**One list of commands, in two classes.** `app/src-tauri/src/ipc_commands.rs` names every
command the window can invoke, split into two classes:

- `value_free`: nothing it returns is a secret's value.
- `vault_values`: `vault_secret_reveal` and `vault_secret_copy`.

The file contains only macros, so two places can read it:

- `lib.rs` hands the list to `tauri-specta`, which registers each command in the invoke handler
  and writes the TypeScript bindings.
- `build.rs` `include!`s the same file and hands the same names to Tauri's supported mechanism:
  `tauri_build::AppManifest::commands`, as tauri-build 2.6 provides it. That creates an
  `allow-<command>` permission for each command. `build.rs` then writes two permission sets
  from the two classes, `value-free` and `vault-values`, into `OUT_DIR`, and points
  `AppManifest::permissions_path_pattern` at them.

Registering a command, typing it and allowing it are therefore one edit, not three.

**Two capabilities, each granting one set to windows by label.**

- `capabilities/default.json` grants `value-free` to `main`, alongside the plugin permissions
  it already granted.
- `capabilities/vault-values.json` grants `vault-values` to `main` and nothing else.

The value-bearing pair has a capability of its own, so a window added to `default` later gets
the ordinary commands without also getting reveal and copy. A new window starts with no app
commands at all. Tauri refuses a command it has no grant for before any handler runs.

**Tests hold it together.** `app/src-tauri/src/ipc.rs` runs its tests against the real ACL. It
builds the app from `tauri.conf.json` and `capabilities/` on Tauri's mock runtime, with a
handler that accepts everything, so any refusal comes from the ACL. The tests check that:

- The names `tauri-specta` registers equal the list, in both directions.
- `main` may invoke every listed command.
- A command nobody listed is refused, even from `main`.
- A window labelled anything else may invoke none of the listed commands.
- Only `vault-values.json` grants the value-bearing commands, only to `["main"]`, and with no
  `webviews` or `remote` reach.
- `tauri.e2e.conf.json` names every capability file. A config that names its capabilities gets
  only those, so a capability it left out would be missing from the scenario tests without
  anyone noticing.
- The value-bearing class is exactly reveal and copy. Adding a command to that class means
  amending this record.

The ACL tests were seen to fail before the manifest existed: an unlisted command and a stray
window were both let through, and no capability granted the value-bearing pair on its own.

**The CSP, reviewed with the allow-list.** `tauri.conf.json`'s policy was
`default-src 'self'; style-src 'self' 'unsafe-inline'`. It is now:

```
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
object-src 'none'; base-uri 'none'; form-action 'none'; frame-src 'none'
```

- `script-src 'self'` has the same effect as the fallback it replaces. It is written out so
  that nobody adds `'unsafe-inline'` or `'unsafe-eval'` to the fallback thinking it only affects
  styles. Tauri adds its own hashes and nonces to this directive.
- `object-src 'none'`: the app embeds no plugins.
- `base-uri 'none'`: the page never sets a `<base>`. Blocking it means an injected one cannot
  redirect the page's relative script URLs.
- `form-action 'none'`: every form the app renders calls `preventDefault` and hands its data to
  a command. A form that actually submitted would navigate away from the page, so blocking
  submission costs nothing.
- `frame-src 'none'`: the app renders no frames. Extension views are drawn by the app's own
  components, not embedded as documents.

A test pins these directives and refuses `'unsafe-eval'` anywhere in the policy.

**What stays, and why.**

- `style-src 'unsafe-inline'` stays. xterm.js writes `<style>` elements at runtime, and those
  cannot carry Tauri's build-time hashes. Removing it would need nonce plumbing into a
  third-party renderer, and the terminal is the app's main surface. Inline styles are much less
  dangerous than inline scripts, which are still refused.
- `connect-src` is unchanged, inheriting `'self'`. Adding Tauri's custom IPC schemes to it
  changes how every command travels, which is not hardening. It is a separate change with a
  measurement behind it.
- `frame-ancestors` is not added. On Linux, Tauri delivers the policy in a `<meta>` tag, and
  browsers ignore that directive there. Nothing frames the top-level webview anyway.

## Consequences

- **A new command is one line in `ipc_commands.rs`**, under the class it belongs to. A command
  registered some other way is refused at runtime, and the tests above fail.
- **A new window is granted commands by name.** The test that refuses a stray window stays true
  until someone decides otherwise and edits a capability.
- **tauri-build writes `app/src-tauri/permissions/autogenerated/` at every build.** That is the
  documented behaviour of `AppManifest::commands`. The directory is generated, so it is ignored
  rather than committed.
- **A debug build explains a refusal.** In debug, Tauri names the command and the window it was
  refused for. A release build only says the command is not allowed.
- **The allow-list limits who can call a command. It does not stop a script already running in
  the main window.** Such a script can still invoke reveal and copy, as the page itself does.
  Against that, the defence is the CSP above, which gives an injected script fewer ways to run,
  together with each command's own rules from ADR 0047: the trace event, the 30-second reveal,
  and the clipboard clear. What the allow-list adds is this. The IPC surface is a list someone
  can review. A command nobody listed cannot be reached. Every window other than `main` is
  refused, and so is any window added later, until a capability grants it by name. The CSP and
  the allow-list are now two separate guards, not one guard behind another.
