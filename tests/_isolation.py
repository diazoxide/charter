"""Shared test helpers: filesystem-isolated persona/config paths + a hook runner.

Persona/memory/hook code reads well-known paths off :mod:`charter.config` at call time
(``config.PERSONAS_DIR``, ``PERSONA_STATE_DIR``, ``ACTIVE_PERSONA_FILE``,
``WORKSPACES_DIR``), so redirecting those module attributes to a tmp dir isolates a test
completely from the real repo. Not a ``test_*`` module, so discovery skips it.
"""

from __future__ import annotations

import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from charter import config, instance, persona, root

from . import _envguard, _gitguard

#: Snapshotted so a test can still hand-patch one value; `config.DERIVED` is the source
#: of truth for WHICH values exist, so a setting added to `config.derive` is isolated the
#: day it is added rather than the day someone remembers this file.
_PATCH = config.DERIVED


class PersonaIso(unittest.TestCase):
    """Base case: every config path points into a throwaway tmp dir, restored after.
    ROOT is redirected too, so anything reading the repo (e.g. the git-based
    uncommitted-memory nudge) sees the tmp (a non-git dir), not the real checkout."""

    def setUp(self) -> None:
        # Many `cmd_*` handlers print progress (util.ok/info/warn/err → stderr; some
        # commands print results to stdout directly). Route both to a throwaway buffer by
        # default so calling a handler directly doesn't leak onto the real test-run
        # output — a test that needs to inspect what was printed enters its own nested
        # redirect_stdout/redirect_stderr, which captures correctly (these nest cleanly).
        self.enterContext(redirect_stdout(io.StringIO()))
        self.enterContext(redirect_stderr(io.StringIO()))
        self.tmp = Path(tempfile.mkdtemp(prefix="edm-test-"))

        # The environment, on the same terms as the plane below: ONE call, and the guard
        # is the only thing that knows WHICH names charter reads out of the shell. This
        # says "no session id, not inside a frame, not inside a tmux" — the answer CI
        # gives, and the one every assertion here was written against. A test that needs
        # a different answer states it with `patch.dict(os.environ, …)`, which wins.
        # Without this, `$CHARTER_SESSION_ID` from the operator's own frame reached
        # sixteen tests and failed them (#519, #521, #528).
        _envguard.unset_all()

        # ONE call. This used to re-implement all twenty-five derivations line for line,
        # and four of them were missing — so the suite wrote fixture data into the
        # developer's real `.charter/vaults.json` and orphaned every vault registered on
        # that machine. `config.derive` is now the only place that knows how a setting
        # follows from the root, and this asks it rather than copying it.
        self._orig = config.use(self.tmp)

        config.PERSONAS_DIR.mkdir(parents=True, exist_ok=True)
        # PRIVATE, and name-mangled on purpose (`_PersonaIso__restore`). `addCleanup` binds
        # a bound method by attribute lookup on the INSTANCE, so a subclass defining its own
        # `_restore` — the obvious name for "put my stubs back" — replaced this one:
        # `super().setUp()` registered the SUBCLASS's method, so NEITHER half below ran:
        # config was never restored and the tmp tree was never removed. `config.ROOT` then
        # pointed, for the rest of the run, at a directory that still EXISTS and is no
        # longer a plane — it holds whatever the fixture wrote (`personas/`, `.charter/`)
        # and never had a `charter.toml`, so `HAS_CONTROL_PLANE` is False and every setting
        # derives to its default. Measured, because "deleted" would send the next reader
        # hunting for ENOENT symptoms that never appear.
        # `test_secret_exec.SecretExecMode` did exactly this, and because discovery runs
        # alphabetically it left the 1186 tests that ran after it — 24.3% of the suite —
        # reading that non-plane. It is also why the two classes #459 is about failed when
        # run alone and passed in a full run: a root with no `charter.toml` derives
        # `UPDATE` to the `stable` default, which is exactly what they assumed.
        # `test_secret_cp_destination` had sidestepped the collision by hand, with a comment
        # warning the next person; a hazard that needs a comment in every subclass is one
        # the base class should have removed.
        self.addCleanup(self.__restore)

    def __restore(self) -> None:
        # An embedded plane's worktree root is a SIBLING of ROOT, so it survives the
        # rmtree below. Removed by name-prefix rather than by "is it outside tmp" — the
        # only path this ever creates is `<tmp>.worktrees`, and anything else sharing the
        # temp directory is not ours to delete.
        wt = getattr(config, "WORKTREES_ROOT", None)
        if wt is not None and Path(wt).name == f"{self.tmp.name}.worktrees":
            shutil.rmtree(wt, ignore_errors=True)
        config.restore(self._orig)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def make_persona(self, name: str, **meta) -> str:
        d = config.PERSONAS_DIR / name
        d.mkdir(parents=True, exist_ok=True)
        lines = "\n".join(f"{k}: {v}" for k, v in {"name": name, **meta}.items())
        (d / "persona.md").write_text(f"---\n{lines}\n---\n\n# {name}\n\ncharter body\n")
        persona.scaffold_memory(name)
        return name


