"""Suite-wide tripwire: no test may read a charter environment variable it did not declare.

`PersonaIso` isolates everything charter derives from the plane ROOT. `_planeguard`
isolates the state directory and the settings the developer's own ``charter.toml``
declares. Neither covers the third fixture a test can inherit without noticing: **the
shell the suite was launched from**.

That fixture is not hypothetical, and it is not stable. Run the suite from inside a live
charter frame — which is where the operator runs it — and sixteen tests fail that do not
fail anywhere else (#519, #521): ``$CHARTER_SESSION_ID`` holds the frame's id, ``$TMUX``
and ``$TMUX_PANE`` say a tmux is real, and each of those reaches a test that never said
whether it was inside a frame. Run it in CI, where none of them are set, and the same
tests pass. A suite whose answer depends on the terminal is not a suite; it is a reading
of the terminal.

**And the failure runs both ways.** On #525 the leak produced a false GREEN: both sides of
one assertion collapsed to the ambient ``$CHARTER_WORKSPACE``, so a mutation died with a
clean environment and *survived under the pin* — a test that quietly stopped testing. The
measurement behind #528 is that 108 of the suite's 168 ``patch.dict(os.environ, …)`` calls
omit ``clear=True``, across 38 unrelated files, so the survivors reach whatever the code
under test asks for.

**Two moves, and they close opposite directions.**

1. *The ambient values are removed*, once, before `charter.config` is first imported. What
   is gone cannot leak — not into a bulk ``dict(os.environ)``, not into a subprocess that
   inherits this process's environment, not into the 108 call sites that let the rest of
   the shell through. This is what makes the suite give the same answer inside a frame and
   outside one, and it takes effect for every test at once rather than 108 times.
2. *A targeted read of one of those names is REFUSED* while a test is running, unless that
   test declared what the name holds. Removal alone would only silence the RED: the test
   would still be asserting against a value it never chose, and the day its expectation
   coincides with the ambient one it goes quietly green for the wrong reason. Refusal is
   what makes instance 109 fail on the pull request that introduces it, on a CI runner
   that never had the variable set in the first place.

**What counts as a charter environment variable is a property, not a list.** It is any
name in charter's own namespace (``CHARTER_*``, ``CLAUDE*``), plus the terminal-identity
variables `charter.session` derives a session and a pane from, plus the terminal-GEOMETRY
variables `charter.tui` measures a width from, plus charter's FORMER namespace — each
asked of the constant production reads it from (`session._PANE_ID_VARS`,
`session._WINDOW_ID_VARS`, `tui.TERMINAL_SIZE_VARS`, `legacyenv.NAMES`) rather than copied
out of them, for the same reason `PersonaIso` asks `config.derive` instead of re-listing
twenty-five settings. A ``CHARTER_`` variable invented next month is guarded the day it is
invented, and #519's specific four are covered by the property rather than by being
spelled.

``$COLUMNS`` arrived here as the counter-example that proves the paragraph above is worth
writing: it is not in charter's namespace, it has no charter in its name at all, and it
flipped five tests (#544). What made it reachable was a list of spellings; what makes it
covered is asking the module that reads it. Its other half — the tty ioctl that
`term_width` falls through to once the variable is gone — is not an environment variable
at all and lives in `tests/_ttyguard.py`, with the
streams whose size it is asking about.

**The former namespace is here because the first version of this file forgot it, and the
way it forgot is the argument for the paragraph above.** That version spelled ``TMUX`` and
justified the spelling — "the one name with no constant to derive it from" — while missing
three names charter *already keeps in a constant*, and which are exactly the ones a
long-time charter developer still has exported. ``$EDM_WORKSPACE`` alone made two tests
fail on that developer's machine and pass in CI: `charter.legacyenv.warn` prints a
133-column stderr banner for each old name still set, at import of `charter.config`, and so
in every subprocess this suite spawns (#540). Scrubbed, not refused — charter never honors
their VALUES, so no test can be asserting the operator's terminal through one; what leaked
was the banner, and removing the name removes the banner.

**How a test declares.** Three ways, and all three are things tests already do:

* Derive from `tests._isolation.PersonaIso`. Its `setUp` calls :func:`unset_all`, which
  says "this test runs outside a frame, with no session id" — the answer CI would give.
  One call, for every test that mixes it in.
* Set the value: ``mock.patch.dict(os.environ, {"CHARTER_SESSION_ID": "sess-abc"})``. A
  name present in the environment is a name the test chose, so it reads normally.
* Say it is unset: ``clear=True`` on that same call (an emptied environment is a stated
  one), or :func:`unset` for individual names.

**Scoped to one test, and reset at its boundary.** Declarations live here rather than in
`os.environ` because `mock.patch.dict` restores by *clearing the whole mapping and
refilling it*, so a declaration stored as a value would be destroyed by the first
unrelated `patch.dict` in the same test. `unittest.TestCase.run` is wrapped to arm the
guard and to save/restore the declaration set around each test — save/restore rather than
plain reset, because a few tests run an inner `TestCase` inside their own body, and the
inner run must not disarm the outer one.

**Disarmed outside a test, deliberately.** Module import — `charter.config` resolving a
root, a test module reading something at collection — happens before any test declared
anything, and refusing there would be refusing the suite's own boot. The ambient values
are already gone by then, so those reads are deterministic; it is only the *loudness* that
waits for a test to be running.

**Raised as a `BaseException`**, for the reason `_planeguard.RealPlaneRead` documents:
charter is full of ``except Exception`` fallbacks that would turn this tripwire into a
degraded code path, and `unittest` records a `BaseException` against the test that raised
it, so the failure keeps its name.

**What this cannot see.** ``os.environb`` shares the same underlying data and bypasses the
mapping (nothing in charter uses it), and a subprocess resolves its own environment — but
it inherits this process's, which no longer carries the ambient values, so the two halves
agree. `setUpModule` and `setUpClass` run outside `TestCase.run` and are unguarded for the
same reason import is.
"""

