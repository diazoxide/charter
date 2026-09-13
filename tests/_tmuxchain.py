"""A tmux fake that understands a command LIST, because since #780 charter sends them.

`tmuxctl.write_all` and `commands_frame._split_all` batch every write nothing reads back
into one invocation — a launch's window dressing, a switch's kills, four `split-window`s
— so a fake that dispatched on ``"set-option" in cmd`` would be answered once for a
command carrying eight of them, and the suite's assertions about WHAT charter told tmux
would quietly stop seeing seven of the eight.

The fix is not to teach forty tests about chains: it is to make the fakes model the tmux
that is really there. :func:`answer` splits an invocation into the commands it carries,
runs each through the fake's own one-command handler, and folds the results the way tmux
folds them — so `fake.calls` goes on holding one entry per tmux COMMAND, exactly as it did
when charter sent them one at a time, and every existing assertion keeps its meaning. What
changes is only that a test may now also ask how many INVOCATIONS those commands cost,
which is the number #780 is about.

**Both halves of the fold are measured against real tmux, on 3.7c and at the 3.2 floor,
and neither is a guess.**

* **Output is concatenated, in order.** Four `split-window -P -F '#{pane_id}'` in one
  invocation print four ids, one per line, in the order given.
* **A failing command ABORTS the rest.** ``set-option @a 1 ; set-option nosuchoption 1 ;
  set-option @b 1`` sets `@a`, refuses the middle one and never sets `@b` — rc 1, third
  command not run. A fake that ran the whole list regardless would let a test pass on a
  batch real tmux would have abandoned half-way, which is the one thing batching can
  genuinely break.
"""

from __future__ import annotations

import subprocess

from charter.frame import tmuxctl


#: tmux's own global flags that take a value (`tmux.c`'s getopt string, ``2c:CDdf:lL:NqS:T:uUvV``).
_VALUED = frozenset({"-c", "-f", "-L", "-S", "-T"})


def head(argv: list[str]) -> list[str]:
    """`tmux` and its own global flags, `-L`/`-S` and the server among them — everything in
    front of the COMMAND: what `tmuxctl.server_argv` builds, what `tmuxctl.chain` puts in
    front of a list, and any global flag a builder adds after them.

    **Found by tmux's grammar, not counted.** The suite read this as `argv[:3]`, the length
    `server_argv` happened to build, until `-u` (#984) made it four — and a count copied
    into a test goes on answering after it stops being true: a verb read at `[3]` became
    the server's name, and `"-u" in argv` became true of every command charter captures,
    because `-u` is also `set-option`'s and `set-hook`'s own flag for a removal. Reading
    the head off the flags keeps a test asking what tmux would, and keeps it from agreeing
    with `chain` by sharing `chain`'s own count.

    Only tmux's OWN leading flags are looked at: `select-pane -L` and `capture-pane -S` are
    a command's flags spelled the same way, and a scan of the whole argv would cut there.
    A flag that takes a value takes the next word with it — `layout.session_argv` puts
    `-f <conf>` after the server, and a head that stopped at `-f` would hand the file back
    as the command.
    """
    cut = 1
    while cut < len(argv) and argv[cut].startswith("-"):
        cut += 2 if argv[cut] in _VALUED else 1
    return argv[:cut]


def command(argv: list[str]) -> list[str]:
    """The tmux command *argv* carries, with its own flags — *argv* after :func:`head`.

    What a test means by "the verb" (``command(argv)[0]``) and by "a removal"
    (``"-u" in command(argv)``), neither of which can be asked of the whole argv any more.
    """
    return argv[len(head(argv)):]


def commands(argv: list[str]) -> list[list[str]]:
    """One tmux invocation, back into the commands it carries — always at least one.

    The head is :func:`head`, which is exactly what `tmuxctl.chain` puts in front; every
    command in a chain shares it, so every command this hands back carries it too and a
    fake cannot tell a batched command from a lone one.

    A separator is a STANDALONE ``;`` argument and never a character inside another one —
    `tmuxctl.SEPARATOR`'s own measurement, and the reason `@charter_hatch`'s value
    (``select-pane -t %1 ; kill-pane -t %2``, one argv element) survives a round trip
    through here whole.
    """
    front, out, cur = head(argv), [], []
    for part in command(argv):
        if part == tmuxctl.SEPARATOR:
            out.append(front + cur)
            cur = []
        else:
            cur.append(part)
    out.append(front + cur)
    return out


def answer(one, argv: list[str], **kwargs) -> subprocess.CompletedProcess:
    """*argv* run through *one* — a fake's own single-command handler — tmux's way.

    A single command is handed straight through, so a fake wrapped in this behaves
    identically for every invocation charter has always made.

    For the fakes that stand in for `subprocess.run`, whose handler takes the argv first.
    :func:`answer_run` is the same fold for the ones that stand in for `tmuxctl.run`,
    whose handler takes the action phrase in front of it.
    """
    return _fold(one, argv, (), kwargs)


def answer_run(one, action: str, argv: list[str], **kwargs
               ) -> subprocess.CompletedProcess:
    """:func:`answer` for a fake patched in over `tmuxctl.run` rather than over
    `subprocess.run` — one whose handler is called ``one(action, argv, …)``.

    The action phrase travels UNSPLIT to every command in the list, which is what
    `tmuxctl.write_all` really does with it: the chained invocation carries one joint
    phrase, and a fake that recorded a different phrase per command would be describing
    a call charter never made.
    """
    return _fold(one, argv, (action,), kwargs)


def recorder(calls: list, *, stdout: str = "", rc: int = 0):
    """A stand-in for `tmuxctl.run` that records one entry per tmux COMMAND.

    For the dozen tests whose whole fake is "write down what charter said" — they hold a
    list and read options out of it by position (``a[-2]``, ``a[-1]``), which a chained
    invocation would answer for its LAST command only. :func:`commands` is what puts each
    command back in the list under its own entry, so those readings mean what they meant
    before #780 batched the writes.

    It returns a real `CompletedProcess` rather than ``None``, which the lambdas this
    replaces did: `tmuxctl.write_all` branches on the chain's return code, so a fake that
    answers nothing is a fake charter cannot use.
    """
    def run(_action, argv, **_kw):
        calls.extend(commands(argv))
        return subprocess.CompletedProcess(argv, rc, stdout, "")
    return run


def _fold(one, argv, before, kwargs) -> subprocess.CompletedProcess:
    parts = commands(argv)
    if len(parts) == 1:
        return one(*before, argv, **kwargs)
    stdout = []
    for part in parts:
        p = one(*before, part, **kwargs)
        stdout.append(p.stdout or "")
        if p.returncode != 0:
            # Everything after this one is not run — see the module docstring's second
            # measurement. The output of the commands that DID run is still returned,
            # which is what `_split_all` counts to learn how far the list got.
            return subprocess.CompletedProcess(argv, p.returncode, "".join(stdout),
                                               p.stderr)
    return subprocess.CompletedProcess(argv, 0, "".join(stdout), "")
