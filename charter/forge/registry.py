"""Which forge does this control plane — or this repo — use?

A control plane declares one or more ``[[forge]]`` blocks in ``charter.toml``. Each repo
record carries a ``forge`` stamp so a merged, multi-forge inventory stays unambiguous.
"""
from __future__ import annotations

import re

from .base import Forge
from .github import GitHubForge
from .gitlab import GitLabForge

KINDS: dict[str, type] = {"gitlab": GitLabForge, "github": GitHubForge}

#: Records written before the stamp existed are GitLab's, since that was the only backend.
_DEFAULT_KIND = "gitlab"


class CollisionError(Exception):
    """Two forges expose a repo with the same bare name."""


#: A hostname, optionally with a port. Deliberately not a URL and deliberately not an
#: allowlist of hosts: a self-hosted forge is the whole reason ``host`` exists, so charter
#: cannot know the set — but it can know the SHAPE. Labels of letters/digits/hyphens
#: separated by dots, no scheme, no path, no userinfo, no whitespace.
_HOST_RE = re.compile(
    r"^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?"
    r"(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)*"
    r"(?::\d{1,5})?$")

NOT_A_HOST = ("host {host!r} is not a hostname. It is read from a committed charter.toml "
              "and reaches both the SSH guard's deny set and the "
              "`url.https://<host>/.insteadOf` that `charter git-policy --apply` writes "
              "into a clone's git config, so it takes a bare host (optionally :port) — no "
              "scheme, no path, no '@'")


def host_ok(host) -> bool:
    """True when *host* is a hostname rather than something else that fits in the slot.

    Uppercase is accepted and NOT normalised: git treats hostnames case-insensitively and
    ``hooks._single_credential_hit`` already lowercases at match time, while rewriting the
    value here would give one forge two identities — the thing `contain` refuses names for.
    """
    return isinstance(host, str) and bool(_HOST_RE.fullmatch(host))


def _build(kind: str, host: str | None) -> Forge:
    cls = KINDS.get(kind)
    if cls is None:
        raise ValueError(
            f"unknown forge kind {kind!r} — known kinds: {', '.join(sorted(KINDS))}")
    if host and not host_ok(host):
        # Raised, because every caller that can be reached from a committed file already
        # catches per block: `declared_forges` records the message and keeps every other
        # block, which is the machinery a typo'd `kind` has used since it was written.
        raise ValueError(NOT_A_HOST.format(host=host))
    return cls(host) if host else cls()


def forges_for(cfg: dict) -> list[tuple[Forge, str]]:
    """``(forge, owner)`` for every ``[[forge]]`` block in a control plane's config."""
    out = []
    for block in cfg.get("forge") or []:
        owner = block.get("group") or block.get("owner") or ""
        out.append((_build(block.get("kind") or _DEFAULT_KIND, block.get("host")), owner))
    return out


def for_repo(repo: dict) -> Forge:
    """The backend that owns a repo record, from its ``forge`` stamp."""
    return _build(repo.get("forge") or _DEFAULT_KIND, None)


def declared_or_default(root) -> list[Forge]:
    """The forges THIS control plane actually declares (``[[forge]]`` blocks in its own
    ``charter.toml`` at *root*), de-duplicated by ``(kind, host)`` — falling back to a
    single default :class:`GitLabForge` when none are declared, the shape every control
    plane had before multi-forge support existed (and still what a fresh ``charter
    init``, or a legacy single-forge control plane, produces).

    The one place both ``doctor`` (which forge CLI/auth to preflight) and a generated
    persona sub-agent (which forge's wording to speak) ask "what forge(s) does this
    control plane use" — sharing this avoids the two ever drifting apart (a GitHub-only
    control plane getting a `glab` doctor check is the same class of bug as its
    generated `.claude/agents/<name>.md` telling the reader about `glab`).

    Never raises: a malformed ``[[forge]]`` block is a config mistake surfaced
    separately (``doctor.check_control_plane_config``) — this just skips it rather than
    taking the caller down."""
    from .. import instance
    try:
        cfg = instance.load(root)
    except Exception:
        cfg = {}
    try:
        pairs = forges_for(cfg)
    except Exception:
        pairs = []
    if not pairs:
        return [GitLabForge()]
    seen: dict[tuple, Forge] = {}
    for forge, _owner in pairs:
        seen.setdefault((forge.kind, forge.host), forge)
    return list(seen.values())


def _default_forges() -> dict[str, Forge]:
    """``host -> Forge`` for every registered kind's DEFAULT host (from :data:`KINDS`,
    so a new forge kind is covered automatically the day it's registered — never a
    hardcoded literal)."""
    return {cls().host: cls() for cls in KINDS.values()}


def declared_forges(root) -> tuple[dict[str, Forge], list[str]]:
    """``(host -> Forge, errors)`` for every ``[[forge]]`` block DECLARED in *root*'s own
    ``charter.toml`` — resolved **per block**, so one broken block can never take a good
    sibling block down with it.

    Before this, a single malformed block (a typo'd ``kind``, a missing field) made the
    WHOLE declared set vanish: :func:`forges_for` raised on the first bad block, which
    aborted the loop before any later ``dict`` update happened — including for perfectly
    good blocks that came before or after it in the same file. A guard that degrades to
    LESS coverage on a partial config error is worse than no guard, because it still
    looks present (see ``known_forges``/``hooks._known_forges``, which is what actually
    denies the SSH bypass). This is the resilient replacement: every block is attempted
    independently, a bad one is recorded in *errors* and skipped, and every block that
    DID resolve still ends up in the returned mapping.

    Only when ``charter.toml`` itself can't be read/parsed at all (truly malformed TOML,
    or a schema newer than this charter understands) is there no per-block salvage to
    do — ``instance.load`` raises before any block is ever seen, so the whole widening is
    unavailable that time; that failure is reported as a single entry in *errors* too.
    Never raises — callers (the PreToolUse guard) must never be broken by a bad file."""
    try:
        from .. import instance
        cfg = instance.load(root)
    except Exception as e:
        return {}, [f"charter.toml: {e}"]
    forges: dict[str, Forge] = {}
    errors: list[str] = []
    for i, block in enumerate(cfg.get("forge") or []):
        try:
            if not isinstance(block, dict):
                raise ValueError(f"[[forge]] block {i} is not a table")
            forge = _build(block.get("kind") or _DEFAULT_KIND, block.get("host"))
        except Exception as e:
            errors.append(f"[[forge]] block {i}: {e}")
            continue
        forges[forge.host] = forge
    return forges, errors


