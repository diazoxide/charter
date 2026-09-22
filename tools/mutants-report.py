#!/usr/bin/env python3
"""Turn one `cargo mutants` run into an answer somebody can read.

`.github/workflows/mutants.yml` runs `cargo mutants` in shards and this script decides what
each shard's output MEANS. It exists because the exit code does not say enough: on
2026-09-21 the nightly was red for the fifth night running and the red said "failure" for
three different reasons at once — three shards killed at GitHub's six-hour job limit with
41%, 29% and 44% of their work done, one shard evicted at 23 minutes, and, underneath all
of that, three real surviving mutants nobody had read. A colour that means four things
means nothing, and fifty PRs merged past it.

So the two questions are asked separately and answered separately:

* **`shard`** — did this shard's run FINISH? A shard has a planned list (`mutants.json`,
  written before the first test) and a done list (`outcomes.json`, appended as it goes).
  Equal, the shard ran to the end; short, it was killed, and by how much says whether the
  budget is wrong or the runner went away. Surviving mutants do NOT enter into it: a shard
  that tested everything it was given did its job, whatever it found.
* **`gather`** — is there anything to REPORT? Survivors and timeouts from every shard,
  collected into one table, with the shards that never reported named rather than quietly
  dropped from the denominator.

Each mode writes GitHub's step summary when `$GITHUB_STEP_SUMMARY` is set, so the answer is
on the run's own page rather than at the end of a 6,000-line log.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import sys

# ---------------------------------------------------------------------------- #
# reading one shard                                                              #
# ---------------------------------------------------------------------------- #

# cargo-mutants' own names for an outcome, in the order a reader wants them.
SURVIVING = ("MissedMutant", "Timeout")
COUNTED = ("CaughtMutant", "MissedMutant", "Timeout", "Unviable", "Success", "Failure")


def read_shard(out_dir: pathlib.Path, shard: str) -> dict:
    """The verdict for one shard, as data. Never raises on a missing or truncated file:
    a shard whose runner disappeared mid-write is exactly the case this has to describe."""
    verdict = {
        "shard": shard,
        "planned": None,
        "tested": 0,
        "counts": {},
        "survivors": [],
        "status": "no-output",
        "why": f"{out_dir} does not exist — cargo-mutants never started, or the runner went "
        "away before it wrote anything",
    }

    planned_path = out_dir / "mutants.json"
    outcomes_path = out_dir / "outcomes.json"

    if planned_path.exists():
        try:
            verdict["planned"] = len(json.loads(planned_path.read_text()))
        except (json.JSONDecodeError, OSError) as exc:
            verdict["why"] = f"{planned_path} could not be read: {exc}"
            return verdict

    if not outcomes_path.exists():
        if verdict["planned"] is not None:
            verdict["status"] = "broken"
            verdict["why"] = (
                f"cargo-mutants planned {verdict['planned']} mutants and wrote no outcomes at "
                "all — it died in the build, or before the first test finished"
            )
        return verdict

    try:
        outcomes = json.loads(outcomes_path.read_text())
    except (json.JSONDecodeError, OSError) as exc:
        verdict["status"] = "broken"
        verdict["why"] = (
            f"{outcomes_path} is not readable JSON ({exc}) — the process was killed while "
            "writing it"
        )
        return verdict

    counts: dict[str, int] = {}
    survivors: list[dict] = []
    baseline_failed = None
    for outcome in outcomes.get("outcomes", []):
        scenario = outcome.get("scenario")
        summary = outcome.get("summary", "?")
        if scenario == "Baseline":
            if summary != "Success":
                baseline_failed = summary
            continue
        counts[summary] = counts.get(summary, 0) + 1
        if summary in SURVIVING:
            mutant = (scenario or {}).get("Mutant", {})
            survivors.append(
                {
                    "name": mutant.get("name", "?"),
                    "file": mutant.get("file", "?"),
                    "summary": summary,
                }
            )

    verdict["counts"] = counts
    verdict["survivors"] = survivors
    verdict["tested"] = sum(counts.values())

    if baseline_failed is not None:
        # The shards run with `--baseline skip` because a separate job checks the unmutated
        # tree once; if a baseline ran here anyway and failed, every later result in this
        # shard is about a tree that was already red and none of them mean anything.
        verdict["status"] = "broken"
        verdict["why"] = (
            f"the unmutated baseline came back {baseline_failed}: charter-core's own tests do "
            "not pass, so nothing this shard says about a mutant is evidence"
        )
        return verdict

    if verdict["planned"] is None:
        verdict["status"] = "broken"
        verdict["why"] = (
            "there are outcomes but no mutants.json, so there is no way to tell whether the "
            "shard finished"
        )
        return verdict

    if verdict["tested"] < verdict["planned"]:
        done = 100.0 * verdict["tested"] / max(verdict["planned"], 1)
        verdict["status"] = "incomplete"
        verdict["why"] = (
            f"{verdict['tested']} of {verdict['planned']} mutants tested ({done:.0f}%) — the "
            "shard was killed before it finished. Either the per-shard budget no longer fits "
            "the crate (add shards) or this runner went away (read the log)"
        )
        return verdict

    verdict["status"] = "complete"
    verdict["why"] = f"all {verdict['planned']} mutants in this shard were tested"
    return verdict


# ---------------------------------------------------------------------------- #
# writing it down                                                                #
# ---------------------------------------------------------------------------- #


def summary(text: str) -> None:
    """Append to GitHub's step summary if there is one, and always to stdout, so the same
    words are in the log for anyone reading it from the CLI."""
    print(text)
    path = os.environ.get("GITHUB_STEP_SUMMARY")
    if path:
        with open(path, "a", encoding="utf-8") as handle:
            handle.write(text + "\n")


def cmd_shard(args: argparse.Namespace) -> int:
    verdict = read_shard(pathlib.Path(args.output), args.shard)
    pathlib.Path(args.verdict).write_text(json.dumps(verdict, indent=1))

    counts = ", ".join(f"{k} {v}" for k, v in sorted(verdict["counts"].items())) or "nothing"
    summary(f"### shard {verdict['shard']}: {verdict['status']}")
    summary("")
    summary(verdict["why"])
    summary("")
    summary(f"`{counts}`")

    # A shard is red ONLY when its run did not happen properly. Survivors are the `gather`
    # job's answer, and mixing them in here is what made five nights of red unreadable.
    if verdict["status"] == "complete":
        return 0
    return 1


def key_of(survivor: dict) -> str:
    """The name a survivor is remembered by, between nights.

    NOT `file:line:col`, which is what cargo-mutants prints: a line number moves every time
    anybody edits above it, so a baseline keyed on one would call the whole crate new after a
    one-line insertion. The file, the function and the mutation together are stable across a
    move, and cargo-mutants already puts the function in its description.

    The cost is that two identical mutations of the same operator in the same function share
    one key — 141 survivors measured on 2026-09-21 collapse to 118. That errs towards silence,
    which is the right direction for a key whose job is to decide what is NEW.
    """
    return f"{survivor['file']}: {survivor['name'].split(': ', 1)[-1]}"


def read_known(path: pathlib.Path) -> set[str]:
    if not path.exists():
        # Every survivor then reads as new, which is the right way round for a missing file:
        # a backlog nobody can find must not quietly become an empty one.
        print(f"::warning::{path} does not exist; every survivor will be reported as new")
        return set()
    known = set()
    for line in path.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            known.add(line)
    return known


def cmd_gather(args: argparse.Namespace) -> int:
    root = pathlib.Path(args.verdicts)
    verdicts = []
    # A run whose `baseline` job failed never reaches the shards, so there is no artifact
    # directory at all. That is a report with nothing in it, not a crash.
    for path in sorted(root.rglob("verdict-*.json")) if root.is_dir() else []:
        try:
            verdicts.append(json.loads(path.read_text()))
        except (json.JSONDecodeError, OSError) as exc:
            print(f"::warning::{path} is unreadable: {exc}", file=sys.stderr)

    reported = {str(v["shard"]) for v in verdicts}
    silent = [str(n) for n in range(args.shards) if str(n) not in reported]
    complete = [v for v in verdicts if v["status"] == "complete"]
    whole_crate = len(complete) == args.shards
    survivors = [s for v in verdicts for s in v["survivors"]]
    tested = sum(v["tested"] for v in verdicts)
    planned = sum(v["planned"] or 0 for v in verdicts)

    by_key: dict[str, dict] = {}
    for survivor in survivors:
        by_key.setdefault(key_of(survivor), survivor)

    known = read_known(pathlib.Path(args.known))
    fresh = sorted(set(by_key) - known)
    # Only meaningful when the whole crate ran: a mutant that was never tested is not a
    # mutant that is now caught.
    mended = sorted(known - set(by_key)) if whole_crate else []

    # The full current set, so re-seeding the baseline is a copy rather than an edit.
    pathlib.Path(args.write_current).write_text("\n".join(sorted(by_key)) + "\n")

    summary("# nightly mutation testing")
    summary("")
    summary(
        f"**{len(complete)} of {args.shards} shards finished.** "
        f"{tested} mutants tested of {planned} planned across the shards that reported, "
        f"{len(by_key)} distinct survivors, **{len(fresh)} of them new**."
    )
    summary("")

    if silent:
        # Named, not dropped: a shard whose job was cancelled uploads nothing, and quietly
        # shrinking the denominator is how a partial run reads as a clean one.
        summary(
            f"> Shards {', '.join(silent)} reported nothing at all — their jobs did not reach "
            "the step that writes a verdict. Whatever mutants they held are UNTESTED, and are "
            "not counted anywhere below."
        )
        summary("")

    for verdict in sorted(verdicts, key=lambda v: str(v["shard"])):
        if verdict["status"] != "complete":
            summary(f"* shard {verdict['shard']}: **{verdict['status']}** — {verdict['why']}")
    if any(v["status"] != "complete" for v in verdicts):
        summary("")

    if mended:
        summary(f"## {len(mended)} survivors are now caught")
        summary("")
        summary(
            f"A test started noticing these. Delete them from `{args.known}` — this is a note "
            "and never a failure, because getting better must not turn the nightly red."
        )
        summary("")
        for item in mended:
            summary(f"* `{item}`")
        summary("")

    if not fresh and tested == 0:
        # Green here would be a lie of omission, but it is not THIS job's red: `baseline` or
        # `core` is already red and saying why. What this job owes the reader is not pretending
        # an empty run was a clean one.
        summary("## Nothing ran")
        summary("")
        summary(
            "No shard reported a single tested mutant, so this run says nothing at all about "
            "charter-core. The `baseline` and `core` jobs above say why."
        )
        return 0

    if not fresh:
        summary("## Nothing new")
        summary("")
        summary(
            f"Every survivor this run found is already written down in `{args.known}`. That "
            "file is a backlog, not an absolution: it was seeded from a run that covered 38% "
            "of the crate, and each line in it is still a change to charter-core no test "
            "notices."
        )
        return 0

    summary(f"## {len(fresh)} NEW surviving mutants")
    summary("")
    summary(
        "Each line is a change to charter-core that **no test noticed**, and that was not "
        "there before. That is either a test worth writing or an equivalent mutation worth "
        "writing down as one — `realpath` in `crates/charter-core/src/pypath.rs` is what the "
        "second looks like when it is done honestly, including the part where the mutation "
        "turned out to be catchable after all.\n\n"
        f"Accepting one into the backlog means adding its line to `{args.known}`, with a "
        "reason."
    )
    summary("")
    summary("| outcome | file | mutation |")
    summary("| --- | --- | --- |")
    for item in fresh:
        survivor = by_key[item]
        file, _, shown = item.partition(": ")
        summary(f"| {survivor['summary']} | `{file}` | {shown.replace('|', chr(92) + '|')} |")

    return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)

    shard = sub.add_parser("shard", help="judge one shard's mutants.out")
    shard.add_argument("--output", required=True, help="the mutants.out directory")
    shard.add_argument("--shard", required=True, help="this shard's index, for the report")
    shard.add_argument("--verdict", required=True, help="where to write verdict JSON")
    shard.set_defaults(func=cmd_shard)

    gather = sub.add_parser("gather", help="collect every shard's verdict into one report")
    gather.add_argument("--verdicts", required=True, help="directory of downloaded artifacts")
    gather.add_argument("--shards", type=int, required=True, help="how many shards there are")
    gather.add_argument(
        "--known",
        default=".github/mutants-survivors.txt",
        help="the survivors already written down; only NEW ones fail this",
    )
    gather.add_argument(
        "--write-current",
        default="mutants-survivors.current.txt",
        help="where to write this run's full survivor set, for re-seeding --known",
    )
    gather.set_defaults(func=cmd_gather)

    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
