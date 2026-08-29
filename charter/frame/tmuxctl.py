"""The only module in charter that runs tmux.

Kept alone so everything else — layout, panels, slots, the palette — is testable on a
machine with no tmux installed, and so there is exactly one place where the argv rule can
be broken. That sentence was aspirational for a while: `commands_frame.py` called
`subprocess.run(["tmux", …])` at thirteen separate call sites of its own, each repeating
`capture_output=True, text=True, timeout=15` and each catching nothing, so a wedged tmux
raised `subprocess.TimeoutExpired` straight out of the launcher. :func:`run` and
:func:`interact` below are what the sentence needed to become true: every tmux command
charter issues goes through one of them.

One honest exception remains, and it is not a tmux call by construction:
`frame/builtin_actions.py` starts a DETACHED child for every action the palette runs. Some
of those argvs are tmux commands (`detach-client`) and some are charter's own (`charter
frame-density`), and none of them may be waited for — §4g's fire-and-report — so they are
`Popen`ed rather than pushed through a helper that would have to lie about both what it
runs and when it returns.
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
from collections.abc import Mapping

from .. import util

#: The tmux the frame is known to work on, and the version charter warns below.
#:
#: **Not `display-popup`.** An earlier version of this constant justified 3.2 by naming
#: `display-popup` (3.2) as part of "the frame's interaction model" — that command
#: appears nowhere in shipped code, in any form; only in two comments. The number is
#: right for a different reason, so the reason is written down rather than the number
#: lowered to match a justification that was never real:
#:
#: * `run-shell -C` — what the escape hatch runs, and the one bind that must keep working
#:   when charter's own code does not (`frame/overlay.hatch_bind`) — arrived in 3.2
#:   exactly. This was `display-menu` (3.0) until the palette replaced the menu; the
#:   number did not move, and the reason it did not is that the line below was always the
#:   binding one.
#: * The exit-code mechanism rests on PANE-scoped hooks (`set-hook -p`, see
#:   `commands_frame._pane_died_write_hook_argv`), which followed pane options into tmux
#:   some time after 3.0. If those do not install, `cmd_launch` does not merely lose the
#:   exit code: a failed TEARDOWN hook makes it refuse to attach at all, which is a far
#:   worse outcome than the warning this floor produces.
#:
#: No tmux older than 3.7c exists on the machine this was written on, so where exactly
#: `set-hook -p` first appeared could not be confirmed by running it — and an unverified
#: reading of tmux's CHANGES is not a good enough reason to lower a floor whose only cost
#: when it is too high is one accurate warning, and whose cost when it is too low is a
#: frame that starts and then refuses to attach. Kept at 3.2, conservatively, with the
#: real reason recorded. Below it charter WARNS and launches anyway — nothing refuses,
#: and nothing disables the hotkey either; see :func:`below_floor_message`.
FLOOR = (3, 2)

#: The first tmux release with a `window-resized` hook AT ALL — the VERSION is read
#: from tmux's own published CHANGES file (github.com/tmux/tmux, "CHANGES FROM 3.2a
#: TO 3.3"): "Add a window-resized hook which is fired when the window is actually
#: resized which may be later than the client resize." (No pre-3.3 tmux binary was
#: available on this machine to install one directly and confirm it refuses the name;
#: the ERROR SHAPE below — "invalid option: <name>", rc 1 — WAS confirmed by hand
#: against a real tmux 3.7c, using a fabricated hook name it does not recognise, which
#: is generic `set-hook` argument-parsing text rather than anything specific to this
#: one hook's name.) HIGHER than `FLOOR` itself — not folded into it — because an
#: operator on 3.2 or 3.2a is explicitly still allowed to launch (`below_floor_message`
#: warns, does not refuse) and would otherwise see `set-hook … window-resized …` fail
#: with that same "invalid option" text on every single launch. Raising `FLOOR` itself
#: would refuse the whole frame over a gap that only costs cosmetic resize-drift, not
#: the escape hatch `FLOOR` protects — the two floors mean two different things and must
#: stay two constants.
RESIZE_HOOK_FLOOR = (3, 3)

#: The first tmux release in which `new-session` accepts `-e` at all — the VERSION is
#: read from tmux's own published CHANGES file (github.com/tmux/tmux, "CHANGES FROM 3.1c
#: TO 3.2"): "Add -e flag for new-session to set environment variables, like the same
#: flag for new-window." Confirmed present in this machine's 3.7c man page
#: (``new-session … [-e environment]``, "-e takes the form 'VARIABLE=value'").
#:
#: Numerically equal to `FLOOR` and deliberately a SECOND constant, for the same reason
#: `RESIZE_HOOK_FLOOR` is: it is a different fact about tmux and it will not necessarily
#: move with the other. `FLOOR` is where charter WARNS; this is where a `new-session`
#: carrying `-e` stops being a command tmux can parse — below it the flag is not degraded
#: but rejected outright ("unknown option"), which would take the whole launch down on a
#: tmux that `below_floor_message` explicitly still allows to launch.
#:
#: **What that costs below 3.2, stated exactly.** #411 stays unfixed there: the harness of
#: a SECOND frame on charter's shared private server goes on inheriting the FIRST frame's
#: `$CHARTER_SESSION_ID`, so `charter ws use` writes the first frame's workspace pointer
#: and hooks bump the first frame's version — that frame's panels do not follow it. (Its
#: PANELS still get their own id: `commands_frame._session_id_env_argv` ties that to the
#: session, which tmux applies to panes split later, so the two halves disagree.)
#:
#: An earlier version of this comment said that was "exactly as every frame did before
#: #411", and that was wrong in the direction that matters. Suppression (ADR 0019) keys on
#: the same variable, so on the id alone that operator's status line would ALSO have gone
#: blank against a frame that is not theirs — leaving no correct surface at all, where
#: before they at least had a correct footer. `state.is_live` takes the harness PANE as
#: well for exactly this band: below 3.2 the second frame's harness holds the first
#: frame's id but sits in a pane the first frame never recorded, so its status line keeps
#: drawing.
SESSION_ENV_FLOOR = (3, 2)

#: The first tmux release in which `split-window` (and `new-window`, and `respawn-pane`)
#: accepts `-e` — VERSION read from tmux's own published CHANGES ("CHANGES FROM 2.9 TO
#: 3.0"): "Add a -e flag to new-window, split-window, respawn-window, respawn-pane to set
#: environment variables." LOWER than :data:`SESSION_ENV_FLOOR`, because `new-session`
#: only grew the same flag two releases later — two flags, two facts, two constants, the
#: same reason `RESIZE_HOOK_FLOOR` is not folded into `FLOOR`.
PANE_ENV_FLOOR = (3, 0)

#: The first tmux release in which `pane-border-style` and `pane-active-border-style` are
#: PANE options rather than only window options — so a panel can be given its own edges
#: and the harness pane can keep the terminal's, which is what stops charter's surface
#: drawing a box around the one rectangle it does not own (ADR 0018).
#:
#: **Read out of tmux's own source at every release either side of the line, and confirmed
#: by running both sides.** `options-table.c`'s entry for `pane-border-style` is
#: ``OPTIONS_TABLE_WINDOW`` in 3.2, 3.3a, 3.4, 3.5, 3.6 and 3.6a, and
#: ``OPTIONS_TABLE_WINDOW|OPTIONS_TABLE_PANE`` in 3.7, 3.7a, 3.7c and master. Run against
#: a real 3.7c: ``set -p -t <panel>`` stores on the pane, ``show -p`` on the sibling and on
#: the harness both answer ``''``, and the window's own value is untouched. Run against a
#: real 3.2 built from source on this machine: the same ``set -p`` is **rc 0 and writes the
#: WINDOW**, ``show -p`` on a pane nobody set answers the window's value, and ``set -p -u``
#: removes the window's.
#:
#: **That silent-success is the whole reason this is a version gate and not a probe.** A
#: refused option is loud and already handled — `tmuxctl.run` reports it and the launch
#: continues. This one is not refused: below the line every per-pane write lands on the
#: window, so the LAST panel written would decide every rule in the frame, and an `off`
#: would `-u` away charter's own #514 border pin for the whole window. A probe that
#: measured it would have to perform exactly that write to find out. So the gate is on the
#: WRITE, not on the value, and below it charter uses the frame-wide answer
#: (`instance.border_bg`) it used before per-pane edges existed.
#:
#: HIGHER than `FLOOR`, and a separate constant for `RESIZE_HOOK_FLOOR`'s reason: an
#: operator on 3.2 is explicitly still allowed to launch, and what they lose here is one
#: shade on one row of cells, not a frame.
PANE_BORDER_FLOOR = (3, 7)

#: The session-scoped tmux environment variable carrying the interpreter that runs
#: charter from inside a frame — `"$CHARTER_PY" -m charter …`, never a bare `charter`
#: off `$PATH`. Defined HERE, not in either of the two modules that build text around it
#: (`commands_frame.conf_text`'s hotkey bind, which opens the palette), so the name the
#: writer sets and the name the reader spells cannot drift apart. See
#: `commands_frame._charter_py_env_argv` for why the value travels out of band at all.
CHARTER_PY_ENV = "CHARTER_PY"

#: The mouse wheel, and the root-table key charter rebinds so the wheel scrolls tmux's own
#: buffer rather than the terminal's (`commands_frame.conf_text`).
WHEEL_KEY = "WheelUpPane"

#: A left click on a pane, and the root-table key charter rebinds so a click on a PANEL
#: does not move the keyboard off the harness (#634, `commands_frame.conf_text`). tmux's
#: own default for it is `select-pane -t = \; send-keys -M`, which is the focus steal.
CLICK_KEY = "MouseDown1Pane"

#: Both of them, as the set of mouse keys charter binds for every frame on its own server.
#:
#: Defined HERE for :data:`CHARTER_PY_ENV`'s reason exactly — two modules need the same
#: names for two different jobs and neither may spell them itself.
#: `commands_frame.conf_text` WRITES a `bind -n` for each; `instance.component_tables`
#: REFUSES each to a component's ``key``, because both lines are emitted before the toggle
#: binds and tmux's key tables have no notion of a conflict — a later `bind -n` simply
#: replaces the earlier, so a component claiming one would delete charter's mouse handling
#: for every frame on the socket and `list-keys` would read back one line where two were
#: meant (#566). Two spellings of these names is that deletion waiting for the day they
#: stop matching.
#:
#: Both names pass `instance._HOTKEY_RE` — they are alphanumerics under twenty characters —
#: so this is a reachable claim rather than a theoretical one.
MOUSE_KEYS = (WHEEL_KEY, CLICK_KEY)

#: How long any one tmux ADMIN command may take before charter stops waiting for it.
#: Every command the launcher issues is a local, in-process request to a server on a
#: unix socket; none of them is supposed to take measurable time at all, so a command
#: still unanswered after this is a wedged server, not a slow one. See :func:`run`.
TIMEOUT = 15

_VERSION = re.compile(r"^tmux (\d+)\.(\d+)")

#: What tmux writes into `$TMUX` for every process it starts inside a pane:
#: `<socket path>,<server pid>,<session id>`, the session id being the NUMBER off
#: tmux's own `#{session_id}` (`$1` is written `1`). Measured against tmux 3.7c by
#: printing the variable from inside a real pane. The socket path is required to be
#: ABSOLUTE here because :func:`server_argv` discriminates on a leading `/` alone — a
#: relative path would be handed to tmux as a `-L` server NAME and quietly start a
#: brand-new server, which is the nesting `commands_frame` reads this to avoid.
_TMUX_ENV = re.compile(r"^(/[^,]*),\d+,(\d+)$")

#: The variable itself, named once so :func:`operator_server` and the tests that fake it
#: cannot drift apart.
_TMUX_ENV_NAME = "TMUX"

#: Every real pane id tmux's own `-P -F '#{pane_id}'` has ever reported (`%<digits>`,
#: confirmed against tmux 3.7c and never observed otherwise). Checked before a value read
#: off `split-window`'s stdout is trusted as a pane id, because two callers interpolate
#: one directly into text tmux later **re-parses as a command line**:
#: `commands_frame._resize_hook_argv`'s hook ACTION, and `frame/overlay.hatch_command`'s
#: escape-hatch option. That is the exact construction `commands_frame`'s module
#: docstring bans for `status_path`, and for the same reason: something interpolated into
#: a command tmux re-parses must be safe BY CONSTRUCTION, not merely safe because the one
#: program that currently produces it (tmux itself) happens to be well-behaved. A value
#: that fails this check arms nothing at all, rather than gambling that whatever the
#: string actually was cannot corrupt the parse.
#:
#: **`[0-9]`, not `\d`, and the difference is the property.** Python's `\d` is Unicode by
#: default: `re.fullmatch(r"%\d+", "%١٢")` is a MATCH, and so is the fullwidth `"%１１"`.
#: Neither is a pane id tmux ever minted, and neither is dangerous on its own — a Unicode
#: digit carries no meaning to any of the parsers a command string passes through — but
#: the check is here to say "this is tmux's own word for a pane", and a class that also
#: admits Arabic-Indic digits is answering a different question. The same
#: spelling-instead-of-the-property gap this repo keeps paying for, caught before it cost
#: anything rather than after.
#:
#: **Here rather than in `commands_frame`**, where it was first written, because it is a
#: fact about tmux's own vocabulary and this is the module that owns those; a second copy
#: beside the second caller is how one guard becomes two that disagree.
PANE_ID_RE = re.compile(r"%[0-9]+")


def operator_server(env: Mapping[str, str] | None = None) -> tuple[str, str] | None:
    """``(socket path, session target)`` for the tmux the operator is ALREADY inside.

    ``None`` means charter is not running inside one — which is the ordinary case, and
    also what a `$TMUX` charter cannot make sense of degrades to. Refusing a value that
    does not match :data:`_TMUX_ENV` exactly is deliberate: the session target this
    returns is interpolated into `new-window -t`, and a half-parsed `$TMUX` would aim
    that at whatever tmux decided "current" meant — someone else's session, on a launch
    whose entire point is not to disturb it. Falling back to charter's own private
    server nests, which is worse UX but never wrong about what it is talking to.

    Both halves are parsed here rather than one being re-queried from tmux, because both
    are already in the variable and a query is a second chance to get a different answer
    (the operator can switch a client's session between the two calls).
    """
    raw = (env if env is not None else os.environ).get(_TMUX_ENV_NAME, "")
    m = _TMUX_ENV.match(raw)
    if not m:
        return None
    return m.group(1), f"${m.group(2)}"


def server_argv(server: str, *args: str) -> list[str]:
    """`tmux`, the flags that select ONE server, then *args* — every element separate.

    Charter talks to two different servers now, and this is the one place that difference
    is spelled: its own private one by NAME (`-L charter`, see `commands_frame.SOCKET`),
    and the operator's existing one by SOCKET PATH (`-S /private/tmp/tmux-502/default`,
    read out of `$TMUX` by :func:`operator_server`). A leading `/` is the whole
    discriminator, and it is total rather than a heuristic: a socket path only ever
    reaches charter from `$TMUX`, which tmux writes absolute, and a `-L` name may not
    contain a separator at all — tmux joins that name onto its own socket directory to
    build the path, so a name with a `/` in it names a directory that does not exist.

    Nothing is ever joined, here or anywhere downstream of here: a joined string is
    shell-interpreted by tmux and a separate argv is not (pinned against 3.7c, see
    `frame/layout.py`'s module docstring).
    """
    return ["tmux", "-S" if is_operator_socket(server) else "-L", server, *args]


def is_operator_socket(server: str | None) -> bool:
    """Is *server* a tmux charter did not start — one it is a guest on?

    The same leading-slash test :func:`server_argv` turns into `-S`, named so the two
    places that care about the difference cannot answer it differently. The second is
    `frame/slots.py`: charter binds no hotkey on a server it is a guest on (a key table
    is server-wide in tmux, with no per-window form), so the bottom panel must not
    advertise one there.
    """
    return bool(server) and server.startswith("/")


#: tmux's own separator between commands sent in ONE invocation. A standalone argument,
#: never a character glued to another one: measured against tmux 3.7c that an argument
#: merely CONTAINING a `;` — `@charter_hatch`'s own value is
#: `select-pane -t %1 ; kill-pane -t %2` — is passed through whole and is not read as a
#: separator, so :func:`chain` is safe over exactly the commands charter already builds.
SEPARATOR = ";"


def chain(argvs: list[list[str]]) -> list[str] | None:
    """Several tmux commands as ONE invocation, so the SERVER runs all of them.

    **This exists because a command list that kills the caller's own pane cannot be sent
    one command at a time.** `frame/overlay.close_argvs` is `select-pane`, `kill-pane`,
    then re-arm the hatch — and the palette runs it from INSIDE the pane being killed.
    Measured against tmux 3.7c, three separate invocations from that pane: the first
    returned 0, and the process was gone before the second even answered, so the re-arm
    never ran and the overlay pane was left standing, unzoomed and unfocused, drawing a
    dead program. As one invocation, all three ran, 3 times out of 3 — tmux parses the
    whole list and executes it server-side, where the caller's death cannot interrupt it.

    ``None`` when the commands do not all address one server. That is not tidiness: the
    head is what selects WHICH tmux this reaches (:func:`server_argv`), and a chain built
    from two servers' argvs would send one server's commands to the other — charter's
    private socket and an operator's own being exactly the two it holds. Refusing costs
    the caller a fallback to one-at-a-time; guessing costs somebody else's session.
    """
    if not argvs:
        return None
    head = argvs[0][:3]
    out = list(head)
    for i, argv in enumerate(argvs):
        if argv[:3] != head:
            return None
        out += ([SEPARATOR] if i else []) + argv[3:]
    return out


def inert_format(text: str) -> str:
    """*text*, made inert against tmux's own format and style parsing. Three transforms:

    1. **`#` -> `##`.** A `#(...)`/`#{...}` in an unexpanded string EXECUTES or
       substitutes the moment tmux draws it — `display-message`'s own docs say the message
       is a format, and this was measured through charter's real production path with a
       git branch literally named `#(id>/tmp/...)`: the job ran, and the branch name never
       appeared on screen. `##` is tmux's own escape for a literal `#`; doubling every
       occurrence closes it regardless of how many times it is later collapsed for
       display, and pre-doubled payloads (`##(...)`, `####(...)`) were tried by hand
       against this fix and found no hole.

    2. **A leading `-` is read as a FLAG.** Verified by hand against a real attached
       client: tmux's own argument parser reads a value beginning with `-` as an
       unrecognised flag of its own (`unknown flag -m`) and refuses the whole command,
       rc 1 — not the "shown dim and may not be chosen" its docs describe. A leading space
       keeps the text intact and stops it ever reaching tmux's flag position.

    3. **A trailing `#` gets a trailing space.** Cosmetic only — nothing here executes
       either way — but verified: a value doubled from a single trailing `#` collides with
       the style-reset sequence tmux appends after it, rendering as literal
       `trailing#[default]` garbage. A trailing space breaks the adjacency.

    Lives here, in the module that owns every tmux argv charter builds, rather than beside
    one of the two callers: `commands_frame._say_on_screen` puts a switch outcome on a
    status line and `frame/palette.py` puts an action's refusal on the same one, and a
    second copy of this would be a second answer to "what may reach tmux's parser" — which
    is #547's shape, and which this repo has already paid for once.
    """
    text = text.replace("#", "##")
    if text.startswith("-"):
        text = " " + text
    if text.endswith("#"):
        text = text + " "
    return text


#: What :func:`run` reports as the return code of a command that never answered.
#: `timeout(1)`'s own convention, and deliberately not `1`: every caller in
#: `commands_frame` already branches on "nonzero", so a timeout degrades exactly the
#: way a refusal does, but the number still tells a reader which of the two happened.
TIMED_OUT = 124

#: What :func:`run` reports when tmux could not be started at all (`OSError` — the
#: binary vanished between `version()` and this call, a fork failure). 127 is the
#: shell's own "command not found", the same code `commands_frame.bypass` returns for a
#: missing harness binary.
COULD_NOT_RUN = 127


def _probe() -> str | None:
    """`tmux -V`'s output, or ``None`` when there is no tmux to ask."""
    if not shutil.which("tmux"):
        return None
    try:
        out = subprocess.run(["tmux", "-V"], capture_output=True, text=True, timeout=5)
    except (OSError, subprocess.SubprocessError):
        return None
    return out.stdout.strip() or None


def version() -> tuple[int, int] | None:
    """``(major, minor)``, or ``None`` when tmux is absent or unparseable.

    ``None`` is not "version zero": it says charter could not find out, which reads
    differently from "too old" and is answered with a different message.
    """
    raw = _probe()
    if not raw:
        return None
    m = _VERSION.match(raw)
    return (int(m.group(1)), int(m.group(2))) if m else None


def absent_message() -> str:
    return ("charter's frame needs tmux, which is not on this machine.\n"
            "  install:  brew install tmux   (or your package manager)\n"
            "  without:  charter <harness> --no-frame  runs the harness bare")


def below_floor_message(v: tuple[int, int]) -> str:
    """What an operator below :data:`FLOOR` is actually in for.

    An earlier wording said "the frame starts with the hotkey disabled". Nothing
    disables the hotkey — `commands_frame.cmd_launch` warns and continues, and
    `conf_text` still emits the bind unchanged — so that sentence described a mechanism
    that does not exist, in the one message an operator reads to find out what they are
    losing. What actually happens is written instead.
    """
    return (f"tmux {v[0]}.{v[1]} composes the frame, and charter still launches it — "
            f"nothing here is disabled. Below tmux {FLOOR[0]}.{FLOOR[1]} charter has "
            f"not verified two things it relies on: the escape hatch stays bound but "
            f"may do nothing (`run-shell -C`), and the pane-scoped hooks that carry the "
            f"harness's real exit code may fail to install, in which case charter says "
            f"so and declines to attach rather than risk a session nothing can end.")


def below_resize_hook_message(v: tuple[int, int]) -> str:
    """What an operator below :data:`RESIZE_HOOK_FLOOR` actually loses.

    **This floor sits ABOVE `FLOOR`, which is why it needs its own sentence.** An operator
    on tmux 3.2 passes `below_floor_message` cleanly — nothing warns, `charter doctor`'s
    frame row is green, `--probe` is a tick — and still has no `window-resized` hook, so
    every resize of their terminal leaves the panels stretched out of shape until the frame
    is relaunched. Folding this into `FLOOR` was considered and rejected where that constant
    is defined: raising it would refuse a frame that works over a gap that is cosmetic.
    Leaving it unsaid was the actual bug (#387) — it was reported nowhere at all except a
    `util.warn` printed 86 bytes before tmux switched the terminal to its alternate screen.

    Named here beside `below_floor_message` rather than written out at each of its two
    reading surfaces (`commands_frame.frame_ready`, `doctor.check_frame`), for the reason
    that message's own history records: two copies of one standing fact drift into two
    different facts.
    """
    return (f"tmux {v[0]}.{v[1]} predates the `window-resized` hook "
            f"(tmux {RESIZE_HOOK_FLOOR[0]}.{RESIZE_HOOK_FLOOR[1]}+), which is what "
            f"restores each panel's fixed size after the terminal is resized. Everything "
            f"else in the frame works; resize this terminal and the panels stretch, and "
            f"stay stretched until the frame is relaunched.")


def report_failure(action: str, cmd: list[str], proc: subprocess.CompletedProcess) -> None:
    """Name the command that failed and tmux's own stderr. Never silent.

    Correction 2's rule, in one place: `subprocess.run(cmd, env=env)` with the result
    thrown away is exactly how the pane-index bug `frame/layout.py`'s own module
    docstring describes would have shipped — a frame missing a panel, and nothing
    anywhere saying why. :func:`run` calls this for every non-zero return by default, so
    a caller has to opt OUT of reporting (and say why) rather than remember to opt in.
    """
    stderr = (proc.stderr or "").strip() or "(tmux printed nothing to stderr)"
    util.err(f"charter frame: {action} failed — `{' '.join(cmd)}`: {stderr}")


def run(action: str, argv: list[str], *, env: dict | None = None,
        timeout: float = TIMEOUT, report: bool = True) -> subprocess.CompletedProcess:
    """Run one tmux ADMIN command, captured and time-boxed. Never raises.

    *action* is the human-readable phrase :func:`report_failure` prints ("installing the
    exit-status hook") — required, not optional, because the failure message is the whole
    reason this wrapper exists and a call site that has not decided what to call itself
    has not decided what it is doing either.

    **A timeout comes back as a return code, not an exception.** Ten of the launcher's
    eleven admin commands run AFTER `new-session` has already started the harness
    detached; a `subprocess.TimeoutExpired` propagating out of one of them gave the
    operator a traceback, a filed charter crash report, an orphaned agent session and no
    reattach line. Every caller already branches on a non-zero return, so a wedged server
    now degrades down the same path a refusal does — see :data:`TIMED_OUT`.

    *report* is opt-out for the two callers that read the failure themselves: a query
    whose whole answer is "cannot tell" (`_query_pane_dead_status`), and one whose
    non-zero return is the ORDINARY case rather than a fault (`_live_sessions` against a
    socket no server has ever run on).
    """
    if not isinstance(argv, list):
        raise TypeError(f"tmux argv must be a list, got {type(argv).__name__}: {argv!r} "
                        "— see frame/layout.py")
    try:
        proc = subprocess.run(argv, env=env, capture_output=True, text=True,
                              timeout=timeout)
    except subprocess.TimeoutExpired:
        proc = subprocess.CompletedProcess(
            argv, TIMED_OUT, stdout="",
            stderr=f"tmux did not answer within {timeout:g}s")
    except OSError as e:
        proc = subprocess.CompletedProcess(argv, COULD_NOT_RUN, stdout="", stderr=str(e))
    if proc.returncode != 0 and report:
        report_failure(action, argv, proc)
    return proc


def interact(argv: list[str], *, env: dict | None = None) -> subprocess.CompletedProcess:
    """Run one tmux command that OWNS the operator's terminal — no capture, no timeout.

`attach` is not an admin command: it IS the session for as long as the harness runs.
    Capturing it would swallow the operator's own screen, and time-boxing it would kill a
    frame for the crime of being used. Split from :func:`run` rather than expressed as
    flags on it, so "this one has no timeout" is a visible property of the call site
    instead of a keyword argument easily copied to a command that needs one.

    It had a second caller — `display-menu`, which drew on an attached client and waited
    for a keypress. The palette replaced it, and the palette waits in a pane charter owns
    rather than in a tmux command, so this function is back to the one call it was written
    for.
    """
    if not isinstance(argv, list):
        raise TypeError(f"tmux argv must be a list, got {type(argv).__name__}: {argv!r} "
                        "— see frame/layout.py")
    return subprocess.run(argv, env=env)