from __future__ import annotations

import os
import unittest

from . import _planeguard


class AmbientEnvRead(BaseException):
    """A test read a charter environment variable without declaring what it holds."""


#: **Tier one — scrubbed.** Charter's whole namespace, as a PREFIX rather than an
#: enumeration, because a variable nobody has invented yet must not be able to arrive from
#: the operator's shell on the day it is invented. ``CLAUDE`` carries no underscore so that
#: ``CLAUDECODE`` — exported by the harness the operator runs this suite from — is inside
#: it too. Everything matching this is removed from the environment at install, so it can
#: neither reach a bulk ``dict(os.environ)`` nor a subprocess that inherits this process.
_PREFIXES = ("CHARTER_", "CLAUDE")


def _scrubbed_names() -> frozenset[str]:
    """The non-namespaced names scrubbed too, asked of the module that reads them.

    `session._PANE_ID_VARS` is the chain `session.terminal` walks (``TERM_SESSION_ID``,
    ``TMUX_PANE``, ``STY``, ``SSH_TTY``); `_WINDOW_ID_VARS` is the one it deliberately does
    NOT walk, scrubbed anyway because "deliberately not read" is a decision that can change
    and a scrub costs nothing if it is wrong.

    `legacyenv.NAMES` is charter's own former namespace — ``$EDM_HOME``, ``$EDM_WORKSPACE``,
    ``$EDM_PERSONA``, from before the rename. Outside ``CHARTER_``/``CLAUDE`` by spelling
    but squarely inside charter by ownership: `legacyenv.warn` runs at import of
    `charter.config` and prints a 133-column line to stderr for each one still set, which
    reaches every subprocess this suite spawns. Missing them cost two failures on a
    developer's machine that CI could not reproduce (#540). Asked of the constant, so a
    fourth rename is guarded on the commit that adds it —
    `test_no_test_reads_the_operators_shell.WhatIsGuarded` pins that the asking still
    happens.

    ``EDM_`` is deliberately NOT a fourth entry in :data:`_PREFIXES`. A prefix is what
    ``CHARTER_`` earns by being open — a variable nobody has invented yet must not be able
    to arrive from the operator's shell. charter's ex-namespace is closed in the other
    direction: no new ``EDM_*`` name will ever be invented, so a prefix would buy nothing
    forward and would reach sideways into whatever unrelated tool on the machine happens to
    share three letters.

    `tui.TERMINAL_SIZE_VARS` is ``$COLUMNS`` and ``$LINES`` — the SIZE of the terminal the
    suite was launched in, which is a fact about the operator's window and not about
    anything in this repository. `tui.term_width` reads the first, and at ``COLUMNS=40``
    with everything else already cleared the suite returned four failures and an error
    (#544). Asked of `charter.tui` because that is the module that reads it and because
    `tui` imports ``os``, ``re`` and ``unicodedata`` and nothing else — the same reason
    `legacyenv` exists as a separate module, stated in its own docstring: a tuple of
    strings was never what needed a plane, and `commands_frame`, which strips the same two
    names out of every child a frame starts, cannot be asked here because importing it
    resolves one.

    **Removing these two is only half of that defect, and the smaller half.** With
    ``$COLUMNS`` absent `term_width` falls through to `os.get_terminal_size()`, an ioctl on
    this process's stdout — so the scrub moves the reading from the shell to the tty rather
    than ending it. Measured at b3dbd54 with both variables unset: the same three modules
    give 3 failures and 1 error on an 80x24 pty at 40 columns and pass at 200. That half is
    closed by `tests/_ttyguard.py`, which owns the file descriptors and where the whole
    argument lives; this entry exists so that the shell's answer cannot reach a
    `dict(os.environ)` or a child process either.

    ``TMUX`` is the one name spelled out, and it has to be: it is tmux's variable, not
    charter's, read in half a dozen places to answer "is this process inside a tmux" with
    no constant to derive it from. Its pane counterpart arrives from `_PANE_ID_VARS`, so
    inventing a constant for one of a pair would buy nothing.
    """
    from charter import legacyenv, session, tui
    return frozenset(session._PANE_ID_VARS + session._WINDOW_ID_VARS + ("TMUX",)
                     + legacyenv.NAMES + tui.TERMINAL_SIZE_VARS)


