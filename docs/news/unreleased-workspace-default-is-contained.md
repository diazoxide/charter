---
version: unreleased
headline: A committed `[workspace] default` is now a workspace name, not a path
---

`[persona] default` in `charter.toml` and `personas/.default` have been checked since
0.48.0: `charter.toml` is committed, so the value is a teammate's, and *"a path that
exists" was never the question being asked*. Their workspace twins — `[workspace] default`
and `workspaces/.default` — sat two rungs over on the same ladder and were checked by
nothing at all.

Both returned the committed value verbatim into `workspace_dir()`, which joins it onto
`workspaces/`. With `default = "../../esc"` in a plane's `charter.toml`, `charter workspace
current` printed `../../esc`, `charter workspace vision` printed a charter from outside the
plane, and `charter init` aimed its first clone there. `charter workspace default ../../esc`
was accepted and written, because the only gate was "does that directory exist" — and it
does; it is simply not a workspace.

Both rungs now ask the same question `charter workspace create` asks, and it is literally
the same function rather than a second copy of it: a value that is not a workspace name
degrades to the built-in `default`, which is the contract `[frame]` already keeps for every
key charter cannot make sense of. `charter workspace default` refuses a name its own reader
would discard, instead of writing a setting that silently does nothing.

**The name was never the only way in.** `workspaces/<ws>` could itself be a committed
symlink pointing out of the plane, with a perfectly legal name in front of it — and then
`workspace.md` and `workspace.json` inside it are ordinary regular files with nothing about
them to object to. `read_charter` and `read_manifest` now ask about the directory as well as
the file, which is what `persona.py` has always done and what `contain.file_refusal`
documents as its own precondition. A plane that symlinks a workspace directory to somewhere
outside `workspaces/`, `personas/` or its persona state will find those two reads refused;
that is the same trade personas have made since 0.48.0.

Honest scope: this is a containment break, not a confidentiality one. There was no
exfiltration channel — the bytes landed in your own terminal — it did not reach SessionStart,
and it could not reach a vault. `SECURITY.md`'s framing holds: guard rails against mistakes
and committed accidents, not against an attacker with shell access as your user.
