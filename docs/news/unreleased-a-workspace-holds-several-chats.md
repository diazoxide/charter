---
version: unreleased
headline: A workspace is a tmux session and a chat is a window in it, so one workspace can hold several harnesses and one dying does not take the others with it
---

Nothing on screen changes yet. Underneath, what a frame *is* has moved: a frame used to be
a tmux session named after the workspace and the launcher's pid, and it is now a **window**
named after a chat, in a session named after the **workspace**. `charter claude` in a
workspace that already has a chat open adds a second one beside it instead of starting a
second session.

**A chat id is allocated, not computed.** `charter.1`, `charter.2`, and the next free
ordinal each time — claimed with a `mkdir` that fails when the name is taken, so the claim
*is* the mutual exclusion and two launchers racing one workspace cannot both be told they
won. The design this replaces was `{workspace}-{some-hash}`, and both halves of it fail. A
hash of the only inputs available at creation is a counter in disguise — (workspace,
harness) is not unique by construction, two Claude chats in one workspace hash the same —
and a truncated hash collides silently into a shared `.charter/frame/<id>/`, where one
chat's token gauge, pane map and repaint clock overwrite the other's with nothing reporting
it.

**The dot is not cosmetic, and it is what keeps `.charter/frame/` bounded.** `reap` is the
only thing that removes frame state, and it keeps any directory whose name ends in a live
pid. Measured on this tree:

```
state._launcher_pid("myws-2")  ->  2       # the ordinal read as a process id
state._launcher_pid("myws.2")  ->  None    # the dot makes it not a claim
```

Pid 1 is `launchd`/`init` and pid 2 is a kernel thread on every Unix, so a `-{ordinal}`
tail would have made **every dead chat look live forever**. `None` is also the version
discriminator: an id charter can read a pid out of is a frame an older charter launched and
keeps the pid rule; one it cannot is a chat and takes its liveness from tmux's own windows.
No flag day, no migration, no new field on disk — old frames still launch, still report
liveness and still reap.

**One chat's harness dying no longer takes the workspace with it.** The `pane-died`
teardown hook ran `kill-session`; under tabs that ends every other chat in the workspace,
mid-turn, for a death that was not theirs. It runs `kill-window` now. Measured on tmux 3.7c
and on tmux 3.2 — charter's own floor, built from the release tarball and run: the dying
chat's window goes, every sibling window is still listed, the session is still listed, and
killing a session's *last* window still destroys the session, so a launcher's `attach`
returns exactly as it did.

**A window name is not an identity.** Charter names each chat's window, but liveness is
read from a window option (`@charter_chat`), not from `#{window_name}`, and that is
measured rather than argued. `new-window -n` does pin a name — it turns that window's
`automatic-rename` off — but with `allow-rename on` the pane's own output takes it anyway:

| tmux 3.7c and tmux 3.2 | before | after the pane prints `\033kPWNED\033\\` |
|---|---|---|
| `#{window_name}` | `api.3` | `PWNED` |
| `#{@charter_chat}` | `api.3` | `api.3` |

Read liveness from the name and a harness that renames its own window gets its state
deleted while it is still running. `automatic-rename` is on by default too, so any window
charter did not name follows whatever runs in it.

**Identity is per chat, and nothing was added to the list of variables charter will put on
a tmux command line.** A chat's window is created with `-e CHARTER_SESSION_ID=<chat>` and
`-e CHARTER_HARNESS=<name>`, which beats a session-wide `set-environment` inside the pane —
measured on 3.7c and 3.2. Everything charter already keys on the session id therefore
becomes per chat with no new code: `.charter/sessions/<chat>.persona`, `.workspace`,
`.tools` and `.gate`. Two chats, two personas, two tool ceilings, each enforced in its own
process.

The same measurement had a second half worth stating, because it decided something else: a
`run-shell` child — which every key binding's action is — reads the **session's**
environment and never the window's. So `F2` and the component toggle keys now carry
`#{@charter_chat}`, expanded in the presser's own window, and the palette that opens is the
one for the chat you pressed it in rather than for whichever chat launched last. A frame
from an older charter, whose window carries no such option, still resolves the way it
always did.

**The exit code follows the chat.** `$CHARTER_FRAME_EXIT` used to name one frame's own
`exit` file; a `set-environment` is session-scoped and a session holds several chats now,
so it names the frame **root** and the hook appends `#{@charter_chat}` itself. The
operator-controlled half of that path still travels out of band, as a single argv value
nothing re-parses; the half that is interpolated is a chat id in a closed alphabet with no
quote, `$`, backtick or space in it.

A tab is a container, not a boundary. Two chats in one plane run as the same user, under
the same plane, with the same vaults, and can read each other's files — a second tab does
not add that risk, it multiplies the number of processes that hold it.
