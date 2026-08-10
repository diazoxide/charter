"""`charter report` — handlers for drafting a report about charter itself.

Two commands, because the second one **is** the consent (docs/adr/0003). These are the
first: they draft, scrub, store and *show*. Nothing here touches the network — that is
what lets detection default to on without charter reading as telemetry.

charter has no interactive prompt anywhere (`util.py` carries only info/ok/warn/err),
because it runs inside hooks and agent sessions where blocking on stdin would hang. So
approval could not be a y/n prompt; it is a second command instead.
"""

from __future__ import annotations

from . import report, util


def _draft(kind: str, text: str) -> int:
    if not (text or "").strip():
        util.err(f"nothing to report — describe the {kind}: charter report {kind} \"<what>\"")
        return 1

    rid = report.record_described(kind, text)
    if rid is None:
        util.err(f"too many undrafted reports ({report.MAX_PENDING}); send or discard some first.")
        return 1

    rec = report.load(rid)
    # To stdout, and in full: the Reporter is being asked to approve this exact text, and
    # approving something they were never shown would make the approval meaningless.
    print(report.render(rec))
    print()
    print(f"id: {rid}")

    util.info("Nothing has been sent. Read the text above — it is exactly what would be "
              "published, on a PUBLIC tracker, under your own GitHub identity.")
    util.info(f"Send it with: charter report send {rid}")
    return 0


def cmd_report_bug(args) -> int:
    """Draft a **bug** the Reporter describes by hand. The automatic path (a real crash,
    with a traceback) records itself; this is for "it did the wrong thing"."""
    return _draft("bug", getattr(args, "text", None))


def cmd_report_gap(args) -> int:
    """Draft a **gap** — charter cannot do something it should."""
    return _draft("gap", getattr(args, "text", None))
