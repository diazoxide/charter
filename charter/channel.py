"""Which charter this plane tracks, and which charter is actually installed.

Two questions that look like one and are not, which is why they live side by side here
rather than being folded into each other:

* **The channel** — ``[update] channel`` in the plane's ``charter.toml``. What the
  operator asked for. Read through :mod:`charter.instance` at the config boundary, where
  it is matched against a closed set (see ``instance.UPDATE_CHANNELS``).
* **The build** — what this interpreter is running, read from the dist-info PEP 610
  ``direct_url.json``. What is on the machine.

They disagree routinely and legitimately: the moment a plane opts into ``dev`` its channel
says dev and its build is still the PyPI wheel, right up until ``charter update`` runs.
Every caller here wants exactly one of the two, and conflating them is how a status line
would claim a dev build that nobody installed.

**Dev builds are never published.** PyPI forbids local version identifiers, so a real dev
release would have to burn ``0.52.0.dev1``, ``.dev2``, … permanently, at a rate of hundreds
a month and irreversibly; and running the release workflow on every push to ``main`` would
multiply exactly the ``id-token: write``-with-unpinned-actions exposure #443 is open about.
So a dev build is installed straight from git and identified from the install record git
left behind, not from a version number that would have to have been published to exist.

Nothing in this module makes a network call or spawns anything. It is read by the status
line's render path, which renders every turn.
"""

from __future__ import annotations

import json

from . import __version__

#: Sentinel for "the dist-info has not been read in this process yet". ``None`` is a real
#: answer here (a PyPI install has no ``direct_url.json``, and neither does a source
#: checkout), so it cannot double as "not looked".
_UNREAD = object()

#: Memoised per process. This is read on the status line's render path, where a dist-info
#: stat per turn is a cost for an answer that cannot change: an install replaces the
#: interpreter's own package, so a running process is the build it started as.
_direct_url: object | dict | None = _UNREAD


def channel() -> str:
    """This plane's update channel — ``"stable"`` or ``"dev"``, never anything else.

    Read off ``config.UPDATE``, which `instance.update_of` has already clamped to
    `instance.UPDATE_CHANNELS`. Callers COMPARE this value and never interpolate it; see
    that constant's own docstring for why the difference is load-bearing.

    Falls back to ``"stable"`` if `config` cannot answer at all — a plane with no
    ``charter.toml``, or one whose parse failed and left `config.CONFIG_ERROR` set.
    """
    from . import config
    got = getattr(config, "UPDATE", None)
    if isinstance(got, dict):
        value = got.get("channel")
        # Re-matched rather than trusted, because `config.UPDATE` is a module attribute a
        # test (or anything else in-process) can assign. The closed set is the guarantee;
        # a second read of it costs a tuple scan of length two.
        from . import instance
        for known in instance.UPDATE_CHANNELS:
            if value == known:
                return known
    return "stable"


def is_dev() -> bool:
    """True when this plane asked for the dev channel. About the PLANE, not the build."""
    return channel() == "dev"


def direct_url() -> dict | None:
    """The PEP 610 ``direct_url.json`` this install recorded, or ``None``.

    PEP 610 is the whole mechanism, and it is a standard rather than a uv detail: an
    installer that resolves a requirement from a *direct URL* — a git ref, a local
    directory, a wheel path — writes this file into the dist-info recording where from; an
    installer that resolved it from an index writes no such file at all. So its ABSENCE is
    the positive statement "this came from PyPI", which is what makes it usable as a
    channel identity without charter having to stamp anything at build time.

    Every failure is ``None`` and none of them raise, because the caller of last resort is
    ``charter --version``, which must always print something. The list is not defensive
    padding — each entry is a state that happens:

    * no dist-info at all — running ``python3 -m charter`` from a checkout, which is what
      CONTRIBUTING.md tells contributors to do;
    * a dist-info with no ``direct_url.json`` — every PyPI install, the common case;
    * unreadable or non-JSON content — a half-written or hand-edited dist-info;
    * JSON that is not an object — same, one level in.
    """
    global _direct_url
    if _direct_url is _UNREAD:
        _direct_url = _read_direct_url()
    return _direct_url if isinstance(_direct_url, dict) else None


