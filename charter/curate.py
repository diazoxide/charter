"""Deterministic memory-curation engine — the analysis + safe-auto half of memory
optimization. It finds curation candidates (exact/near duplicates, stale entries,
index drift, charter-worthy rules) and applies only the **tier-1 safe & reversible**
ops; the lossy / human-owned ops (near-dup merges, stale archives, charter edits) are
returned as **proposals** for the steward agent to quiz on. No LLM here — stdlib only,
fully testable; the semantic judgment lives in the agent layer on top of this.

Tiers mirror the steward's Hat 2 model:
  auto (safe, reversible) → collapse EXACT duplicates, repair the index
  propose (needs a human) → merge NEAR duplicates, archive stale, promote a rule to charter
"""

from __future__ import annotations

import datetime
from pathlib import Path

from . import contain, memstore

# Durable-RULE language worth promoting to the charter. Weighted toward IMPERATIVE,
# time-invariant phrasing ("standing rule", "always/never") and deliberately EXCLUDING
# ops-log markers ("gate"/"guard"/"deployed state") that fire on transient snapshots —
# those scored highest in a first pass and were exactly NOT charter-worthy. Deterministic
# nomination only; the steward judges + drafts the actual edit.
_RULE_MARKERS = {"standing rule": 5, "invariant": 3, "the rule is": 3, "must not": 2,
                 "always ": 2, "never ": 2, " must ": 1, "do not ": 1}
# Transient snapshots are the opposite of durable — never nominate them for the charter.
_TRANSIENT = ("deployed state", " as of ", "deploy of ", "verified live", "re-verif")


def _rule_score(text: str) -> int:
    low = (text or "").lower()
    if any(t in low for t in _TRANSIENT):
        return 0
    return sum(w * low.count(m) for m, w in _RULE_MARKERS.items())


def report(mem_dir: Path, stale_days: int = 90, near_threshold: float = 0.5,
           today=None) -> dict:
    """Read-only curation findings for one memory dir. Returns:
      total, exact_dups (groups of [filenames]), near_dups ([(jac, a, b)]),
      stale ([(filename, date_iso, age_days)]), index (orphans/missing), rules
      ([(filename, title, score)] charter-worthy candidates, strongest first)."""
    day = today or datetime.date.today()
    ents = memstore.entries(mem_dir)

    # exact duplicates — identical normalized body, grouped
    by_body: dict[str, list[str]] = {}
    for p, _t, text in ents:
        by_body.setdefault(memstore.body(text), []).append(p.name)
    exact = sorted([sorted(names) for names in by_body.values() if len(names) > 1])

    # near duplicates (excluding exact pairs, which are handled by tier-1)
    exact_pairs = {frozenset(g[:2]) for g in exact}
    near = []
    for jac, pa, _ta, pb, _tb in memstore.duplicates([mem_dir], near_threshold):
        if frozenset((pa.name, pb.name)) in exact_pairs:
            continue
        near.append((round(jac, 2), pa.name, pb.name))

    # stale — older than the window (age-based; a PROPOSAL, never auto: age != obsolete)
    stale = []
    for p, _t, text in ents:
        d = memstore.memory_date(text, p.name)
        if d and (day - d).days > stale_days:
            stale.append((p.name, d.isoformat(), (day - d).days))
    stale.sort(key=lambda x: -x[2])

    # index health — one implementation, shared with `doctor` so the two can never
    # disagree about what counts as drift.
    drift = memstore.index_drift(mem_dir)
    index = {"orphans": drift["dangling"],   # indexed but file gone
             "missing": drift["unindexed"]}  # file exists but unindexed

    # charter-worthy rule candidates
    rules = sorted(((p.name, t, _rule_score(x)) for p, t, x in ents if _rule_score(x) >= 2),
                   key=lambda r: -r[2])[:10]

    return {"total": len(ents), "exact_dups": exact, "near_dups": near,
            "stale": stale, "index": index, "rules": rules}


def apply_safe(mem_dir: Path) -> list[str]:
    """Apply ONLY the tier-1 safe & reversible ops and return a log of what was done:
      - collapse each exact-duplicate group → keep the first, ARCHIVE the rest (reversible)
      - repair the index (re-append any file that exists but isn't listed)
    Never merges, never archives-by-age, never touches a charter."""
    actions = []
    rep = report(mem_dir)
    for group in rep["exact_dups"]:
        for dup in group[1:]:                 # keep group[0], archive the redundant copies
            if memstore.archive(mem_dir, dup):
                actions.append(f"archived exact-duplicate {dup} (kept {group[0]})")
    # repair missing index links (safe: purely additive to MEMORY.md)
    missing = report(mem_dir)["index"]["missing"]  # recompute after archiving
    if missing:
        idx = memstore.index_path(mem_dir)
        # `apply_safe` returns a LOG the operator reads, and by here it may already have
        # archived files. A refusal is therefore reported into that log rather than raised
        # out of a half-finished batch — and reported rather than swallowed, because a
        # linked index turns this one command into N appends into whatever it points at,
        # which is the largest single write charter performs unprompted (#349).
        try:
            for p, title, _text in memstore.entries(mem_dir):
                if p.name in missing:
                    memstore.index_append(idx, p.name, title)
            actions.append(f"repaired index: linked {len(missing)} unindexed memory(ies)")
        except contain.Refused as e:
            actions.append(f"index NOT repaired — {e}")
    return actions


def pending_auto(rep: dict) -> list[str]:
    """What ``apply_safe`` *would* do — for the read-only report.

    Deliberately kept beside ``apply_safe``: a read-only report that hides a
    change the tool will make is worse than no report, because you approve
    ``--apply`` without knowing it rewrites ``MEMORY.md``. Every branch here
    must mirror one in ``apply_safe``.
    """
    out = []
    if rep["exact_dups"]:
        redundant = sum(len(g) - 1 for g in rep["exact_dups"])
        out.append(f"collapse {len(rep['exact_dups'])} exact-duplicate group(s) — "
                   f"archives {redundant} redundant copy(ies), reversible")
    missing = rep["index"]["missing"]
    if missing:
        shown = ", ".join(missing[:3]) + (", …" if len(missing) > 3 else "")
        out.append(f"repair index: link {len(missing)} memory(ies) that exist but "
                   f"aren't listed ({shown})")
    return out


def proposals(rep: dict) -> list[str]:
    """Human-readable tier-2 PROPOSALS from a report — what the steward should quiz on
    (never auto-applied): near-dup merges, stale archives, charter promotions."""
    out = []
    for jac, a, b in rep["near_dups"]:
        out.append(f"merge near-duplicates ({jac}): {a} + {b} → one canonical memory?")
    if rep["stale"]:
        n = len(rep["stale"])
        oldest = rep["stale"][0]
        out.append(f"archive {n} stale memory(ies) (age-based, oldest {oldest[2]}d — "
                   f"review first: age ≠ obsolete)?")
    for fn, title, score in rep["rules"]:
        out.append(f"promote to charter? [{fn}] \"{title[:60]}\" (rule-signal {score})")
    if rep["index"]["orphans"]:
        out.append(f"index lists {len(rep['index']['orphans'])} missing file(s) — "
                   f"stale links to prune: {', '.join(rep['index']['orphans'][:5])}")
    return out
