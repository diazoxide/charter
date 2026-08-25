"""The `edm`-era environment variables, and the one thing charter still does with them.

charter was renamed from ``edm``. Three variables came with the name, and the old spellings
are no longer honored — only the new ones are. Silence about a set-but-ignored
``$EDM_HOME`` would look like state (vaults!) had vanished rather than like a variable
needing a rename, so each one still exported gets a loud line on stderr.

**Why these three strings are not in `charter.config`, which is where they lived and where
:func:`warn` is still called from.** Importing `charter.config` RESOLVES A PLANE — that is
its whole job, and it happens at import — so a caller that only wants to know which names
charter used to answer to had to pay for a plane it never asked for. `tests/_envguard.py`
is exactly that caller, and it is the one caller that cannot pay: it removes charter's
environment from the suite's process *before* `charter.config` is first imported, precisely
so the plane is not resolved out of the operator's shell, and asking config which names to
remove would defeat the ordering it exists to protect.

So it did not ask, and these three names were the only charter variables that reached a
suite which had scrubbed every other one. `charter init` runs in a subprocess there, that
subprocess inherited ``$EDM_WORKSPACE`` from the developer's shell, :func:`warn` printed a
133-column banner into its stderr, and two tests failed on a long-time charter developer's
machine that pass in CI and on a fresh checkout (#540).

A tuple of strings was never what needed a plane. It lives here, in a module that imports
``os`` and ``sys`` and nothing else, so that anyone may ask — and `tests._envguard` derives
the names it scrubs from :data:`NAMES` rather than re-spelling them, the same way it asks
`session._PANE_ID_VARS` instead of copying it out. A fourth rename is covered by the guard
on the commit that adds it, rather than on the commit that debugs it.
"""

from __future__ import annotations

import os
import sys

#: ``(the `edm`-era name, the name that replaced it)``. The value of the old name is never
#: honored anywhere in charter — this pairing exists so the warning can name the variable
#: the reader should set instead, which is the whole reason the warning is worth printing.
RENAMES = (("EDM_HOME", "CHARTER_HOME"),
           ("EDM_WORKSPACE", "CHARTER_WORKSPACE"),
           ("EDM_PERSONA", "CHARTER_PERSONA"))

#: Just the old spellings — derived, so the two can never disagree. What
#: `tests._envguard._scrubbed_names` asks for: charter's own former namespace, which is a
#: closed historical list rather than a prefix, because charter will not invent a new
#: ``EDM_*`` variable and a blanket ``EDM_`` prefix would reach past charter's own names
#: into whatever unrelated tool on the machine happens to share three letters.
NAMES = tuple(legacy for legacy, _ in RENAMES)


def warn() -> None:
    """Print a loud stderr warning for each old `edm`-era env var that's still set —
    its value is never honored (only the new name is), so silence here would look
    like state (vaults!) vanished rather than simply needing a renamed env var."""
    for legacy, new in RENAMES:
        if os.environ.get(legacy):
            print(f"charter: ${legacy} is no longer used (charter was renamed from `edm`) — "
                  f"set ${new} instead. Ignoring ${legacy}.", file=sys.stderr)
