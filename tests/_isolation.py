"""Shared test helpers: filesystem-isolated persona/config paths + a hook runner.

Persona/memory/hook code reads well-known paths off :mod:`charter.config` at call time
(``config.PERSONAS_DIR``, ``PERSONA_STATE_DIR``, ``ACTIVE_PERSONA_FILE``,
``WORKSPACES_DIR``), so redirecting those module attributes to a tmp dir isolates a test
completely from the real repo. Not a ``test_*`` module, so discovery skips it.
"""

from __future__ import annotations

import io
import json
import shutil
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

from charter import config, instance, persona, root

_PATCH = ("ROOT", "PERSONAS_DIR", "PERSONA_STATE_DIR", "STATE_DIR", "ACTIVE_PERSONA_FILE",
          "WORKSPACES_DIR", "SESSIONS_DIR", "TERMINALS_DIR", "HAS_CONTROL_PLANE",
          "CONFIG_ERROR", "GROUP", "EXCLUDE", "DEFAULT_WORKSPACE", "INVENTORY",
          "MEMORY_SHARE", "PLANE_SHAPE", "WORKTREES_ROOT",
          # Every one of these was missing, and each is a path a test can WRITE to in the
          # developer's real checkout. `VAULTS_REGISTRY` is the worst: the suite replaced a
          # real registry with its own fixture data, orphaning every vault registered on
          # the machine — the exact harm issue #22 describes, delivered by the test suite
          # instead of by `vault add`. `DOCS_DIR` lets a doc-generating test write into the
          # real `docs/`. See `EveryRootDerivedPathIsIsolated`, which now fails if another
          # one is ever added to config.py without landing here.
          "VAULTS_REGISTRY", "VAULTS_DIR", "ACTIVE_WORKSPACE_FILE", "DOCS_DIR")


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
        self._orig = {k: getattr(config, k) for k in _PATCH}
        config.ROOT = self.tmp
        config.STATE_DIR = self.tmp / ".charter"
        config.PERSONAS_DIR = self.tmp / "personas"
        config.PERSONA_STATE_DIR = config.STATE_DIR / "persona-state"
        config.ACTIVE_PERSONA_FILE = config.STATE_DIR / "active-persona"
        config.WORKSPACES_DIR = self.tmp / "workspaces"
        config.SESSIONS_DIR = config.STATE_DIR / "sessions"
        config.TERMINALS_DIR = config.STATE_DIR / "terminals"
        # Derived exactly as config.py derives them, against the temp ROOT.
        config.VAULTS_REGISTRY = config.STATE_DIR / "vaults.json"
        config.VAULTS_DIR = config.STATE_DIR / "vaults"
        config.ACTIVE_WORKSPACE_FILE = config.STATE_DIR / "active-workspace"
        config.DOCS_DIR = config.ROOT / "docs"
        # Task 1's fix round found a test that wrote a fake inventory/repos.json into the
        # REAL checkout because this tuple omitted INVENTORY — every other well-known path
        # was redirected into the tmp tree, but inventory.load()/save() kept resolving
        # against the real repo's config.INVENTORY (ambient state). Derive it the same way
        # config.py itself does (`ROOT / "inventory" / "repos.json"`), so a test reading or
        # writing the inventory through PersonaIso is isolated like everything else.
        config.INVENTORY = self.tmp / "inventory" / "repos.json"
        # Same derivation config.py itself uses at import time (`ROOT / MARKER`), but
        # re-run against the just-installed temp ROOT — otherwise a test reading this
        # flag through PersonaIso would see whatever the real process's ROOT happened
        # to be (ambient state), not something consistent with the tmp dir every other
        # patched attribute above already points at.
        config.HAS_CONTROL_PLANE = (config.ROOT / root.MARKER).is_file()
        # Same derivation config.py itself uses at import time (`instance.load(ROOT)` +
        # group_of/exclude_of/default_workspace_of), re-run against the temp ROOT — a
        # test reading GROUP/EXCLUDE/DEFAULT_WORKSPACE/CONFIG_ERROR through PersonaIso
        # must see values consistent with the tmp dir, not whatever real charter.toml
        # (or absence of one) happened to be ambient in the process that ran the suite.
        try:
            _cfg = instance.load(config.ROOT)
            config.CONFIG_ERROR = None
        except Exception as e:
            _cfg, config.CONFIG_ERROR = {}, str(e)
        config.GROUP = instance.group_of(_cfg, config.GROUP_FALLBACK)
        config.EXCLUDE = instance.exclude_of(_cfg)
        config.DEFAULT_WORKSPACE = instance.default_workspace_of(_cfg, config.DEFAULT_WORKSPACE_FALLBACK)
        config.MEMORY_SHARE = instance.share_of(_cfg)
        config.PLANE_SHAPE = instance.shape_of(_cfg)
        # Re-derived through config's own resolver, not reimplemented here — an embedded
        # plane's worktree root is a SIBLING of ROOT, so a stale value points outside the
        # tmp tree entirely. Left unpatched it resolved against the real checkout, and the
        # suite wrote worktrees into the developer's projects directory and carried them
        # from one test to the next (a `⑂2` assertion failing with `⑂6`).
        config.WORKTREES_ROOT = config.worktrees_root_for(
            config.ROOT, config.PLANE_SHAPE, _cfg)
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
        for k, v in self._orig.items():
            setattr(config, k, v)
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