class PlaneIso(PersonaIso):
    """`PersonaIso` whose throwaway root **is** a control plane — the base for a case that
    drives a hook handler.

    `PersonaIso` deliberately writes no ``charter.toml``, which is right for most cases and
    wrong for every case about a hook. Since #852 each handler in `hooks._HANDLERS` opens
    with `hooks._in_a_plane`, because outside a plane ``config.STATE_DIR`` is
    ``<cwd>/.charter`` and charter was leaving its session pointers, tallies and
    ``guard-seen.json`` in whatever repository the plugin happened to be enabled in. So a
    hook case on a root with no marker no longer asserts anything: the handler returns on
    its first line and every expectation about what it wrote or said is vacuously about
    silence.

    This is `make_plane` given a name, so that "this case is about a hook, therefore it
    needs a plane" is declared in the class list rather than repeated in twenty-seven
    ``setUp``\\ s — and so the next hook case inherits the precondition instead of
    discovering it as a mystery empty string.

    **Not for a case that is ABOUT the gate.** `tests/test_hooks.py`'s
    `TestGuardsAreScopedToAPlane` and
    `tests/test_a_repo_that_is_not_a_plane_gets_no_housekeeping.py` want a root that is not
    a plane, and they stay on `PersonaIso` where that is the stated fixture.
    """

    def setUp(self) -> None:
        super().setUp()
        make_plane(self)


def run_hook(fn, payload: dict):
    """Call a hook handler with ``payload`` on stdin; return parsed stdout JSON or None."""
    old = sys.stdin
    sys.stdin = io.StringIO(json.dumps(payload))
    try:
        buf = io.StringIO()
        with redirect_stdout(buf):
            fn()
    finally:
        sys.stdin = old
    out = buf.getvalue().strip()
    return json.loads(out) if out else None


class ReportIso(PersonaIso):
    """`PersonaIso` plus the two isolations upstream reporting needs.

    1. **Consent home.** Reporting consent is stored per-*human*, outside the plane, so
       `PersonaIso` alone does not isolate it — without this a test would read (or write!)
       the developer's real consent. Redirected via ``$CHARTER_CONFIG_HOME`` and
       deliberately NOT ``$XDG_CONFIG_HOME``: **`gh` keeps its own auth under
       XDG_CONFIG_HOME**, so hijacking that logs `gh` out mid-test and silently pushes
       `send` down its no-`gh` fallback path instead of the branch under test.

    2. **`gh` availability.** Stubbed true, so no test depends on whether whoever is
       running the suite happens to be logged in — the flakiness `test_forge_github.py`
       exists to avoid. A test covering the *unavailable* path re-patches it false.

    Nothing here stubs :func:`charter.report.gh` itself; each test does that, so a test
    that forgets cannot silently reach the network — it fails on a real `gh` call instead.
    """

    def setUp(self) -> None:
        super().setUp()
        home = Path(tempfile.mkdtemp(prefix="charter-consent-"))
        self.addCleanup(shutil.rmtree, home, True)
        self.enterContext(mock.patch.dict(os.environ, {"CHARTER_CONFIG_HOME": str(home)}))
        self.enterContext(mock.patch("charter.report.gh_available", return_value=True))
        self.consent_home = home


