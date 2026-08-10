"""Upstream reporting — turning a charter failure into an issue on charter's own tracker.

charter is installed by strangers (`uv tool install charter-cp`), so when it breaks in
someone's session the maintainer never hears about it: the Reporter is mid-task on their
own work, and opening a browser to write up a reproduction is enough friction that nobody
does. The reporting agent is already sitting there holding the traceback, the version and
what was being attempted — this module turns that into something publishable.

Vocabulary (see ``workspaces/user-reporting/workspace.md``):

* **Reporter** — the human who installed charter and in whose session this surfaced.
* **report** — the *private* draft. It is not public and may never become public.
* **upstream issue** — the public artifact. Qualified deliberately: charter also works
  with issues in the Reporter's *own* repos, so a bare "issue" is ambiguous here.
* **bug** — charter did something wrong. Structured, fingerprintable.
* **gap** — charter cannot do something it should. Free prose, no fingerprint.
* **condition** — a failure that is *not* a bug (a timeout, an interrupt). Never reported.

The governing decisions live in ``docs/adr/`` 0001-0003.
"""

from __future__ import annotations

import hashlib
import json
import platform
import re
import traceback
from pathlib import Path

from . import __version__, config

#: Ceiling on **distinct** pending reports. A broken config makes every invocation fail,
#: and a Reporter who walks away should not return to a full disk. Repeats of a report
#: already on disk are unaffected — they collapse onto it, which is the whole point.
MAX_PENDING = 25

#: The installed charter package. A frame is ours if it lives under here — resolved, so a
#: symlinked install (``uv tool install`` makes several) still matches its own frames.
_PKG = Path(__file__).resolve().parent

#: Every key a bug payload may carry. A **closed allowlist**: a public tracker is the
#: wrong place to discover that some field quietly carried a client's repo name, so
#: nothing travels unless it was decided to be safe. Note the deliberate absence of
#: ``argv`` — the subcommand *name* is safe, its *arguments* carry workspace and repo
#: names — and of anything derived from the session transcript.
BUG_FIELDS = (
    "charter_version",
    "python_version",
    "os",
    "subcommand",
    "exception_type",
    "frames",
    #: The one field that cannot be made safe mechanically: messages routinely embed the
    #: very names we are stripping (``no workspace 'acme-migration'``). Treated as free
    #: text the Reporter must read before sending, never as a field we vouch for.
    "message",
)


def safe_frames(frames) -> list[str]:
    """Charter's own stack frames, rendered package-relative. Everything else is dropped.

    Two different leaks are closed here, and only one of them is about paths. Relativizing
    ``/Users/someone/.local/.../charter/cli.py`` to ``charter/cli.py`` strips the
    Reporter's username. Dropping non-charter frames strips something we could not sanitize
    even if we wanted to: *their* filenames and *their* function names, which on a bug hit
    inside their own tooling would describe code they never agreed to publish.

    A frame that is not a real path (``<string>``, ``<stdin>``) simply fails the check and
    is dropped, which is the correct answer for it too.
    """
    out: list[str] = []
    for f in frames:
        p = Path(f.filename).resolve()
        if not p.is_relative_to(_PKG):
            continue
        # Prefixed with the package directory's own name rather than a literal "charter/",
        # so the rendering stays honest if the package is ever vendored under another name.
        out.append(f"{_PKG.name}/{p.relative_to(_PKG)}:{f.lineno} in {f.name}")
    return out


def fingerprint(payload: dict) -> str:
    """A stable identifier for *the same bug*, from the exception type and the deepest
    charter frame — the place it actually broke.

    Deliberately built **without the message**. The message is where the variable part
    lives: ``no workspace 'alpha'`` and ``no workspace 'beta'`` are one bug, and folding
    the message in would defeat the collapse this exists for — as well as baking a
    Reporter-specific name into a key that gets stored and compared.

    Line numbers are included, so the same bug fingerprints differently across a charter
    release that moved the code. That is the right trade: within a version (where collapse
    matters) it is exact, and across versions the upstream duplicate search is what catches
    the repeat.
    """
    frames = payload.get("frames") or []
    # Deepest charter frame — the innermost is where it broke; the outer ones are just
    # how we got there, and they are identical across unrelated failures in `main`.
    site = frames[-1] if frames else "<no charter frame>"
    return hashlib.sha256(
        f"{payload.get('exception_type')}\n{site}".encode()).hexdigest()[:16]


