"""No test, in this process or in a child, may reach the developer's real Claude Code.

`charter init` installs charter's own Claude Code plugin (#881) — `claude plugin
marketplace add`, then `claude plugin install charter@charter --scope project`. Nine
modules call `commands.cmd_init` in-process and `tests/test_cli_smoke.py` runs it as a real
subprocess, so on a laptop with `claude` on `PATH` a suite run installs a plugin into the
operator's real Claude Code once per fixture plane. Measured before this file existed: five
`charter@charter` rows in `claude plugin list`, every one of them scoped to a temp directory
that no longer existed.

CI has no `claude`, so CI would never have shown it — the same shape `tests/_ttyguard.py`
records for the suite's file descriptors and `tests/__init__.py` for `$XDG_CONFIG_HOME`.

**Two mechanisms, because a child process cannot be patched.**

* In this process, :func:`install` makes `plugincache.available()` answer ``False`` — CI's
  answer. A test that wants the other one says so, with the idiom
  `tests/test_plugin_freshness.py` already uses seventeen times::

      mock.patch.object(plugincache, "available", return_value=True)

  and stubs the seam underneath it in the same `with`. Opting in without also stubbing
  `plugincache.util.run` spawns a real binary — `available` is a fact about the machine,
  not a fake `claude`.

* For children, a fake `claude` goes FIRST on ``$PATH``. It answers `plugin list --json`
  and `plugin marketplace list --json` with ``[]`` — *Claude Code is here and charter's
  plugin is not* — and accepts the mutating subcommands silently. So a subprocess `charter
  init` walks the whole install path and reports what a first install reports, having
  touched nothing. It is deliberately stateless: a fixture that remembered its own installs
  would be a second implementation of Claude Code's plugin registry, and every test here
  asks one question of it.
"""

from __future__ import annotations

import os
import stat
import tempfile
from pathlib import Path

#: A `claude` that knows exactly as much as this suite asks it. Everything it does not
#: recognise exits 0 and says nothing, which is what makes it safe rather than complete:
#: the failure mode of an unknown subcommand is "nothing happened", never "something
#: happened to the operator's machine".
_FAKE = '''#!/usr/bin/env python3
"""The test suite's `claude`. See tests/_claudeguard.py — nothing here is real."""
import sys

argv = sys.argv[1:]
if argv[:1] == ["plugin"] and "--json" in argv:
    # `plugin list` and `plugin marketplace list`: an empty registry, so charter reads
    # "Claude Code is installed here and charter's plugin is not" and takes the install
    # path. An unreadable answer would instead be `UNKNOWN`, which is the branch that
    # deliberately installs NOTHING.
    print("[]")
sys.exit(0)
'''


#: A fake `opencode`, for the same reason and one release later.
#:
#: `doctor`'s per-profile wiring rows probe every LISTED profile, and a built-in is listed
#: when its program is on `PATH` — so on a laptop with opencode installed, every module that
#: reaches `doctor.run_all()` would spawn the real binary (measured: 631-718 ms a call).
#: `[]` means *opencode is here and charter's shim is not*, which is the answer CI gives and
#: the one every assertion in the suite was written against.
_FAKE_OPENCODE = '''#!/usr/bin/env python3
"""The test suite's `opencode`. See tests/_claudeguard.py — nothing here is real."""
import json, os, sys
from pathlib import Path

if sys.argv[1:3] == ["debug", "config"]:
    # The one thing the real binary does that any test could care about: it lists every
    # plugin in `<XDG_CONFIG_HOME>/opencode/plugin/` as a `file://` URI, with the
    # un-resolved spelling (measured against 1.18.23). Listing `[]` unconditionally would
    # make `charter harness install opencode` report a shim it had just written as absent.
    xdg = os.environ.get("XDG_CONFIG_HOME") or os.path.join(os.path.expanduser("~"), ".config")
    d = Path(xdg) / "opencode" / "plugin"
    try:
        here = sorted(p for p in d.iterdir() if p.suffix in (".ts", ".js"))
    except OSError:
        here = []
    print(json.dumps({"plugin": [f"file://{p}" for p in here]}))
sys.exit(0)
'''


#: `plugincache.available` as charter wrote it, kept before :func:`install` stands in for it
#: — for the one kind of case that is ABOUT how a program is looked up (which `PATH`), and
#: stubs `util.run` underneath so nothing it finds is ever spawned.
REAL_AVAILABLE = None


def install() -> None:
    """Arm both halves. Idempotent, and called once from `tests/__init__.py`."""
    from charter import plugincache

    # Patched on the module rather than on `shutil.which`, because `which` answers for
    # every binary the suite looks up — `git`, `gh`, `tmux` — and this is a statement about
    # one of them. It is also the exact seam every existing opt-in already patches, so a
    # test that wants the other answer needs to know one name and not two.
    # `*a, **k`, not a zero-argument lambda: `available` takes the COMMAND to look up now,
    # because a profile names its own (`["/opt/claude-wrap"]`), and the first call passing
    # one would otherwise be a `TypeError` out of the guard rather than an answer.
    global REAL_AVAILABLE
    if REAL_AVAILABLE is None:
        REAL_AVAILABLE = plugincache.available
    plugincache.available = lambda *a, **k: False

    d = Path(tempfile.mkdtemp(prefix="charter-suite-claude-")) / "bin"
    d.mkdir(parents=True, exist_ok=True)
    for name, body in (("claude", _FAKE), ("opencode", _FAKE_OPENCODE)):
        fake = d / name
        fake.write_text(body)
        fake.chmod(fake.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    # PREPENDED, not appended: the operator's real `claude` is further down this same PATH
    # and appending would leave it winning. A child that spawns a grandchild inherits this
    # too, which is the point.
    os.environ["PATH"] = f"{d}{os.pathsep}{os.environ.get('PATH', '')}"