def no_background_refresh(case) -> None:
    """Stop this case's renders from forking charter's two background refreshers.

    One spelling of the three lines six modules already wrote by hand, with the same
    comment twice over::

        spawn = update.maybe_spawn          # never fork a network child from the suite
        update.maybe_spawn = lambda: None
        self.addCleanup(lambda: setattr(update, "maybe_spawn", spawn))

    `statusline.render` calls both spawners on its own path, and a case that renders a
    status line is almost never a case about refreshing anything. On a real machine neither
    fires: both are throttled by state in `config.STATE_DIR`, and the cache is fresh and the
    cooldown lock is held. A test's plane is a fresh temp directory, so the cache is always
    absent and the lock never exists — the throttles that make this rare in real use are
    exactly what a throwaway plane removes, and `test_statusline_brand` has said so in a
    comment for as long as it has had one: *"a temp STATE_DIR always looks stale"*.

    **Called by the case, never by `PersonaIso`, and the difference is the whole design.**
    A base class that stubbed these for every test would make `test_glstate_respawn`'s
    "must not raise" cases pass without running anything, and would do the same for the
    next such case somebody writes — the trap #542 names. What refuses the fork itself is
    `tests._planeguard.BackgroundCharterChild`, which fires at the `Popen` and therefore
    lets every one of those cases run their throttle logic and assert on it exactly as
    before. This helper is only for the cases that never wanted a child at all.

    Named for what it prevents rather than for what it patches: a case that grows a third
    background refresher gets it here, once, instead of in eighteen modules.
    """
    from charter import glstate, update

    for module, name in ((update, "maybe_spawn"), (glstate, "maybe_spawn")):
        patcher = mock.patch.object(module, name, lambda *a, **k: None)
        patcher.start()
        case.addCleanup(patcher.stop)


def no_update_check_in(plane: Path | None = None) -> Path:
    """Stop anything standing in *plane* from forking the newer-charter check, by giving the
    plane the cooldown lock a real machine already has.

    For a case that hands a plane to a CHILD charter — a panel in a tmux pane, a detached
    `charter frame-gather`, `charter hook sessionstart` run through the real entry point — or
    that lets its own process fork with `tests._planeguard.allow_background_children`.
    :func:`no_background_refresh` stubs the spawner in this process and reaches neither. Since
    #938 the frame's gather and SessionStart both start `charter _version-check`, a GET to
    PyPI, and a plane made a moment ago has no lock to stop them: one full run forked 18 of
    them from children, and `tests._planeguard` saw none, because it only watches this
    process.

    The lock is the one `update.maybe_spawn` touches before it forks, and its path is asked of
    production under `config.use(plane)` rather than spelled here. The child's own throttle
    then answers "attempted within the hour" and forks nothing. That is the throttle doing
    what it does in the field, not a stub standing in for it, so the gather and the hook still
    run every line they ran before.

    Not for a case whose subject is a plane with no state directory yet: planting a lock
    creates one. `test_the_state_directory_is_charters_to_choose` keeps its child off the
    network instead.
    """
    from charter import update

    snapshot = config.use(config.ROOT if plane is None else Path(plane))
    try:
        lock = update._lock_file()
        config.private_mkdir(lock.parent)
        config.touch_for(lock)
    finally:
        config.restore(snapshot)
    return lock


def shipped_frame(case) -> None:
    """Pin this case's ``config.FRAME`` to the frame charter SHIPS.

    **For a case whose subject is a default, and it exists because a default stopped being
    a constant.** `[frame] ok`/`warn`/`bad` made three of `frame/chrome.py`'s recipes read
    the plane, so "``ok`` is green" and "the served vocabulary is these seven numbers" are
    now claims about the plane the suite is running INSIDE — which on a developer's machine
    is charter's own control plane, and on the machine those keys were written for is a
    plane actively being tuned. Without this, an operator who writes ``warn = "blue"`` in
    their own `charter.toml` turns somebody else's test red, and the failure is about their
    config rather than about charter.

    That is #402/#492's rule — the suite reads the repo, never the machine — arriving
    through the newest door, and it is the door this whole file was built for.

    **Only for cases about the shipped answer.** A case that is about the keys DOING
    something patches `config.FRAME` with the words it means and asserts on those; this is
    for the ones that assert what a plane which said nothing gets. `clear=True`, so a key
    the ambient plane set and `FRAME_DEFAULTS` does not name cannot survive into the case.
    """
    patcher = mock.patch.dict(config.FRAME, instance.FRAME_DEFAULTS, clear=True)
    patcher.start()
    case.addCleanup(patcher.stop)


