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
from typing import NamedTuple

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

#: The first tmux release that has `pane-border-indicators` AT ALL — one of the five
#: options `commands_frame._CHROME` pins so the operator's own `.tmux.conf` cannot decide
#: part of the frame's chrome (#514).
#:
#: **VERSION read out of tmux's own published CHANGES** (github.com/tmux/tmux, "CHANGES
#: FROM 3.2a TO 3.3"): "Add an option (pane-border-indicators) to select how the active
#: pane is shown on the pane border (colour, arrows or both)." **Confirmed by running both
#: sides**: `show -w pane-border-indicators` is rc 0 on a real 3.7c and answers `invalid
#: option: pane-border-indicators`, rc 1, on a real 3.2 built from source on this machine —
#: which is the whole of #716. `_CHROME`'s other four are rc 0 on that same 3.2, measured
#: the same way, which is why this is the only entry in that table floored above `FLOOR`.
#:
#: HIGHER than `FLOOR`, and a separate constant for `RESIZE_HOOK_FLOOR`'s reason exactly —
#: it is the same failure as that one, in an option rather than a hook. An operator on 3.2
#: is explicitly still allowed to launch (`below_floor_message` warns, does not refuse),
#: and pinning a name that tmux does not have is not degraded but REFUSED: :func:`run`
#: reports it, so before this constant existed **every launch on the supported floor**
#: printed `charter frame: styling the frame's own rules failed — … invalid option:
#: pane-border-indicators` to the operator's stderr. What is lost below the line is one
#: hostile setting left inherited — `pane-border-indicators arrows` marks the active
#: pane's borders with `←`/`↓` and its neighbours' not — on a tmux that has no such
#: setting to inherit. Nothing at all, in other words, which is why the gate is a version
#: and the option stays in the table.
BORDER_INDICATORS_FLOOR = (3, 3)

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
#: ABSOLUTE here because :func:`is_socket_path` discriminates on a leading `/` alone — a
#: relative path would be handed to tmux as a `-L` server NAME and quietly start a
#: brand-new server, which is the nesting `commands_frame` reads this to avoid. What the
#: absolute form does NOT establish is whose server it is: it is exactly how tmux writes
#: charter's OWN socket into charter's own panes, which is #812 — see
#: :func:`is_operator_socket`.
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


#: tmux's ``_PATH_TMP``, verbatim: a literal in its own source, and **not** ``$TMPDIR``.
#:
#: Measured rather than read, on tmux 3.7c and at the 3.2 floor, by starting a server
#: with ``TMPDIR`` pointed at a scratch directory and asking it where it actually is:
#: both answered ``/private/tmp/tmux-502/<name>``, and the scratch directory stayed
#: empty. The same pair of runs with ``TMUX_TMPDIR`` set DID build
#: ``<that dir>/tmux-502/``, so the variable tmux honours is that one alone. The
#: distinction is load-bearing on macOS, where `tempfile.gettempdir()` answers a per-user
#: ``/var/folders/…`` that tmux never puts a socket in.
DEFAULT_TMPDIR = "/tmp"


def socket_path(name: str) -> str:
    """The socket FILE tmux computes for ``-L name`` — ``<tmpdir>/tmux-<uid>/<name>``.

    **The other half of :func:`server_argv`'s split, and the reason it needs one.** That
    function turns a NAME into ``-L`` and a PATH into ``-S``, which is right for aiming a
    command; it says nothing about whether two spellings aim at the SAME server. They
    routinely do: `commands_frame.SOCKET` is ``charter``, and a process started in one of
    that server's own panes reads ``/private/tmp/tmux-502/charter`` out of ``$TMUX``.
    #812 is what happens when those two are compared as strings.

    **The result is RESOLVED, and that is not tidiness.** tmux calls ``realpath()`` on the
    base directory before it uses it, which on macOS turns ``/tmp`` into ``/private/tmp``
    — measured: a server started as ``-L <name>`` reports ``socket_path`` as
    ``/private/tmp/tmux-502/<name>`` on 3.7c and on 3.2 alike, while
    `tests/test_frame_tmux_integration.OP_SOCKET_PATH` builds the ``/tmp`` spelling of
    that same file and reaches the same server with it. Two spellings of one socket that
    differ only by a symlink must not read as two servers, so both sides go through
    `os.path.realpath` and no caller has to know which form it is holding.

    Nothing here touches the filesystem beyond that resolution: `os.path.realpath` leaves
    what does not exist lexical, so a socket directory tmux has not created yet still gets
    the right answer and asking the question starts no server.
    """
    base = os.environ.get("TMUX_TMPDIR") or DEFAULT_TMPDIR
    return os.path.join(os.path.realpath(os.path.join(base, f"tmux-{os.getuid()}")), name)


