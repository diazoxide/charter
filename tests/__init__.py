"""Suite-wide isolation for the config directories charter writes OUTSIDE the plane.

`_isolation.PersonaIso` redirects everything charter derives from the plane root. It could
not cover what came later: charter now installs a plugin into opencode's own config dir,
and a test that called `wire()` without redirecting `$XDG_CONFIG_HOME` wrote a plugin, a
command and a generated context file into the developer's real `~/.config/opencode/` —
including an `instructions` entry pointing at a file describing a fixture plane.

That is the same failure `_isolation` already records for `.charter/vaults.json`: "the
suite wrote fixture data into the developer's real vaults and orphaned every vault
registered on that machine". One redirect there fixed it for every test at once, and this
is the same move for the directories charter reaches outside the plane.

This runs at import of the `tests` package — before any test module is collected — so no
test can opt out by forgetting. A test that WANTS a specific location sets it itself; the
default is simply never the real one.
"""

from __future__ import annotations

import os
import tempfile

_SANDBOX = tempfile.mkdtemp(prefix="charter-suite-config-")

# Redirected, not merely defaulted: a developer with these already set in their shell is
# exactly the case that leaked, and inheriting theirs would keep the hole open for them.
os.environ["XDG_CONFIG_HOME"] = os.path.join(_SANDBOX, "xdg")
os.environ["CODEX_HOME"] = os.path.join(_SANDBOX, "codex")
os.makedirs(os.environ["XDG_CONFIG_HOME"], exist_ok=True)
os.makedirs(os.environ["CODEX_HOME"], exist_ok=True)

# The third fixture a test can inherit without noticing, after the plane root and these
# config directories: the SHELL the suite was launched from. `$CHARTER_SESSION_ID`,
# `$TMUX` and `$CHARTER_WORKSPACE` reach the code under test the same way `$XDG_CONFIG_HOME`
# did, and cost sixteen false failures inside a live frame plus one false GREEN (#519,
# #521, #528). This removes them and then refuses an undeclared read of one.
#
# ABOVE the `_planeguard` import, which is not cosmetic: `$CHARTER_ROOT` is one of the
# names scrubbed, and importing `charter.config` is what resolves the plane. Scrub after it
# and the whole suite is already pointing at whichever plane the operator's shell pinned.
from . import _envguard      # noqa: E402

_envguard.install()

# The same move for the one directory that CANNOT be redirected away from every test —
# some tests read the real plane on purpose — but that none of them may write to. See
# `tests/_planeguard.py`: after this line, a write into the developer's own `.charter/`
# fails the test that made it instead of quietly deleting a running frame's state (#402).
from . import _planeguard      # noqa: E402  (env above must be set before charter loads)

_planeguard.install()