def child_plane_env(case, **extra: str) -> tuple[Path, dict]:
    """A throwaway plane, and the environment that points a CHILD charter at it.

    The way out `tests._planeguard.RealPlaneSpawn` names for a test that genuinely wants
    to run charter as a subprocess. ``$CHARTER_ROOT`` wins outright in `root.find_root`,
    so the child resolves THIS plane wherever it is standing — which is the point, because
    ``python3 -m charter`` normally needs a cwd of the checkout to import the tree under
    test, and that cwd is exactly what used to resolve the developer's own plane instead.

    A ``charter.toml`` is written, so the child finds a plane rather than raising and
    falling back to its cwd — the fallback being the hazard, not the fix.

    Returns the plane too, so a case can assert on what the child left in it.
    """
    plane = Path(tempfile.mkdtemp(prefix="charter-child-plane-"))
    case.addCleanup(shutil.rmtree, plane, True)
    (plane / root.MARKER).write_text("schema = 1\n")
    return plane, {**os.environ, root.ENV_VAR: str(plane), **extra}


def make_plane(case, body: str = "schema = 1\n") -> Path:
    """Turn a `PersonaIso` case's throwaway root into a REAL control plane, and re-derive.

    `PersonaIso` hands every case a root; it deliberately does not put a ``charter.toml``
    at it, so `config.HAS_CONTROL_PLANE` is False and every setting derives to its default.
    That is right for most cases and wrong for any case driving a path charter gates on
    having a plane at all — `glstate.maybe_spawn` and `update.maybe_spawn` both refuse to
    fork a background refresh without one, because outside a plane `config.STATE_DIR` is
    ``<cwd>/.charter`` and there is nowhere legitimate for the child to cache.

    **Re-derives rather than setting the flag by hand.** ``config.HAS_CONTROL_PLANE = True``
    over a root with no marker is a fixture that tells this process one thing and the
    child process charter is about to spawn another: the child reads ``$CHARTER_ROOT``,
    finds no ``charter.toml`` there, and goes looking for a plane of its own. On the
    machine this was written on that walk landed on the operator's live plane (#527). A
    plane a test claims to have should be one a subprocess can also find.

    Safe to call after `PersonaIso.setUp`: `config.use` snapshots, and the snapshot the
    base class already took is the one `restore` puts back.
    """
    (case.tmp / root.MARKER).write_text(body)
    config.use(case.tmp)
    return case.tmp


def point_config_at(case, root: Path) -> Path:
    """Point every derived setting at *root* for one case, restoring all of them after.

    What ``mock.patch.object(config, "ROOT", root)`` was standing in for in the four `init`
    fixtures, and it was standing in badly. Patching the one name looked like isolation and
    was a coincidence: `cmd_init`'s helpers all take *root* as an argument, so the other
    twenty-odd settings were simply never read — while still pointing at the developer's
    real plane throughout.

    It stopped being survivable when `cmd_init` began re-deriving where the marker lands
    (#858). `config.use` updates every `DERIVED` name; a patcher scoped to ``ROOT`` then
    puts ``ROOT`` back and leaves the other nineteen in a temp directory that `addCleanup`
    has already deleted — measured, on `tests/test_init.py`, and the WORSE half of #402's
    shape: `config` reporting the real plane's root beside a `PERSONAS_DIR` that no longer
    exists, for every test that runs afterwards.

    So the snapshot is taken over the same set `config.use` writes, which is the set
    `config.derive` produces — a setting added there is covered here the day it is added.
    `PersonaIso`-based cases need none of this; they already snapshot in `setUp`.
    """
    case.addCleanup(config.restore, config.use(root))
    return root


#: What most profile fixtures want in `charter.local.toml`: one profile whose `env` names a
#: path under `~`, because expanding that at the `exec` is the launcher's own job, and one
#: of another kind, because "a profile is a way to run ONE kind" is a rule with two sides.
DECLARED_PROFILES = """
[harness.claude-work]
kind = "claude"
command = ["claude"]
env = { CLAUDE_CONFIG_DIR = "~/.cw" }

[harness.codex-pinned]
kind = "codex"
command = ["npx", "-y", "@openai/codex@0.140.0"]
"""