def _loud_names() -> frozenset[str]:
    """**Tier two — refused.** The variables that are an IDENTITY the operator's own
    session exports, as opposed to a path or a knob.

    The distinction is what makes this a guard and not a chore, so it is worth stating
    precisely. Every name in tier one is scrubbed, so no test's answer can differ between
    two machines either way. What tier two adds is *loudness*, and loudness is only worth
    its cost where "unset" is a CLAIM ABOUT THE WORLD the test runs in — am I inside a
    frame, is this a real tmux, whose session is this, which workspace is pinned over every
    pointer. A test that reads one of those and finds nothing has asserted something about
    the operator's terminal without saying so, which is precisely the sixteen failures of
    #519/#521 and the false green of #528. A test that reads ``$CHARTER_WORKTREES`` and
    finds nothing has asserted charter's documented default, which is a fixture like any
    other.

    Derived, not spelled. `commands_frame._FRAME_IDENTITY` already exists to answer exactly
    "which variables must be THIS session's rather than whichever launcher happened to
    start the shared tmux server", and its own docstring commits the next such variable to
    that list — so a variable added there is guarded here on the same commit. The terminal
    half comes from `session._PANE_ID_VARS`/`_WINDOW_ID_VARS` plus ``TMUX``, and
    ``CLAUDE_CODE_SESSION_ID`` is `session.current`'s second rung, which
    `_FRAME_IDENTITY` has no reason to carry because a frame shadows it rather than
    exporting it.

    Resolved on the first test rather than at install: `charter.commands_frame` pulls in
    `charter.config`, and the scrub has to happen BEFORE anything resolves a plane.
    """
    from charter import commands_frame, session
    return frozenset(commands_frame._FRAME_IDENTITY
                     + session._PANE_ID_VARS + session._WINDOW_ID_VARS
                     + ("TMUX", "CLAUDE_CODE_SESSION_ID"))


