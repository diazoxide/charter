"""charter's own documentation pages, resolved for the CLI to print.

A control plane has no reason to vendor a copy of these pages, and every reason not to:
a copy drifts from the binary that implements what it describes, in both directions and
invisibly. The page a user reads should come from the same install as the behaviour, so
`charter docs show secrets` cannot describe a vault the running CLI does not have.

There are two places the pages can live, and both are real:

* **Installed** — `charter/_docs/`, put there by the wheel's `force-include`. This is the
  case for everyone who did not clone the repo.
* **A checkout** — the repo's own `docs/`. `CONTRIBUTING.md` tells contributors to run
  `python3 -m charter ...` from the clone precisely because a `uv tool install` shadows
  it, so this path is the documented development workflow rather than a courtesy.

Installed wins when both exist. A developer with both is running one specific tree; the
packaged copy is the one that travelled with the code being executed.
"""
from __future__ import annotations

import re
from pathlib import Path

#: A topic names one page, and nothing else. The command takes a topic rather than a path
#: on purpose: `charter docs show ../../etc/passwd` must not be a file-read primitive
#: wearing a documentation command, and `adr/0014` must not quietly widen the surface to
#: the design record. Anything outside this shape is simply not a topic.
_TOPIC = re.compile(r"^[a-z0-9][a-z0-9._-]*$")

_PACKAGED = Path(__file__).resolve().parent / "_docs"
_CHECKOUT = Path(__file__).resolve().parents[1] / "docs"


def source() -> Path | None:
    """The directory the pages are read from, or None when neither exists — which
    happens only in a build so broken that the wheel shipped without its data."""
    for candidate in (_PACKAGED, _CHECKOUT):
        if candidate.is_dir():
            return candidate
    return None


def topics() -> list[str]:
    """Every page that can be shown, sorted. Only top-level `*.md`: `adr/`, `audits/`
    and `superpowers/` are design record and working notes, not usage documentation."""
    root = source()
    if root is None:
        return []
    return sorted(p.stem for p in root.glob("*.md") if _TOPIC.match(p.stem))


def read(topic: str) -> str | None:
    """The page's text, or None if `topic` does not name one.

    None covers both "no such page" and "not a topic at all"; the caller classifies once
    rather than distinguishing a typo from an escape attempt, which it has no reason to
    treat differently.
    """
    root = source()
    if root is None or not _TOPIC.match(topic or ""):
        return None
    page = root / f"{topic}.md"
    # `_TOPIC` already excludes separators and `..`, so this cannot currently fail. It
    # stays because the regex is the kind of thing that gets loosened later to allow some
    # new page name, and the containment check is what makes that safe rather than lucky.
    if page.parent.resolve() != root.resolve() or not page.is_file():
        return None
    try:
        return page.read_text()
    except OSError:
        return None