def known_forges(root) -> dict[str, Forge]:
    """``host -> Forge`` for every host this control plane's one-credential policy
    covers: every registered kind's DEFAULT host, widened by *root*'s own
    ``charter.toml`` ``[[forge]]`` blocks (:func:`declared_forges` — a DECLARED host wins
    over a same-named default, so a self-hosted GitLab declared as ``gitlab.com`` would
    still take priority, though that's not the normal case). That widening is what
    recognises a DECLARED self-hosted host (``host = "git.internal"``), which no class's
    default host can ever match on its own.

    Shared by the SSH guard (``hooks._known_forges``) and forge resolution
    (:func:`resolve_host`, used by ``gitpolicy.forge_for`` / ``commands._origin_https``)
    so the same host set backs both the denial and the "is this repo compliant" check —
    they can never drift apart.

    Best-effort and never raises. A malformed/unreadable ``charter.toml`` degrades to
    just the default hosts (:func:`declared_forges` already handles that); one broken
    ``[[forge]]`` block degrades only itself — every OTHER declared host, and the class
    defaults, still come back. This runs on every Bash PreToolUse call, so it must also
    stay non-raising even against a bug this function didn't anticipate — hence the
    outer guard below."""
    try:
        declared, _errors = declared_forges(root)
    except Exception:
        declared = {}
    return {**_default_forges(), **declared}


def known_forges_report(root) -> tuple[dict[str, Forge], list[str]]:
    """Like :func:`known_forges`, but also returns the block/file-level errors
    encountered — for ``doctor`` to surface (the guard itself only ever wants the plain
    mapping; visibility is `doctor`'s job, not something that should slow down or risk
    a Bash PreToolUse call). Never raises."""
    try:
        declared, errors = declared_forges(root)
    except Exception as e:
        declared, errors = {}, [f"charter.toml: {e}"]
    return {**_default_forges(), **declared}, errors


#: Matches the HOST component of ``scheme://[user@]host[:port]/...`` — stops at the
#: first ``/``, ``:`` (port), ``?`` (query) or ``#`` (fragment), so nothing past the
#: host (path, branch/ref, query string) is ever mistaken for it.
_URL_HOST_RE = re.compile(r"^[A-Za-z][A-Za-z0-9+.\-]*://(?:[^@/]*@)?([^/:?#]+)")
#: Matches the HOST component of scp-style ``[user@]host:path`` (e.g. ``git@host:org/repo.git``).
#: The negative lookahead excludes ``scheme://`` forms (already handled above) — a colon
#: immediately followed by ``//`` is a scheme separator, not the scp host/path separator.
_SCP_HOST_RE = re.compile(r"^(?:[^@/\s]+@)?([^/\s:]+):(?!//)")


def _host_of(url: str) -> str:
    """The HOST COMPONENT of a git remote URL — never a substring match anywhere else in
    the URL. Handles both shapes git accepts: ``scheme://[user@]host[:port]/path`` and
    scp-style ``[user@]host:path``. Case-folded (git treats hostnames case-insensitively
    on the wire). Returns ``""`` when no host can be parsed — never raises.

    This is FINDING 1's fix: the previous ``host in url`` substring check matched a known
    host appearing ANYWHERE in the string — including inside the *path* — so
    ``git@git.internal:gitlab.com-mirror/api.git`` (a self-hosted remote whose path
    happens to start with ``gitlab.com-``) misresolved as gitlab.com's own forge."""
    url = (url or "").strip()
    if not url:
        return ""
    m = _URL_HOST_RE.match(url)
    if m:
        return m.group(1).lower()
    m = _SCP_HOST_RE.match(url)
    if m:
        return m.group(1).lower()
    return ""


def resolve_host(url: str, root) -> Forge | None:
    """Infer a backend from a remote URL, recognising both every registered kind's
    DEFAULT host (``gitlab.com``, ``github.com``) AND a host DECLARED in *root*'s own
    ``charter.toml`` (see :func:`known_forges`). This is what lets a self-hosted forge
    (GitLab Enterprise, GHE) resolve to its own real policy instead of silently falling
    through to another forge's, or going unrecognised entirely.

    Matches *url*'s **host component** (:func:`_host_of`) — never a substring of the
    whole URL — and checks **declared** hosts before class defaults, so a control plane
    that (unusually) redeclares a default host's name under its own ``[[forge]]`` block
    still gets the declared one. Returns ``None`` when *url*'s host matches neither a
    declared nor a default forge — genuinely unrecognised. Callers MUST treat that as
    **unmanaged**, never silently apply another forge's policy: doing so is what let a
    self-hosted clone report a false-green "token-only" while its SSH remote was never
    actually rewritten off SSH (see ``gitpolicy.forge_for``)."""
    host = _host_of(url)
    if not host:
        return None
    declared, _errors = declared_forges(root)
    if host in declared:
        return declared[host]
    return _default_forges().get(host)