_SCRUB_NAMES: frozenset[str] = frozenset()
_LOUD: frozenset[str] = frozenset()

#: Names this test has declared. Reset — saved and restored — around every `TestCase.run`.
_declared: set[str] = set()

#: Set by :func:`unset_all`: every guarded name is declared unset for this test, except any
#: the test also puts in the environment (a real value always wins over a blanket unset).
_all_unset = False

#: Armed only while a test is running. See the module docstring.
_active = False

_installed = False


def _in_namespace(key: str) -> bool:
    """Tier one: is this a name the scrub removes?"""
    return key.startswith(_PREFIXES) or key in _SCRUB_NAMES


def _explain(key: str) -> str:
    return (
        f"REFUSED: read of ${key}\n"
        f"{_planeguard._current_test()} read ${key} without declaring what it holds. That "
        f"is an identity the operator's own session exports — inside a charter frame "
        f"$CHARTER_SESSION_ID is the frame's id, $CHARTER_WORKSPACE outranks every pointer "
        f"and $TMUX says a tmux is real; in CI all three are unset — so what this test "
        f"asserts would be a reading of the developer's terminal rather than of a fixture. "
        f"Sixteen tests failed exactly this way inside a frame (#519, #521), and one went "
        f"falsely GREEN (#528). Two ways out:\n"
        f"  - derive the case from `tests._isolation.PersonaIso`, which declares the whole "
        f"charter environment unset for the test, the way it already re-derives every "
        f"config setting from a tmp plane; or\n"
        f"  - state what this test needs: "
        f"`mock.patch.dict(os.environ, {{\"{key}\": \"...\"}})` for a value, or "
        f"`tests._envguard.unset(\"{key}\")` (or `clear=True` on that same call) to say it "
        f"is UNSET.")


class _GuardedEnviron(os._Environ):
    """`os.environ`, with the session-identity variables refusing an undeclared read.

    A subclass of the real thing rather than a wrapper: `os.getenv`, `subprocess`,
    `tempfile` and `posixpath.expanduser` all look `os.environ` up on the `os` module at
    call time, so replacing the object is enough, and inheriting means every behaviour not
    named below — the `putenv`/`unsetenv` syncing that keeps subprocesses honest included —
    is CPython's own.

    Only *targeted* reads refuse. Iteration, ``len``, ``copy()`` and so ``dict(os.environ)``
    are untouched, because `mock.patch.dict` snapshots the whole mapping on entry and
    `commands_frame._frame_env` builds a child environment out of it: a bulk read that
    exploded would refuse the very calls that isolate a test. Those bulk reads are safe by
    construction — the ambient values were removed at install, so what they see is the same
    on every machine.
    """

    def __getitem__(self, key):
        if _active and not _all_unset and key in _LOUD and key not in _declared:
            raise AmbientEnvRead(_explain(key))
        return super().__getitem__(key)

    def __setitem__(self, key, value):
        # Setting a name IS declaring it — including when `patch.dict` restores the
        # mapping it snapshotted, which is why declarations are not stored as values.
        if _active:
            _declared.add(key)
        super().__setitem__(key, value)

    def __delitem__(self, key):
        if _active:
            _declared.add(key)
        super().__delitem__(key)

    def pop(self, key, *default):
        """``pop`` and ``del`` state that a name is unset; they never refuse.

        `MutableMapping.pop` is written in terms of ``self[key]``, so without this a test
        clearing a variable it does not want — ``os.environ.pop("TMUX", None)``, the most
        direct way there is of saying "not inside a tmux" — would be refused for reading
        the thing it was in the act of removing.
        """
        if _active:
            _declared.add(key)
        try:
            value = os._Environ.__getitem__(self, key)
        except KeyError:
            if default:
                return default[0]
            raise
        os._Environ.__delitem__(self, key)
        return value

    def clear(self):
        """An emptied environment is a stated one: every guarded name is now unset.

        This is the ``clear=True`` that #528 asks 108 call sites to add, and it must count
        as a declaration or the recommended fix would trip the guard. `mock.patch.dict`
        calls this on the way out as well, so the declaration outlives the block — which is
        harmless: what the block restored is the install-time environment, where every
        guarded name is already absent on every machine.
        """
        global _all_unset
        if _active:
            _all_unset = True
        super().clear()