def bug_payload(exc: BaseException, subcommand: str) -> dict:
    """The publishable part of a **bug** report, built from a caught exception.

    Returns only :data:`BUG_FIELDS`. Callers must not add to the result — the closed set
    *is* the privacy guarantee.
    """
    return {
        "charter_version": __version__,
        "python_version": platform.python_version(),
        "os": platform.platform(),
        "subcommand": subcommand,
        "exception_type": type(exc).__name__,
        "frames": safe_frames(traceback.extract_tb(exc.__traceback__)),
        "message": str(exc),
    }


# --- gaps ---------------------------------------------------------------------------
# A gap is prose, so it cannot be allowlisted field-by-field the way a crash can. What it
# gets instead is a scrub of the things charter can positively identify, plus the human
# read that docs/adr/0003 makes mandatory. The scrub is the careless half; the Reporter is
# the rest, because nothing mechanical catches "our billing service can't do X".

#: Left in place of anything removed. Visible on purpose — a silent redaction reads as
#: charter having published the original.
REDACTED = "[redacted]"

#: Absolute paths rooted at a user's home. These carry the Reporter's username in the
#: second segment and their project names in the rest, so the whole token goes, not just
#: the prefix.
_HOME_PATH_RE = re.compile(r"(?:/Users|/home)/\S*")


def scrub(text: str) -> str:
    """Remove from *text* what charter can positively identify as the Reporter's.

    The advantage over a generic scrubber is that charter knows its own **workspace** and
    **persona** names — which are routinely named after the client or the project. That is
    also the trade-off: a workspace named after a common word will redact that word out of
    ordinary prose. Preferring a mangled sentence to a leaked client name is deliberate,
    and the Reporter reads the result before it is sent either way.
    """
    # Lazy imports: this is the only place report.py needs them, and keeping them out of
    # module scope means importing `report` (which `cli` does on every invocation) does not
    # drag the workspace and persona machinery in.
    from . import persona, workspace

    out = _HOME_PATH_RE.sub(REDACTED, text)

    names = set(workspace.list_workspaces()) | set(persona.list_personas())
    # `default` exists on every install, so it identifies nobody — and redacting it would
    # mangle ordinary English in most reports.
    names.discard(config.DEFAULT_WORKSPACE)
    # Longest first, so `acme-migration` is removed whole rather than being half-eaten by
    # a shorter `acme` and leaving `[redacted]-migration` behind.
    for name in sorted(names, key=len, reverse=True):
        out = re.sub(rf"\b{re.escape(name)}\b", REDACTED, out)
    return out


def gap_payload(text: str) -> dict:
    """The publishable part of a **gap** report. No exception type and no frames — a gap
    is not a crash — but the same version context, which is what tells the maintainer
    whether the capability landed since."""
    return {
        "charter_version": __version__,
        "python_version": platform.python_version(),
        "os": platform.platform(),
        "text": scrub(text),
    }


# --- local storage ------------------------------------------------------------------
# A report outlives the invocation that found it: detection is automatic, approval is not
# (docs/adr/0003), so the two can be hours apart. Reports are also kept AFTER sending,
# stamped with their issue URL, so the next identical crash can point at the existing
# upstream issue instead of re-drafting — local dedupe at zero API cost.

def _path(report_id: str) -> Path:
    return config.REPORTS_DIR / f"{report_id}.json"


def load(report_id: str) -> dict | None:
    """One report by id, or None if there is no such report."""
    p = _path(report_id)
    if not p.exists():
        return None
    try:
        return json.loads(p.read_text())
    except ValueError:
        # A truncated draft (killed mid-write) is not worth crashing a CLI over; the
        # Reporter loses a draft, which is recoverable, where a crash is not.
        return None