def same_server(a: str | None, b: str | None) -> bool:
    """Do *a* and *b* name ONE tmux server, whichever way each is spelled?

    The question `is_operator_socket` could not ask while it was a leading-slash test.
    Each side is resolved to the socket FILE it names — a name through
    :func:`socket_path`, a path through `os.path.realpath` — and the files are compared.

    Empty on either side is never the same server as anything, including another empty:
    ``""`` is not a spelling of a socket, it is the absence of one, and the callers that
    can produce it (`state.frame_server` reading a truncated marker) mean "unknown".
    """
    if not a or not b:
        return False
    return _resolved(a) == _resolved(b)


def _resolved(server: str) -> str:
    """*server* as the socket FILE it names, whichever of the two spellings it is.

    **One expression and no branch, because `os.path.join` already is the branch.** This
    read ``server if is_socket_path(server) else socket_path(server)``, and the deletion
    sweep found that `if` to be exactly equivalent to its else — correctly, and the reason
    is worth writing down rather than the line being quietly deleted:
    `os.path.join(base, tail)` throws every earlier component away when *tail* is absolute
    (CPython's documented rule), so :func:`socket_path` handed a socket PATH answers with
    that path unchanged and handed a `-L` NAME builds the file for it. Measured on this
    machine: ``socket_path("/private/tmp/tmux-<uid>/charter")`` is that string back.

    That property is pinned by name in `tests/test_frame_tmuxctl.py`, because it is the
    whole of why there is no branch here and a future :func:`socket_path` that stopped
    honouring it would silently turn every guest comparison into nonsense rather than
    failing.
    """
    return os.path.realpath(socket_path(server))


def is_socket_path(server: str | None) -> bool:
    """Is *server* spelled as a socket PATH rather than as a `-L` name?

    A leading ``/`` is the whole discriminator, and it is total rather than a heuristic: a
    socket path only ever reaches charter from `$TMUX`, which tmux writes absolute, and a
    `-L` name may not contain a separator at all — tmux joins that name onto its own
    socket directory to build the path, so a name with a `/` in it names a directory that
    does not exist.

    **This is the SPELLING, and it is no longer the same question as "whose server is
    it".** It was both until #812: a frame launched inside one of charter's own panes
    records the path spelling of charter's own socket, and every reader that asked
    :func:`is_operator_socket` was told it was a guest on somebody else's tmux. Splitting
    the two leaves `server_argv` exactly the test it always had — which flag aims at this
    string — and gives the ownership question a comparison that can see through a
    spelling.
    """
    return bool(server) and server.startswith("/")


def server_argv(server: str, *args: str) -> list[str]:
    """`tmux`, the flags that select ONE server, then *args* — every element separate.

    Charter talks to two different servers now, and this is the one place that difference
    is spelled: its own private one by NAME (`-L charter`, see `commands_frame.SOCKET`),
    and the operator's existing one by SOCKET PATH (`-S /private/tmp/tmux-502/default`,
    read out of `$TMUX` by :func:`operator_server`). :func:`is_socket_path` is the whole
    discriminator and says why it is total.

    Nothing is ever joined, here or anywhere downstream of here: a joined string is
    shell-interpreted by tmux and a separate argv is not (pinned against 3.7c, see
    `frame/layout.py`'s module docstring).
    """
    return ["tmux", "-S" if is_socket_path(server) else "-L", server, *args]


