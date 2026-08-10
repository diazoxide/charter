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
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from unittest import mock

from charter import config, instance, persona, root

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

        # ONE call. This used to re-implement all twenty-five derivations line for line,
        # and four of them were missing — so the suite wrote fixture data into the
        # developer's real `.charter/vaults.json` and orphaned every vault registered on
        # that machine. `config.derive` is now the only place that knows how a setting
        # follows from the root, and this asks it rather than copying it.
        self._orig = config.use(self.tmp)

        config.PERSONAS_DIR.mkdir(parents=True, exist_ok=True)
        self.addCleanup(self._restore)

    def _restore(self) -> None:
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
