"""One name, one directory: the containment rule for names charter reads out of files.

Charter validated a name when a human typed it and never when it read one from a
committed file (#328). `valid_name` lives in :mod:`charter.persona` and
:mod:`charter.workspace` and is called from six places, all of them commands. The reading
side — `extends:`, `uses:`, `[persona] default`, a `workspace.json` repo name, an
`inventory/repos.json` name — joined the value onto a path with nothing in between, so a
committed file could name a target outside the directory charter meant to look in.

:mod:`charter.docsrc` already had the answer and it was never reused: a topic is matched
against a shape before it becomes a page, because ``charter docs show ../../etc/passwd``
"must not be a file-read primitive wearing a documentation command". This module is that
idea, extracted so the five reading sites share one implementation instead of four
near-misses and a fifth that forgets.

**Two questions, kept apart on purpose.**

*Shape* — "could this string name one entry in a directory?" — is :func:`segment_ok`.
*Containment* — "does joining it stay inside the base?" — is :func:`child`. Callers use
both, and the pair is deliberate rather than redundant: the shape check belongs where an
identity is decided (`inventory.merge`, beside the bare-name collision logic that already
treats the name as load-bearing), and the containment assertion belongs at every join,
because a hand-edited or PR-modified tracked file never passes through the code that
decided the identity. An identity-layer check alone is a guard the attacker walks around.

**Who gets which shape rule.** Charter mints persona and workspace names — `persona
create` and `workspace create` enforce their own `valid_name` — so those keep answering
to `valid_name`, and lint agrees with the resolver by construction rather than by a second
check kept in step by hand. A **forge** mints repo names, and `org/.github` is a real repo
GitHub itself tells organisations to create, while `MyRepo` is merely ordinary. Imposing
charter's creation-time alphabet on someone else's forge would refuse to clone both.
:func:`segment_ok` is therefore the permissive rule: it forbids traversal and separators
and nothing else.

**Containment is lexical, and does not follow symlinks.** :func:`docsrc.read` resolves its
page precisely to catch a symlink pointing out of the directory, and copying that here
would be a mistake in two directions. It would do half of #336 — whose containment half is
about symlinks in *every* file charter reads, not only the ones named by a name — while
claiming none of it; and it would refuse a plane that legitimately symlinks a persona
directory today, which is a working plane broken in exchange for a hole that stays open
anyway. Traversal and absoluteness are what these five issues are about. Symlinks are
filed, and stay filed.

**Nothing here raises.** These checks sit under `doctor`, the status line and SessionStart,
where the rule is that a hook may cost a session its briefing and never its turn. A refused
name is reported as data — see :data:`NOT_A_SEGMENT` and the vocabulary in
:mod:`charter.news`, which says five kinds of "no answer" five different ways for the same
reason: folding a defect in a file behind a generic message hides the defect, and somebody
has to fix it.
"""

from __future__ import annotations

import os
from pathlib import Path

#: Said once, so every site refuses in the same words. A reader who hits this has a defect
#: in a committed file, not a typo, and the sentence has to be enough to act on: "no such
#: repo" would send them looking for a repo that was never the point.
NOT_A_SEGMENT = ("'{name}' is not a name — it is a path. This is read from a committed "
                 "file and joined onto a directory, so it may name one entry there and "
                 "nothing else: no '/', no '\\', no '.' or '..', and nothing absolute")

#: The separators every platform charter runs on will honour. Backslash is included on
#: POSIX too: the file is committed and shared, so the machine that *wrote* the name is
#: not necessarily the machine that resolves it.
_SEPARATORS = ("/", "\\")


def segment_ok(name: str) -> bool:
    """True when *name* could name one entry inside some directory.

    Deliberately a question about the *string*, never about the disk. Asking the
    filesystem would make a traversal succeed exactly when the attacker's target happens
    to exist, which is the one case where the answer must not change.
    """
    if not name or not isinstance(name, str):
        return False
    if name in (".", ".."):
        return False
    if "\x00" in name:
        # A NUL terminates the string inside the C library, so the name Python checked and
        # the name the kernel opened would be two different strings.
        return False
    if any(sep in name for sep in _SEPARATORS):
        return False
    # Catches a Windows drive-qualified name ("C:x") and anything else the running
    # platform considers rooted, without charter maintaining its own list of what those
    # look like.
    if os.path.isabs(name) or os.path.splitdrive(name)[0]:
        return False
    return True


def child(base, name: str) -> Path | None:
    """``base / name`` when that is a direct child of *base*, else ``None``.

    ``None`` rather than an exception because every caller is on a path that must not
    crash, and rather than a sanitised name because silently rewriting a name invents a
    second identity for the same thing and hides the defect in the file that somebody
    still has to fix.

    The normalised comparison is belt and braces over :func:`segment_ok` — which already
    forbids every separator, so the join cannot escape today. It is kept because it is the
    half that still holds if the shape rule is ever loosened to admit some new name, which
    is exactly the drift `docsrc`'s own comment warns about.
    """
    if not segment_ok(name):
        return None
    base = Path(base)
    joined = base / name
    if Path(os.path.normpath(joined)).parent != Path(os.path.normpath(base)):
        return None
    return joined


def refusal(name: str) -> str:
    """The one sentence every site uses to say why *name* was refused."""
    return NOT_A_SEGMENT.format(name=name)