def declare_profiles(case, text: str = DECLARED_PROFILES) -> Path:
    """Give *case*'s throwaway plane a `charter.local.toml` git would not carry. Its path.

    Four things a profile fixture needs, and every one of them is a trap by itself — which
    is why they are one helper rather than four copies:

    * **the file**, which is the only place a profile may be declared;
    * **a real git repository with the ignore line**, because `profiles.ignore_check` makes
      a real `git --no-optional-locks status` — the one subprocess profile code runs — and
      a declared profile is refused wherever git would carry that file. A fixture that
      skipped it would be testing the refusal, not the profile;
    * **`$GIT_CEILING_DIRECTORIES`**, so that walk stops above the plane. A temp directory
      inside somebody's own checkout otherwise answers for THEIR repository: green here,
      red on the machine next door, which is the shape `CONTRIBUTING.md` calls "your
      machine is not the runner";
    * **`$HOME`**, so a test asserting where `CLAUDE_CONFIG_DIR = "~/.cw"` expanded to can
      name the answer instead of running the same `expanduser` the code did and agreeing
      with itself. It is `case.home` afterwards.

    `make_plane(case)` first: profiles are read off a plane, and `instance.load` wants the
    marker.
    """
    home = case.tmp / "home"
    home.mkdir(exist_ok=True)
    case.home = home
    case.enterContext(mock.patch.dict(os.environ, {
        "HOME": str(home),
        "GIT_CEILING_DIRECTORIES": str(Path(case.tmp).resolve().parent),
    }))
    local = config.ROOT / "charter.local.toml"
    local.write_text(text)
    (config.ROOT / ".gitignore").write_text("/charter.local.toml\n")
    subprocess.run(["git", "-C", str(config.ROOT), "init", "-q"], check=True,
                   capture_output=True, env={**os.environ, **_gitguard.environment()})
    return local


class Typed:
    """A terminal somebody is sitting at, holding the lines they are about to type.

    Replacing ``sys.stdin`` outright is how a test declares its terminal-ness to
    `tests/_ttyguard` — the guard's own stream is not consulted at all — and it is the only
    way to count the READS, which is what "asked exactly once" is an assertion about. A
    read with nothing left is an error rather than an empty string, because a second
    question is the failure two of those cases exist to catch.
    """

    def __init__(self, *lines: str) -> None:
        self.lines = list(lines)
        self.reads = 0

    def isatty(self) -> bool:
        return True

    def readline(self) -> str:
        self.reads += 1
        if not self.lines:
            raise AssertionError("nothing here was going to answer a second question")
        return self.lines.pop(0)


class APipe:
    """The other end of the same question: not a terminal, and never read from or written
    to — a read here is a launch waiting on something that cannot answer."""

    def isatty(self) -> bool:
        return False

    def readline(self) -> str:
        raise AssertionError("nobody is here to answer a question")

    def write(self, text: str) -> None:
        raise AssertionError("nothing may be asked of a stream that is not a terminal")


class ATerminal(io.StringIO):
    """A `StringIO` that says it is a terminal: the screen half of an ask, kept where a
    test can read back what was drawn on it."""

    def isatty(self) -> bool:
        return True


def approve_profile(case, name="claude-work"):
    """Seed *name*'s launch record, the way an operator who already said yes once has.

    *name* may also be a `profiles.Profile` built by hand, for a case whose subject is what
    happens to a profile once it is approved and which never declares it in a file — the
    record is keyed by the profile's name and holds what it runs, whichever way it was made.

    **A real-tmux test that launches a declared profile has to call this** (Global
    Constraint, N6). `_launch` passes `--attended` for an open somebody is in front of,
    and a real pane has a terminal on both of its ends — so a declared profile with no
    record does exactly what it promises there: it prints its command and waits at
    `run this? [y/N]` until the suite's own 600 s watchdog kills the run. A hang is not a
    verdict, and this is the one line that keeps it from being one.

    The profile, so a caller can assert about the very object the record was made from.
    `profiletrust` is imported here rather than at the top for ruling 43's reason: this
    module is imported by most of the suite, and profile code stays off that path.
    """
    from charter import profiles, profiletrust

    p = name if isinstance(name, profiles.Profile) else profiles.current().profiles[name]
    why = profiletrust.record_launched(p)
    case.assertEqual(why, "", f"the launch record for '{p.name}' could not be written")
    return p


def approve_every_profile(case) -> None:
    """:func:`approve_profile` for every profile this plane declares right now.

    For a fixture whose cases are about something else entirely — which profile a `+`
    carries, what a refusal says about `PATH` — where naming each one would be a list that
    goes stale the next time the fixture grows a profile. Called AFTER a case rewrites
    `charter.local.toml`, because it records what is declared at the moment it runs.
    """
    from charter import profiles

    for name in profiles.current().profiles:
        approve_profile(case, name)


