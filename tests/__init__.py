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

# The config file charter never writes and every `git` it spawns reads: the operator's own
# `~/.gitconfig`. Redirected here for the same reason as the two above and with a sharper
# consequence — a fixture repository inherits `commit.gpgsign = true`, and with 1Password's
# `op-ssh-sign` behind it `git commit` parks on a biometric prompt and the suite never
# returns (#641). Not a failure: a hang, with no verdict and no line saying why, and one CI
# can never see because a runner has no signing config. See `tests/_gitguard.py`.
#
# ABOVE the `_ttyguard` import below and so above everything that pulls charter in:
# `charter.config` resolves a plane at import and reads a git worktree pointer on the way.
from . import _gitguard      # noqa: E402  (imports no charter module, by design)

_gitguard.install()

# The FOURTH fixture, installed FIRST because it is the one thing here that must be true
# before `charter` is imported at all: the suite's own file descriptors. `charter.util`
# computes `_USE_COLOR` from `sys.stderr.isatty()` at import, and `sys.stdin`'s tty-ness
# decides whether charter stops and asks a human — which is how 122 tests in one module sat
# on charter's own workspace picker, forever, whenever the suite was run from a terminal
# (#545). See `tests/_ttyguard.py`: all three streams now answer what CI answers, and a
# test that wants a different answer for stdin has to say so.
#
# The same module answers how WIDE they are, which is the same fixture one question along:
# `os.get_terminal_size()` is an ioctl on this process's stdout, and removing `$COLUMNS`
# from the environment only moves `tui.term_width`'s reading onto it. Measured with both
# geometry variables already unset: three modules give three failures and an error on a
# 40-column pty and pass on a 200-column one (#544).
from . import _ttyguard      # noqa: E402  (imports no charter module, by design)

_ttyguard.install()

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

# The one thing the tripwire above cannot do is ANSWER. `doctor.check_forge_auth` runs
# `gh auth status --hostname github.com` — the operator's real token, validated against a
# real forge, 28 times per run from 18 modules (#638). Refusing that would only redden
# them, so this answers it with a recorded reply, and `_planeguard.RealForgeReach` refuses
# every forge-CLI spawn that is left. BELOW `_planeguard.install()`, deliberately: the
# refusal is armed first, so anything this fixture does not cover fails by name rather than
# reaching github.com.
from . import _forgeprobe      # noqa: E402

_forgeprobe.install()

# One more fact about the machine that CI does not have and a laptop does: the `claude`
# CLI. `charter init` now installs charter's own Claude Code plugin through it (#881), and
# nine modules call `cmd_init` — so on a developer's machine a suite run would install a
# plugin into their real Claude Code, per fixture plane, and CI would never have seen it.
# See `tests/_claudeguard.py`; the opt-in is the one `test_plugin_freshness` already uses.
from . import _claudeguard      # noqa: E402

_claudeguard.install()

# And the one thing no guard can prevent, only clean up after: a run that was KILLED.
# Measured — a `kill -9` two seconds into `test_frame_overlay_escape_hatch` leaves a live
# tmux server and its socket file behind, because the signal skips every `addCleanup`
# there is. 14 such servers and 497 stale socket files were on this machine when #564 was
# fixed. This runs at START, which is the only moment that can clean up after a run that
# had no exit — and it touches nothing whose pid is still alive, so a concurrent run is
# safe.
from . import _tmuxreap      # noqa: E402

_tmuxreap.install()