def _all() -> list[dict]:
    if not config.REPORTS_DIR.exists():
        return []
    out = []
    for p in sorted(config.REPORTS_DIR.glob("*.json")):
        r = load(p.stem)
        if r:
            out.append(r)
    return out


def pending() -> list[dict]:
    """Reports awaiting the Reporter's approval — drafted but never published."""
    return [r for r in _all() if not r.get("issue_url")]


def _write(rec: dict) -> str:
    config.REPORTS_DIR.mkdir(parents=True, exist_ok=True)
    _path(rec["id"]).write_text(json.dumps(rec, indent=2))
    return rec["id"]


def _record(rid: str, kind: str, payload: dict) -> str | None:
    """Store a report under *rid*, collapsing onto an existing one with the same id.

    Nothing here touches the network — recording is not reporting. That separation is what
    lets detection default to on: it only ever writes to the Reporter's own disk.
    """
    existing = load(rid)
    if existing:
        existing["occurrences"] = existing.get("occurrences", 1) + 1
        return _write(existing)

    # Checked only for a genuinely new report, so a crash loop sitting at the cap keeps
    # counting the very bug that is looping.
    if len(pending()) >= MAX_PENDING:
        return None

    return _write({
        "id": rid,
        "kind": kind,
        "payload": payload,
        "occurrences": 1,
        "issue_url": None,
    })


def record_bug(exc: BaseException, subcommand: str) -> str | None:
    """Record a **bug** locally. Returns its id, or None if the cap refused it.

    The id *is* the fingerprint, so collapse falls out of the storage layout rather than
    needing a search: the same bug twice is the same filename twice.
    """
    payload = bug_payload(exc, subcommand)
    return _record(fingerprint(payload), "bug", payload)


def record_described(kind: str, text: str) -> str | None:
    """Record a report the Reporter **described in prose**, scrubbed. Returns its id, or
    None if the cap refused it.

    Covers both a gap and a bug the Reporter reports by hand — the latter has no exception
    object, so it gets the same prose treatment rather than a traceback payload.

    Scrubbed on the way *in*, not on the way out: a draft sitting on disk unredacted is a
    draft that can be published without the scrub having run. Identical text collapses the
    way a repeated crash does — prose has no stack frame to fingerprint, but an agent
    re-raising the same request every session should not accumulate.
    """
    payload = gap_payload(text)
    rid = hashlib.sha256(f"{kind}\n{payload['text']}".encode()).hexdigest()[:16]
    return _record(rid, kind, payload)


def record_gap(text: str) -> str | None:
    """Record a **gap** locally, scrubbed. See :func:`record_described`."""
    return record_described("gap", text)


def render(rec: dict) -> str:
    """The report as the Reporter will see it — and, later, as the upstream issue body.

    One renderer for both, deliberately: if what is shown and what is sent could drift,
    the Reporter's approval would stop meaning anything (docs/adr/0003).
    """
    p = rec["payload"]
    lines = []
    if p.get("text"):
        lines += [p["text"], ""]
    if p.get("exception_type"):
        lines.append(f"**{p['exception_type']}** in `charter {p.get('subcommand', '?')}`")
        if p.get("message"):
            # Flagged, not vouched for: this is the one field no allowlist could make safe.
            lines.append(f"> {p['message']}   ← free text, check before sending")
        if p.get("frames"):
            lines += ["", "```", *p["frames"], "```"]
        lines.append("")
    if rec.get("occurrences", 1) > 1:
        lines.append(f"Hit {rec['occurrences']} times on this machine.")
    lines.append(
        f"charter {p.get('charter_version')} · Python {p.get('python_version')} "
        f"· {p.get('os')}")
    return "\n".join(lines)


def mark_sent(report_id: str, issue_url: str) -> None:
    """Stamp a report with the upstream issue it became. Kept rather than deleted: a later
    identical crash points the Reporter at their own issue instead of drafting again."""
    rec = load(report_id)
    if rec:
        rec["issue_url"] = issue_url
        _write(rec)