def assert_approved(name: str = "claude-work") -> None:
    """Fail now, before tmux, if *name* has no launch record — :func:`approve_profile`'s
    other half.

    A missing record is a **hang** rather than a failure once a real pane is involved, and
    a hang costs the whole run its verdict. Asked before the launch so the message names
    the fixture's mistake instead of arriving as a watchdog kill fifteen minutes later.
    """
    from charter import profiles, profiletrust

    p = profiles.current().profiles.get(name)
    if p is None or profiletrust.approval_needed(p):
        raise AssertionError(
            f"no launch record for '{name}' — its pane would wait at run this? [y/N]")


def wired_as_today(case=None):
    """Let every profile launch, for a case whose subject is not wiring (ruling 10).

    Task 4 refuses a profile whose config folder does not carry charter's guard, and that
    applies to built-ins: a launch test would otherwise refuse where it used to start.
    In-process the suite's own guard makes `plugincache.available` answer False, so
    detection reads UNKNOWN and refuses; in a CHILD process its fake `claude` answers `[]`,
    which reads as unwired. Either way the answer is "not wired", and a test about `+`,
    tabs or reopen is not about that.

    `wiring.refusal` and not `wiring.detect`, so the probe seam stays available to the
    module that IS about wiring. **A real-tmux test cannot use this** — a pane is a child
    process no patch reaches — so those declare a profile whose `command` is their own
    recorder, and the recorder answers `plugin list --json` with a covering, enabled entry.

    Given no *case* it returns the patcher instead of entering it, for a module where NO
    test is about wiring — `setUpModule` starts it and `tearDownModule` stops it, which is
    one fixture rather than one per class in a file with thirty of them.
    """
    from charter import wiring

    patcher = mock.patch.object(wiring, "refusal", lambda p, *, cwd: "")
    if case is None:
        return patcher
    case.enterContext(patcher)
    return None


def isolate_state_dir(case) -> Path:
    """Point ``config.STATE_DIR`` at a throwaway dir for one test case, restoring after.

    For tests that deliberately run against the REAL control plane — proving `render()`
    never crashes in a real environment, say — while still not writing into the
    developer's own `.charter/`. `STATE_DIR` holds per-developer caches and session
    pointers, so redirecting it changes nothing the test is asserting about the plane.

    The render path writes a cache per turn (`repostate.json`, `glstate.json`, and now
    `vaulthealth.json`), so any unisolated test calling `statusline.render` has been
    quietly writing there. `PersonaIso` covers the tests that want a fake plane; this
    covers the ones that want the real one.
    """
    tmp = Path(tempfile.mkdtemp(prefix="charter-state-"))
    orig = config.STATE_DIR
    config.STATE_DIR = tmp / ".charter"
    case.addCleanup(lambda: (setattr(config, "STATE_DIR", orig),
                             shutil.rmtree(tmp, ignore_errors=True)))
    return config.STATE_DIR


def pin_update_channel(case, channel: str = "stable") -> dict:
    """Pin ``config.UPDATE`` for one test case, restoring after.

    The companion to :func:`isolate_state_dir`, for the same shape of test and for the
    reason `tests/_planeguard.py` sets out at length: a case that deliberately runs against
    the REAL plane still may not read `[update] channel` off it, because that value belongs
    to whoever is running the suite rather than to charter (#459). Without a pin, such a
    case passes on a stable plane and fails on a dev one, and `tests._planeguard` refuses
    the read rather than let it decide the assertion silently.

    Defaults to ``"stable"`` — the channel almost every case here means when it means
    nothing in particular, and the one `instance.UPDATE_DEFAULTS` gives a plane that
    declares no ``[update]`` section at all. A case that is ABOUT the dev channel passes
    ``"dev"`` and is then testing a fixture rather than an environment.

    A misspelt channel is refused here rather than clamped. `channel.channel()` re-matches
    whatever it is handed against `instance.UPDATE_CHANNELS` and falls back to ``stable``,
    which is right for a committed file written by a human and wrong for a fixture: a case
    that pinned ``"deb"`` would silently test the stable path while its own source says it
    is testing dev, and pass for the wrong reason.
    """
    if channel not in instance.UPDATE_CHANNELS:
        raise ValueError(f"{channel!r} is not a charter update channel; expected one of "
                         f"{instance.UPDATE_CHANNELS}")
    pinned = {"channel": channel}
    patcher = mock.patch.object(config, "UPDATE", pinned)
    patcher.start()
    case.addCleanup(patcher.stop)
    return pinned