def unset(*names: str) -> None:
    """Declare *names* unset for the rest of this test.

    For the case that has no value to patch in: a test that needs charter to answer "there
    is no session" or "this is not a tmux" states it here instead of inheriting it.
    """
    _declared.update(names)


def unset_all() -> None:
    """Declare the whole charter environment unset for the rest of this test.

    What `PersonaIso.setUp` calls, and the reason a test deriving from it needs no further
    thought: it runs as if launched from a shell that had never seen charter — which is the
    environment CI runs in, and the one every assertion in this suite was written against.
    A name the test then puts in the environment itself still wins.
    """
    global _all_unset
    _all_unset = True


def scrubbed() -> dict[str, str]:
    """What was removed from the environment at install, for a test that wants it back.

    Nothing in the suite needs this today. It exists so that a test which genuinely has to
    know what the operator's shell held — a doctor check about the real terminal, say — has
    somewhere honest to get it, instead of the guard being disabled.
    """
    return dict(_SCRUBBED)


_SCRUBBED: dict[str, str] = {}


def install() -> None:
    """Scrub the ambient values, replace `os.environ`, and arm the guard per test.

    Called once, at import of the `tests` package — and *before* `charter.config` is first
    imported, which is why `tests/__init__.py` calls it above the `_planeguard` import.
    ``$CHARTER_ROOT`` is one of the names removed here, and `charter.config` resolves the
    plane at ITS import: scrub afterwards and the whole suite would already be pointing at
    whatever plane the operator's shell pinned.
    """
    global _SCRUB_NAMES, _installed
    if _installed:
        return
    _installed = True

    # `charter.session` imports `os` and `re`, `charter.legacyenv` imports `os` and `sys`,
    # `charter.tui` imports `os`, `re` and `unicodedata`, and none of them imports anything
    # else — in particular not `charter.config` — so asking them for the pane variables,
    # the pre-rename names and the terminal geometry here cannot pull the plane resolution
    # in ahead of the scrub below. `legacyenv` is a separate module for exactly this
    # reason: those three names used to live in `config`, where nobody could ask for them
    # without resolving a plane, so this file did not ask and they leaked (#540).
    _SCRUB_NAMES = _scrubbed_names()

    for key in [k for k in os.environ if _in_namespace(k)]:
        _SCRUBBED[key] = os.environ[key]
        del os.environ[key]

    base = os.environ
    os.environ = _GuardedEnviron(base._data, base.encodekey, base.decodekey,
                                 base.encodevalue, base.decodevalue)

    original_run = unittest.TestCase.run

    def run(self, result=None):
        """Arm the guard for the duration of one test.

        The declaration set is saved and restored rather than merely cleared: a handful of
        tests construct a `TestCase` and run it inside their own body to observe what the
        harness does, and an inner run that reset the set would leave the outer test
        undeclared for everything after it.
        """
        global _declared, _all_unset, _active, _LOUD
        if not _LOUD:
            _LOUD = _loud_names()
        outer, outer_all, outer_active = _declared, _all_unset, _active
        _declared, _all_unset, _active = set(), False, True
        try:
            return original_run(self, result)
        finally:
            _declared, _all_unset, _active = outer, outer_all, outer_active

    run.__module__ = __name__
    unittest.TestCase.run = run
