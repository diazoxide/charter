"""`charter report` — handlers for drafting a report about charter itself.

Two commands, because the second one **is** the consent (docs/adr/0003). These are the
first: they draft, scrub, store and *show*. Nothing here touches the network — that is
what lets detection default to on without charter reading as telemetry.

charter has no interactive prompt anywhere (`util.py` carries only info/ok/warn/err),
because it runs inside hooks and agent sessions where blocking on stdin would hang. So
approval could not be a y/n prompt; it is a second command instead.
"""

from __future__ import annotations

import sys
from pathlib import Path

from . import __version__, report, util


def _body(args) -> str | None:
    """The report body, from a file, from stdin, or from argv. ``None`` on a read error.

    A body worth filing is exactly the text that does not survive a shell argument: it
    carries backticks, ``$``, quotes and fenced code. Passed inline, backticked terms are
    command-substituted **away** — "when `open` returns" is stored as "when  returns", a
    word silently deleted from a sentence still grammatical enough to skim past — and
    ``$(…)`` inside a code sample executes.

    charter already treats "do not put this on argv" as first-class for `secret set`, and
    these are the same two flags spelled the same way. There the reason is disclosure; here
    it is corruption, and the stakes are comparable: a report is published irreversibly, on
    a public tracker, under the reporter's own identity, and the material that gets mangled
    is the code sample that made it worth filing.
    """
    src = getattr(args, "from_file", None)
    if src:
        try:
            return Path(src).expanduser().read_text()
        except OSError as e:
            util.err(f"could not read {src}: {e}")
            return None
    text = getattr(args, "text", None)
    # `-` is the conventional spelling and costs nothing to honour.
    if getattr(args, "stdin", False) or text == "-":
        return sys.stdin.read()
    if text:
        return text
    # A body arriving down a pipe with no flag is the ordinary shape, exactly as it is for
    # `secret set`, whose docstring records that demanding an explicit `--stdin` broke every
    # working pipeline to prevent a mistake an emptiness check already catches.
    if not sys.stdin.isatty():
        return sys.stdin.read()
    return ""


def _draft(kind: str, text: str) -> int:
    if not (text or "").strip():
        util.err(f"nothing to report — describe the {kind}.")
        util.info(f"  short:  charter report {kind} \"<what>\"")
        util.info(f"  long:   charter report {kind} --from-file <path>   "
                  f"(or --stdin, or `-`)")
        util.info("  A long body does not survive the shell — backticks and `$(…)` are "
                  "expanded before charter ever sees them.")
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
    body = _body(args)
    return 1 if body is None else _draft("bug", body)


def cmd_report_gap(args) -> int:
    """Draft a **gap** — charter cannot do something it should."""
    body = _body(args)
    return 1 if body is None else _draft("gap", body)


def cmd_report_consent(args) -> int:
    """Record, once, that this human agrees to publish under their own GitHub identity.

    Its own command rather than a flag on `send`: ADR 0003 rejects flags that collapse
    approval into the sending call, because a flag an agent can pass is a flag it will
    pass every time. A one-off command has a single, auditable purpose.
    """
    report.grant_consent()
    util.ok(f"Consent recorded → {report.consent_path()}")
    util.info("`charter report send` may now open issues on "
              f"{report.upstream_repo()} as you. Delete that file to withdraw.")
    return 0


def cmd_report_send(args) -> int:
    """Publish a drafted report. **This is the consent step** — the only command here
    that touches the network."""
    rid = getattr(args, "id", None)
    rec = report.load(rid) if rid else None
    if not rec:
        util.err(f"no drafted report '{rid}'. List them: charter report list")
        return 1

    if rec.get("issue_url"):
        util.info(f"Already reported → {rec['issue_url']}")
        return 0

    if not report.has_consent():
        util.err("publishing sends this to a PUBLIC tracker under your own GitHub "
                 "identity, and that has not been agreed yet.")
        util.info("Read the draft first:  charter report show " + rid)
        util.info("Then agree once with:  charter report consent")
        return 1

    _warn_if_stale()

    # Before any network call, deliberately. A dry run is the offline affordance — it is
    # how this feature was developed without spamming a real tracker — so it must not
    # depend on `gh`, and a duplicate must not short-circuit it before it has shown the
    # Reporter the payload they asked to see.
    if getattr(args, "dry_run", False):
        util.ok("dry run — nothing sent, no network touched. This would open:")
        print(f"  {report.upstream_repo()}: {report.title(rec)}")
        print(report.issue_body(rec))
        return 0

    try:
        if not report.gh_available():
            # Not an error: a GitLab-only Reporter has no `gh` identity, and a broken or
            # rate-limited `gh` should never lose the report either.
            util.warn("no usable `gh` — open this prefilled issue instead:")
            print(report.fallback_url(rec))
            return 0

        if not getattr(args, "new", False):
            dups = report.search_duplicates(rec)
            if dups:
                return _report_duplicates(dups, rid)

        url = report.create_issue(rec)
    except report.ReportingError as e:
        # Loud, never swallowed: a silent failure here means the Reporter believes they
        # reported something they did not (docs/adr/0002).
        util.err(f"could not open the issue: {e}")
        util.info("Your draft is safe. Retry, or use: charter report send "
                  f"{rid} --dry-run to see exactly what it would send.")
        return 1

    report.mark_sent(rid, url)
    util.ok(f"Reported → {url}")
    return 0