def _read_direct_url() -> dict | None:
    """The uncached read. Split out so a test can exercise it without the memo, and so the
    memo has exactly one writer."""
    try:
        from importlib.metadata import Distribution

        # `update.DIST` rather than a second literal: the *package* is `charter-cp` and
        # the *command* is `charter`, and two copies of that fact would be one copy too
        # many. Imported inside the function because `update` imports this module back —
        # for `newer_than`'s dev branch — and a module-level pair would be a cycle.
        from .update import DIST
        raw = Distribution.from_name(DIST).read_text("direct_url.json")
    except Exception:
        # `PackageNotFoundError` is the expected one; the bare catch covers a broken or
        # partially-written dist-info, which raises from inside importlib rather than as
        # something this module could name.
        return None
    if not raw:
        return None
    try:
        doc = json.loads(raw)
    except (ValueError, TypeError):
        return None
    return doc if isinstance(doc, dict) else None


def _vcs_info() -> dict | None:
    """The ``vcs_info`` block of a **git** direct URL, or ``None`` for anything else.

    ``None`` covers three different installs and deliberately does not distinguish them
    here — the callers do. A PyPI install (no direct URL), a directory install
    (``dir_info``, which `build_label` reports as ``+local``), and a non-git VCS install
    (``hg``, ``svn`` — PEP 610 allows them) all answer the same question the same way:
    there is no git commit to compare ``main`` against.
    """
    doc = direct_url()
    if not doc:
        return None
    info = doc.get("vcs_info")
    if not isinstance(info, dict) or info.get("vcs") != "git":
        return None
    return info


def installed_commit() -> str | None:
    """The exact commit this build was installed from, or ``None`` if it was not a git
    install.

    This is the left-hand side of the dev channel's "is there anything newer?" — see
    `update.newer_than`, which compares it against ``main``'s head. ``None`` there means
    *behind by definition*: a plane that asked for dev and is running something that did
    not come from git has not got what it asked for.
    """
    info = _vcs_info()
    commit = (info or {}).get("commit_id")
    return commit.strip() or None if isinstance(commit, str) else None


def installed_ref() -> str | None:
    """The ref that was asked for — ``main`` for the shipped dev install — or ``None``.

    ``requested_revision`` is optional in PEP 610: ``pip install git+…`` with no ``@ref``
    records the commit and no revision at all. `build_label` drops the ref from its output
    in that case rather than inventing ``main``, which would be a guess printed as a fact.
    """
    info = _vcs_info()
    ref = (info or {}).get("requested_revision")
    return ref.strip() or None if isinstance(ref, str) else None


def build_label() -> str:
    """This build's identity, as ``charter --version`` prints it.

    ``0.51.0`` from PyPI, ``0.51.0+dev (main @ abc1234)`` from git, and something honest
    for each way that can degrade. The version number itself is unchanged in every case —
    it is the same wheel contents — so the suffix is additive and a stable install's
    output is byte-identical to what it printed before this existed. That is not cosmetic:
    `commands_update._handoff` verifies an install by comparing ``charter --version``'s
    last word to the version it asked for, and the stable path still hands it one word.
    """
    doc = direct_url()
    if not doc:
        return __version__
    if _vcs_info() is None:
        # A direct URL that is not a git install: ``uv tool install .`` from a checkout,
        # a wheel installed by path, an editable install. Not from PyPI, so saying plain
        # `0.51.0` would claim a provenance this build does not have — but there is no
        # commit to name either.
        return f"{__version__}+local"
    commit = installed_commit()
    ref = installed_ref()
    if not commit:
        # `vcs_info` without a `commit_id` is malformed — PEP 610 requires it — but a
        # malformed record still tells us it was a VCS install, which is the part that
        # decides the channel.
        return f"{__version__}+dev"
    short = commit[:7]
    return f"{__version__}+dev ({ref} @ {short})" if ref else f"{__version__}+dev ({short})"


def _reset_cache_for_tests() -> None:
    """Drop the memo. Named for what it is so it cannot be mistaken for API."""
    global _direct_url
    _direct_url = _UNREAD
