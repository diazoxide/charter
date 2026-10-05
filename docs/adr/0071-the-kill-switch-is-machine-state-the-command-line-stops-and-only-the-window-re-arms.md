# The kill switch is machine state: the command line stops, and only the window re-arms

**Accepted 2026-09-30** by the operator (ruling V22d), drafted for program-map ticket OV-1
(charter#639) from the `/code-review` of charter#747. It amends
[ADR 0034](0034-charter-keeps-a-little-state-outside-every-plane.md).

The kill switch stops every chat and shell purlis started, in every project and every window,
and starts no chat until the operator re-arms it. It is thrown from two places: **Stop all** on
the title bar, and `purlis stop --all` in any terminal. The second works with no app running,
and must reach an app that is running. The stop has to outlive a quit, too: a relaunch puts back
every chat its record names, so a stop that ended with the process would undo itself at the next
launch.

## The decision

**The switch is two files in purlis's config directory** (`$PURLIS_CONFIG_HOME`, else
`$XDG_CONFIG_HOME`, else `~/.config`, then `charter/`), beside `machine.json`:

- `halted`, an empty file whose existence means stopped;
- `kill-switch.jsonl`, one line per event: `at`, `event` (`stop`, `rearm` or `tamper`), `by`
  (`window`, `cli` or `app`).

**Either one says stopped**: the marker is there, or the journal's last entry is not a re-arm.
A marker removed by hand therefore re-arms nothing.

**The command line stops, and never re-arms.** Any agent can run a command, so a command that
lets agents start again would put a human's power in every agent's hands. `purlis stop --all`
exits non-zero, and says agents were NOT stopped, when it cannot write the stop.

**Only the window re-arms.** The window's control is the one caller of `halt::rearm`, and a
re-arm writes its journal line or does not happen.

**While the app runs, the switch in its memory is the authority.** It watches purlis's
directory (`notify`, non-recursive, filtered to the switch's two files). A stop that appears on
disk stops the app and ends every session. A marker that disappears while stopped is written
back and journaled as a `tamper`; it never re-arms. The re-arm and the watch take one lock, so
the re-arm's own removal of the marker is never read as a tamper. The watch also looks at the
files every 3 seconds when it has heard nothing, and watches the directory again when it is
made anew, so a watch the platform lost makes a stop slower to arrive, never lost (D-88l). A stop the app cannot write
to disk still stops the app, and the window says so on the title bar.

**What it stops, and what it lets through.** Every session's program gets an interrupt, then a
hangup, then a kill, to its whole process group, all sessions at once, well inside five seconds.
Shell tabs are stopped too, because a harness can be started by hand in one (ADR 0062). Every
chat and harness start is refused until re-armed: the operator's, a relaunch's, a handoff's, a
curation action's. **A shell the operator opens from the window is let through**, because
looking at what the agents did is a human act. A shell a record puts back is not.

## Why not in `machine.json`

ADR 0034's store is read and written whole under a lock, and every field in it has to be argued
onto its list. A stop is written by a second program, possibly while the app is writing the store
for another reason. It has to be read correctly by a process that parses nothing, and a stop is
the one fact where failing to read it must fail closed. A file whose existence is the answer
needs no lock and no parse. `reporting-consent` is the precedent.

## Amending ADR 0034

ADR 0034 lists the facts that may live outside every plane. This adds one, **whether this
machine's agents are stopped, and the journal of that switch**, held in two files of its own
rather than in the store. It passes ADR 0034's test. It is about the machine, and it is false
inside any one plane, because the switch spans every plane the app holds and a plane opened
after the stop must start nothing either. Deleting both files costs the operator the stop,
and every plane still opens complete.

## The residual

Both files belong to the operator's user. A process running as that user can edit both: it can
forge a re-arm in the journal and remove the marker while no app is running, and the next launch
believes it. While the app runs, its memory holds, and a removed marker is put back and
journaled. Protecting the switch from the agents it stops needs the switch held somewhere no
chat can write: the chat host of ADR 0068, with the chat sandbox denying purlis's config
directory (V16). That is follow-up work, not this record's.

## Not in scope

Headless agents, triggered chats, and chats a `purlisd` hosts are OV-2. Revoking tokens on
a stop is OV-4. A policy that throws the switch is OV-5.

## Ruled (V22, 2026-09-30)

1. **The CLI stops, and only the window re-arms.**
2. **Tampering is journaled and never re-arms.**
3. **The operator may open a new shell while stopped.**
4. **The stop state is its own file**, not a field in `machine.json`.
5. **Stop asks for no confirmation.**
6. **Re-arming restarts nothing.**