def cmd_report_list(args) -> int:
    """Every report on this machine — drafts awaiting approval, and what was already sent."""
    recs = report.all_reports()
    if not recs:
        util.info("No reports drafted. charter records one automatically when it crashes; "
                  "describe one yourself with: charter report bug|gap \"<what>\"")
        return 0
    for r in recs:
        state = r["issue_url"] if r.get("issue_url") else "not sent"
        hits = f" ×{r['occurrences']}" if r.get("occurrences", 1) > 1 else ""
        print(f"  {r['id']}  {r.get('kind', '?'):4}{hits:5}  {report.title(r)}\n      {state}")
    return 0


def cmd_report_show(args) -> int:
    """Print one report exactly as it would be published."""
    rec = report.load(getattr(args, "id", None))
    if not rec:
        util.err(f"no report '{getattr(args, 'id', None)}'. List them: charter report list")
        return 1
    print(report.render(rec))
    if rec.get("issue_url"):
        util.info(f"Already reported → {rec['issue_url']}")
    else:
        util.info(f"Not sent. Send it with: charter report send {rec['id']}")
    return 0


def cmd_report_delete(args) -> int:
    """Discard a report drafted on this machine.

    `not sent` is a TODO, and its worth is that it is actionable. A superseded draft that
    can never be removed turns `report list` into a list with a permanent false positive,
    and a list you cannot trust is one where a real pending report gets missed among the
    dead ones — the failure mode of a suite full of permanently-skipped tests (#155).

    A SENT report needs `--force`, and not merely as ceremony: `prune` keeps sent reports
    forever on purpose, because they are what lets a later identical crash point at the
    existing upstream issue instead of drafting a duplicate. Deleting one removes that
    pointer, and with it the only local trace of something that exists publicly under the
    Reporter's identity.
    """
    rid = getattr(args, "id", None)
    rec = report.load(rid)
    if not rec:
        util.err(f"no report '{rid}'. List them: charter report list")
        return 1
    if rec.get("issue_url") and not getattr(args, "force", False):
        util.err(f"'{rid}' was already sent → {rec['issue_url']}")
        util.info("Kept so a later identical crash can point at that issue instead of "
                  "filing a duplicate. Discard it anyway with --force.")
        return 2
    report.delete(rid)
    util.ok(f"Discarded report '{rid}'.")
    return 0


def cmd_report_comment(args) -> int:
    """Add this Reporter's details to an existing upstream issue.

    The default answer to a confirmed duplicate: a repeat carries a second environment
    where the problem reproduces, which is the most useful thing about it — dropping it
    silently is the one outcome that wastes it.
    """
    rid = getattr(args, "id", None)
    rec = report.load(rid) if rid else None
    if not rec:
        util.err(f"no drafted report '{rid}'. List them: charter report list")
        return 1
    if not report.has_consent():
        util.err("commenting publishes under your own GitHub identity, which has not "
                 "been agreed yet. Agree once with: charter report consent")
        return 1

    issue = str(getattr(args, "on", "") or "").lstrip("#")
    try:
        report.comment_on(issue, rec)
    except report.ReportingError as e:
        util.err(f"could not comment on #{issue}: {e}")
        return 1

    url = f"https://github.com/{report.upstream_repo()}/issues/{issue}"
    report.mark_sent(rid, url)
    util.ok(f"Added your details to → {url}")
    return 0


def _report_duplicates(dups: list[dict], rid: str) -> int:
    """Stop and hand the candidates to whoever can judge them.

    Not a verdict: a keyword score cannot tell "clone fails on a private repo" from
    "clone fails on a submodule". And not a silent drop — a duplicate carries a second
    environment where the bug reproduces, which is the most useful thing about it.
    """
    util.warn("this may already be reported:")
    for d in dups:
        print(f"  #{d.get('number')}  {d.get('title')}\n      {d.get('url')}")
    util.info(f"Add your details to one:  charter report comment {rid} --on <number>")
    util.info(f"Or file anyway:           charter report send {rid} --new")
    return 2


def _warn_if_stale() -> None:
    """Warn, never block. A bug that survives on an old charter is still worth having, and
    "upgrade first, then report" is a reliable way to never get the report at all.

    Carries the channel chip (#458). On the dev channel ``{latest}`` is not a published
    release at all — `update.newer_than` hands off to `newer_head`, so the number beside
    "is out" is `main`'s head commit — and "0.52.0 is out" read on a dev plane is ambiguous
    between *a release was cut* and *main moved*. That is the exact ambiguity
    `statusline._dev_chip` exists to resolve, and #457 already made it one function so a
    third surface can call it rather than re-derive `channel.is_dev()` and its try/except.
    """
    try:
        from . import statusline, update
        latest = update.newer_than(__version__)
        if latest:
            # `util.color_enabled()`, not the chip's ANSI default: this is the one caller
            # that is not a terminal surface, and `util.warn` gates its own glyph the same
            # way. The call stays INSIDE the f-string on purpose — `test_version_shows_
            # channel`'s AST property looks for it in the same `JoinedStr` as the version.
            color = util.color_enabled()
            util.warn(f"you are on charter {__version__}{statusline._dev_chip(color)}; "
                      f"{latest} is out — this may already be fixed. Reporting anyway is "
                      "fine.")
    except Exception:  # noqa: BLE001 - a staleness check must never block a report
        pass