def is_operator_socket(server: str | None, *, own: str | None = None) -> bool:
    """Is *server* a tmux charter did not start — one it is a guest on?

    Two readers care and each would be wrong in its own way for a wrong answer.
    `frame/slots.py` drops the hotkey hint on a server charter is a guest on (a key table
    is server-wide in tmux, with no per-window form, so charter binds none there);
    `commands_frame._switch_workspace` refuses a workspace switch there, because inside an
    operator's tmux every chat is a `new-window` in the session they were already in and
    there is no session for another workspace to BE (§2.1).

    **It was a leading-slash test, and #812 is the bug that was.** The operator's own
    ``$TMUX`` on the machine this was written for is
    ``/private/tmp/tmux-502/charter,18923,83`` — charter's OWN socket, spelled as an
    absolute path, because the process reading it was started in one of charter's own
    panes. So a chat opened from a workspace tab recorded that spelling, every tab in it
    (including the one back) read it as a guest tmux, and the operator was told *"this
    chat is a window in your own tmux, where a workspace is not a session"* about a frame
    sitting on charter's private server. **Two spellings of one socket are not the same
    string.** The refusal itself is not the defect and is still here; what was wrong was
    concluding that this frame was in one.

    So the question is asked of the SERVER (:func:`same_server`) rather than of the
    string. *own* is charter's own private socket and defaults to it; it is a parameter so
    a caller that has already resolved which server it means can say so rather than have
    this reach for a module global.

    Imported lazily for `frame/builtin_actions._server`'s reason, and reached at CALL time
    rather than bound at import: `commands_frame` imports this module at load, so a
    top-level import would be a cycle — and a value bound once would not follow the
    `commands_frame.SOCKET` a test patches, which is exactly how a suite ends up proving
    the wrong socket's ownership.
    """
    if not is_socket_path(server):
        return False
    if own is None:
        from ..commands_frame import SOCKET
        own = SOCKET
    return not same_server(server, own)


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

    **Nothing in charter calls this today, and that is a statement of fact rather than a
    reservation** (#729). It had one caller, `commands_frame._say_on_screen`, which put a
    switch outcome on the client's status line; that outcome now goes into the frame's own
    state and is drawn by its attention panel into a pane charter owns, where no tmux
    parser is in the path. (This docstring also named `frame/palette.py` as a second
    caller. That was never true on this branch — `palette.py` does not import `tmuxctl` at
    all — and it is corrected here rather than left to be believed.)

    Every remaining place charter puts a derived value into a tmux grammar is closed by a
    narrower, construction-specific guard instead, which is stronger than doubling `#`: a
    pane id by `PANE_ID_RE`, a frame or chat id by `commands_frame._FRAME_ID_RE`, a session
    name by `state.workspace_prefix`'s alphabet, a hotkey by `instance._HOTKEY_RE`, a
    chrome or background value by a lookup table that answers `()` for anything it does not
    know. charter sets no `status-left`, `status-right`, `pane-border-format` or
    `window-status-format`, and has no `display-menu` or `display-popup` at all, so there
    is no format today whose text is not a module constant.

    It is kept rather than deleted because the measurements above are the reason those
    guards are shaped the way they are, and because the next surface that hands tmux a
    string built from a name will need exactly this. **A caller that appears must use it
    rather than re-spell it** — a second copy would be a second answer to "what may reach
    tmux's parser", which is #547's shape and which this repo has already paid for once.
    If no such surface arrives, this and its tests are a clean deletion.
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

#: How every captured child in this module is decoded, and the THIRD way one of them could
#: end badly (#828). `text=True` with no `errors=` decodes the pipe **strictly**, so a child
#: whose output is not valid UTF-8 raised `UnicodeDecodeError` — a `ValueError`, which
#: neither :func:`run`'s `TimeoutExpired` clause nor its `OSError` one catches — out of the
#: one function documented as never raising.
#:
#: **Which tmux output can do that, measured on 3.7c rather than reasoned about.** #828
#: files the sharp caller as `commands_frame._capture_transcript`, reading a HARNESS pane
#: during a quit, on the grounds that a pane holds arbitrary bytes. It does not reach
#: charter that way: under `LANG=C.UTF-8` and again under `LC_ALL=C`, a pane that prints
#: `\377` is stored in tmux's own screen as U+FFFD, and `capture-pane -p -e -N` hands back
#: valid UTF-8. tmux sanitises it first. Two other paths do reach charter, both measured on
#: the same binary:
#:
#: * A tmux USER OPTION round-trips its bytes untouched — `set-option -w @charter_chat` with
#:   a raw `\377` in it comes back out of ``list-windows -a -F '#{@charter_chat}'`` and out
#:   of `display-message -p` exactly as it went in. That listing is
#:   `commands_frame._chat_seats`, which `cmd_quit` asks BEFORE it kills anything, so the
#:   raise does land in a quit — one call to the left of where the issue put it. §3.3 is why
#:   it is not hypothetical: one tmux server serves every plane on the machine, so charter
#:   reads windows it did not create.
#: * tmux's own stderr echoes the raw bytes of an argument it refuses (`invalid window name:
#:   BAD\377NAME`), which is :func:`report_failure`'s input — so the decode could take
#:   charter down while it was REPORTING a failure.
#:
#: **Not a third invented return code beside :data:`TIMED_OUT`.** Those two say *charter
#: never got an answer*. Here tmux answered — rc 0, the whole listing — and only charter
#: could not read one byte of it. Reporting that as a refusal would name tmux in a failure
#: message for a command it ran correctly, and would throw away every row charter CAN read
#: over one it cannot. What the callers do with the replaced byte is what they already do
#: with anything they cannot read: the chat id fails `_FRAME_ID_RE` and its row is dropped.
#:
#: **And not `surrogateescape`, which was the shape #828 suggested.** It would stop the
#: raise HERE and move it: a lone surrogate has no UTF-8 encoding at all, so it raises
#: `UnicodeEncodeError` on any strict encode a caller later performs — `sys.stdout` under a
#: normal `LANG=en_US.UTF-8` is exactly that (measured: `sys.stdout.errors` is `strict`
#: there, and writing one raises), as is any `Path.write_text`. A function whose contract is
#: that it never raises has to hand back a value that does not raise either, or the promise
#: covers only its own frame. The round trip is not cashed anywhere either: the one caller
#: that PERSISTS the text encodes it with `errors="replace"` first, which writes `?` — 0x3F,
#: measured — so the bytes would be lost at the sink regardless, and lost as a character
#: indistinguishable from a question mark the agent really printed. U+FFFD says instead that
#: something unreadable was there, which is both true and what a terminal shows anyway.
#:
#: Read by the two captured children here and by nothing else. `interact` captures nothing
#: — it IS the operator's terminal — so it has no pipe to decode.
DECODE_ERRORS = "replace"


def _probe() -> str | None:
    """`tmux -V`'s output, or ``None`` when there is no tmux to ask.

    Decoded like :func:`run`'s pipes and for the same reason: this gates the whole launch
    and is asked before anything is drawn, so a `tmux` on `$PATH` answering in some other
    encoding would be a traceback in place of a frame. It already has an answer for a
    `tmux -V` it cannot parse, and "could not read it" is that same fact.
    """
    if not shutil.which("tmux"):
        return None
    try:
        out = subprocess.run(["tmux", "-V"], capture_output=True, text=True, timeout=5,
                             errors=DECODE_ERRORS)
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

    **Two things this used to say were measured false (#744), and both were the kind of
    wrong that stops an operator looking.**

    *"…until the frame is relaunched."* It is until they ask. `charter frame-resize` typed
    in the frame's own window does exactly what the missing hook would have done — it is
    the same `cmd_resize`, and nothing in it is version-dependent; only the HOOK that
    fires it is. Measured on ``~/.local/share/charter-testing/tmux-3.2``, a frame launched
    at 120x40, dragged to 80x24 and back::

        %1 5x120   %0 22x97   %4 22x22   %3 5x120   %2 5x120     <- stretched, and staying
        $ charter frame-resize
        %1 1x120   %0 34x97   %4 34x22   %3 1x120   %2 1x120     <- launch geometry, exactly

    Naming the remedy on the ceiling it answers is `doctor.check_harness`'s rule for its
    deficit lines, and it matters more here than there: below this floor that command is
    the operator's ONLY recovery, and a limit stated with no remedy reads as "nothing can
    be done".

    *"Everything else in the frame works."* At 80x24 on the same build, the sidebar is
    squeezed to **two columns** (`%4 18x2`, one truncated glyph per row) and the repo pane
    holds `⋯ too narrow for the repo table — 95 columns needed` — a line
    `frame/slots.py` writes to be transient, and which on 3.2 never settles because
    nothing re-measures. "The panels stretch" reads as cosmetic; a sidebar of stubs and a
    permanently apologising repo pane is not, and an operator told the first will not
    connect the second to their tmux version.
    """
    return (f"tmux {v[0]}.{v[1]} predates the `window-resized` hook "
            f"(tmux {RESIZE_HOOK_FLOOR[0]}.{RESIZE_HOOK_FLOOR[1]}+), which is what "
            f"restores each panel's fixed size after the terminal is resized. Resize this "
            f"terminal and the panels do not come back on their own: the sidebar can be "
            f"left a couple of columns wide and the repo pane can hold `⋯ too narrow for "
            f"the repo table` for good, because nothing re-measures. Run `charter "
            f"frame-resize` in the frame's own window and every panel is restored to its "
            f"launch geometry at once — that is the same command the hook would have "
            f"called, and it is the recovery on this tmux. Everything else works "
            f"unchanged.")


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

    **And output charter cannot read comes back as text, not an exception** —
    :data:`DECODE_ERRORS`, which is #828 and the third way this could end badly. That one
    is not a refusal: tmux answered, and what charter could not read is one codepoint of
    what it said.

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
                              errors=DECODE_ERRORS, timeout=timeout)
    except subprocess.TimeoutExpired:
        proc = subprocess.CompletedProcess(
            argv, TIMED_OUT, stdout="",
            stderr=f"tmux did not answer within {timeout:g}s")
    except OSError as e:
        proc = subprocess.CompletedProcess(argv, COULD_NOT_RUN, stdout="", stderr=str(e))
    if proc.returncode != 0 and report:
        report_failure(action, argv, proc)
    return proc


class Write(NamedTuple):
    """One fire-and-forget tmux command, with everything :func:`run` would need for it.

    A record rather than a bare `(action, argv)` pair because the two things a batch has
    to carry per command are exactly the two :func:`run` takes per command and neither is
    the same for every member of a group: `_reconcile_panels` batches a `set-hook -u`
    that must NOT be reported (unsetting a hook nothing set is rc 0 — measured on 3.7c
    and at the 3.2 floor — so the only way it fails is a pane that is gone) beside a
    `kill-pane` that must be. Flattening the two would either print a second sentence
    about one broken pane or swallow the sentence about it.
    """

    #: The phrase :func:`report_failure` prints for THIS command, if it is ever run on
    #: its own. Required for the same reason :func:`run`'s is.
    action: str
    #: The full argv, `tmux -L … verb …`, built by :func:`server_argv` like any other.
    argv: list[str]
    #: Whether a non-zero return from this one command is worth printing.
    report: bool = True


#: The two return codes :func:`run` invents rather than reads off tmux, and the two
#: :func:`write_all` will not replay on. Public because `commands_frame._split_all` reads
#: it too: the split batch cannot use :func:`write_all` (a replayed `split-window` is a
#: second pane) and so has to make the same call about the same two codes, and one
#: constant is what stops the two answering differently — for two different reasons, both of which end in
#: "one at a time buys nothing here".
#:
#: :data:`TIMED_OUT` is the load-bearing one: a wedged server has not told charter what
#: it did, so re-issuing would be repeating writes that may already have taken against a
#: tmux that is not answering — the one case idempotence cannot cover, because charter
#: cannot know it is repeating. :data:`COULD_NOT_RUN` is the opposite and is grouped with
#: it for economy rather than for safety: nothing ran and nothing can, so a replay is N
#: more failed `exec`s and N copies of one sentence about a tmux that is not there.
UNKNOWABLE = (TIMED_OUT, COULD_NOT_RUN)


def write_all(joint: str, writes: list[Write], *, env: dict | None = None,
              timeout: float = TIMEOUT) -> list[subprocess.CompletedProcess]:
    """Every write in *writes* — as ONE invocation when they all take, one at a time when
    any of them does not. One result per write, in order, whichever way it went.

    **What this buys is round trips, and they are most of what a launch and a switch
    spend.** Measured on this machine against tmux 3.7c, a `tmuxctl.run` costs ~5 ms of
    wall clock whatever it carries — the operator who filed #728 measured ~13.4 ms on
    theirs — and a four-panel switch made 58 of them. Two thirds of those read nothing
    back: window options, pane options, hooks, kills. tmux parses and executes a whole
    `;`-separated list server-side (:func:`chain`), so a group of them costs one.

    **A failing command ABORTS the rest of the list, and that is the whole reason this
    is not just :func:`chain`.** Measured on tmux 3.7c and at the 3.2 floor alike:
    `set-option @a 1 ; set-option nosuchoption 1 ; set-option @b 1` sets `@a`, refuses
    the middle one, and **never sets `@b`** — rc 1, and the third command is not run.
    So a chain is one command as far as failure is concerned, while charter's callers
    are written around each write failing on its own with its own sentence and its own
    consequence (`_dress_window`'s rules are decorative, `_install_resize_hook`'s hook
    is a capability ceiling, the launcher's five `set-environment`s each warn about a
    different thing). Collapsing those into one phrase would be a worse frame than the
    one the round trips buy.

    So: the chain is tried first with reporting off, and **the moment it returns
    non-zero every write is re-issued one at a time**, through :func:`run`, with its own
    action and its own *report*. The fast path is one invocation; the failure path is
    one wasted invocation plus exactly the calls, the codes and the sentences the caller
    would have got before this function existed.

    **Which makes idempotence the entry condition, stated here because nothing else can
    enforce it.** The replay re-runs writes that already took, so every *write* must be
    one that means the same thing twice: `set-option`, `set-hook`, `set-environment`,
    `kill-pane`. Never `split-window` — a replayed split is a second pane, and
    `_split_panels` reads its ids back for that reason and batches by hand.

    **A wedged server is not replayed** (:data:`UNKNOWABLE`). A timeout says charter
    never learnt what the server did, not that it did nothing, so re-issuing would be
    the one thing idempotence cannot cover: writes charter cannot know it is repeating,
    against a tmux that is not answering. The batch is reported once — and only if any
    write in it would have reported on its own — every write gets that same result back,
    and the caller degrades exactly as it does for any other non-zero return.
    """
    if len(writes) == 1:
        w = writes[0]
        return [run(w.action, w.argv, env=env, timeout=timeout, report=w.report)]
    argv = chain([w.argv for w in writes])
    if argv is None:
        # **Two cases, one answer, and no `if not writes` in front of them.** :func:`chain`
        # answers ``None`` both for a group spanning two servers — which it will not guess
        # between, and which nothing in charter builds, every caller deriving its argvs
        # from a single *socket* — and for an EMPTY group, which several callers really do
        # hand over (a frame with no doomed panel, no missing slot, no chrome to set). One
        # at a time is exactly right for the first and is a loop over nothing for the
        # second, so an early return for the empty case could not change an answer: the
        # deletion sweep reported it as a survivor and this repository deletes an
        # equivalent mutant rather than documenting it.
        return [run(w.action, w.argv, env=env, timeout=timeout, report=w.report)
                for w in writes]
    proc = run(joint, argv, env=env, timeout=timeout, report=False)
    if proc.returncode == 0:
        return [subprocess.CompletedProcess(w.argv, 0, stdout="", stderr="")
                for w in writes]
    if proc.returncode in UNKNOWABLE:
        # Reported only if any write in the group would have reported on its own — a
        # batch of `resize-pane`s that all opted out (`_apply_sizes`) must not start
        # printing over the agent's screen just because it is now a batch.
        if any(w.report for w in writes):
            report_failure(joint, argv, proc)
        return [proc for _ in writes]
    return [run(w.action, w.argv, env=env, timeout=timeout, report=w.report)
            for w in writes]


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
