/**
 * **What changed in one file** (FM-11, #1103 story 21): the file against the branch's base,
 * committed or not, in a view tab of its own — what the preview's *Show what changed* opens.
 *
 * The comparison is the core's one diff engine (RC-2, ADR 0084): `what_changed` answers both
 * sides and git's hunks, and the merge view draws the hunks it was handed and diffs nothing of
 * its own (`hunks.ts`). **Nothing is ever drawn as an empty diff**: a file the branch did not
 * change, a binary one, one past 2 MiB, one charter cannot read, or a path the core refuses is
 * said in a sentence.
 */
import { useEffect, useState } from "react";
import { FileDiff as FileDiffMark, LoaderCircle } from "lucide-react";
import { EmptyState } from "../EmptyState";
import { commands, type PlaneId, type WhatChanged } from "../bindings";
import type { Place } from "../pieceViews";
import { MergeViewer } from "./LightEditor";
import { sized } from "./PieceFiles";

/** The comparison, as the tab draws it: being read, refused with the core's sentence, or read. */
type Read =
  { kind: "reading" } | { kind: "refused"; why: string } | { kind: "read"; shown: WhatChanged };

/** One file of a branch against the branch's base. `Compare again` reads it again. */
export function PieceDiffTab({ plane, cut, path }: { plane: PlaneId; cut: Place; path: string }) {
  const [asked, setAsked] = useState(0);
  const [read, setRead] = useState<{ asked: number; read: Read }>();
  useEffect(() => {
    let gone = false;
    const told = (got: Read) => {
      if (!gone) setRead({ asked, read: got });
    };
    void commands
      .whatChanged(plane, cut.workspace, cut.repo, cut.piece, path)
      .then((answer) =>
        told(
          answer.status === "error"
            ? { kind: "refused", why: answer.error }
            : { kind: "read", shown: answer.data },
        ),
      )
      .catch((err: unknown) => told({ kind: "refused", why: String(err) }));
    return () => {
      gone = true;
    };
  }, [plane, cut.workspace, cut.repo, cut.piece, path, asked]);
  const now: Read = read?.asked === asked ? read.read : { kind: "reading" };
  return (
    <div className="piece-file">
      <header className="piece-files-head">
        <code>{path}</code>
        {now.kind === "read" && <span>{against(now.shown)}</span>}
        <span className="piece-files-actions">
          <button type="button" tabIndex={0} onClick={() => setAsked((n) => n + 1)}>
            Compare again
          </button>
        </span>
      </header>
      <Compared path={path} read={now} />
    </div>
  );
}

/** What the file is compared against, and where it came from when it moved. */
function against(shown: WhatChanged): string {
  const base = shown.base === null ? "its last commit" : shown.base;
  const moved = shown.from === null ? "" : `, moved from ${shown.from}`;
  return `against ${base}${moved}`;
}

/** The comparison drawn, or the sentence that says why it is not. */
function Compared({ path, read }: { path: string; read: Read }) {
  const name = path.slice(path.lastIndexOf("/") + 1);
  if (read.kind === "reading")
    return <EmptyState mark={LoaderCircle} headline={`Comparing ${path}…`} size="panel" />;
  if (read.kind === "refused")
    return <EmptyState headline={read.why} size="panel" testid="piece-diff-trouble" />;
  const { diff, base } = read.shown;
  const on = base ?? "its last commit";
  switch (diff.kind) {
    case "binary":
      return (
        <EmptyState
          mark={FileDiffMark}
          headline={`${name} is a binary file, so its lines are not compared`}
          body="Open it in your editor to see it."
          size="panel"
          testid="piece-diff-trouble"
        />
      );
    case "too-large":
      return (
        <EmptyState
          mark={FileDiffMark}
          headline={`${name} is ${sized(diff.bytes)}, past what the comparison draws (2 MiB)`}
          body="Open it in your editor to see it."
          size="panel"
          testid="piece-diff-trouble"
        />
      );
    case "text":
      return diff.hunks.length === 0 ? (
        <EmptyState
          mark={FileDiffMark}
          headline={`No line of ${name} differs from ${on}`}
          body="What changed is not in its lines: its mode, or whether it is a link."
          size="panel"
          testid="piece-diff-trouble"
        />
      ) : (
        <MergeViewer path={path} base={diff.base} head={diff.head} hunks={diff.hunks} />
      );
  }
}
