/**
 * **What changed in one file** (FM-11, #1103 story 21): the file against the branch's base,
 * committed or not, in a view tab of its own — what the preview's *Show what changed* opens.
 *
 * The comparison is the core's one diff engine (RC-2, ADR 0084): `what_changed` answers both
 * sides and git's hunks, and the merge view draws the hunks it was handed and diffs nothing of
 * its own (`hunks.ts`). **Nothing is ever drawn as an empty diff**: a file the branch did not
 * change, a binary one, one past 2 MiB, one charter cannot read, or a path the core refuses is
 * said in a sentence.
 *
 * **It reads again when the branch moves** (#1189): the window's `branch-changed` for this
 * branch asks once more, one read at a time, and the comparison drawn stays until the new one
 * comes, marked busy with *Comparing … again…* said as a status; its *Open at line N* buttons
 * stay usable meanwhile (they land in the file tab, which reads the file as it is now). The tab
 * never asks the core to watch a branch (`branch_watch` sets the window's whole
 * watched set, which is the explorer's): it hears a branch the explorer watches, and a branch
 * nobody watches is compared again with *Compare again*.
 *
 * **A change opens in the file tab at its line** (#984): each hunk's first line on the head's
 * side is offered, and a press is the window's one jump to a file and line (`fileJump.ts`): the
 * branch's file tab picks the file and lands on the line. The path goes through the core's read
 * there, so one the branch does not offer is refused as the viewer refuses it.
 *
 * **Each sentence is a status** (#1189): the reading, the refusal and the change with no line in
 * it are said to a screen reader as they come, and the merge view is named after the file and
 * what it is compared against.
 */
import { useEffect, useRef, useState } from "react";
import { FileDiff as FileDiffMark, LoaderCircle } from "lucide-react";
import { EmptyState } from "../EmptyState";
import { commands, type PlaneId, type WhatChanged } from "../bindings";
import type { Place } from "../pieceViews";
import { MergeViewer } from "./LightEditor";
import { sized, ToYourEditor } from "./PieceFiles";
import { jumpTo } from "../fileJump";
import { useBranchMoved } from "./branchMoved";

/** The comparison, as the tab draws it: being read, refused with the core's sentence, or read. */
type Read =
  { kind: "reading" } | { kind: "refused"; why: string } | { kind: "read"; shown: WhatChanged };

/**
 * One file of a branch against the branch's base. *Compare again* reads it again, from
 * *Comparing…*; the branch moving reads it again behind the comparison drawn.
 */
export function PieceDiffTab({ plane, cut, path }: { plane: PlaneId; cut: Place; path: string }) {
  const [asked, setAsked] = useState(0);
  /** How often the branch moved and was read again for it. */
  const [moved, setMoved] = useState(0);
  /** The last comparison told, with the asks and the moves it was read for. */
  const [read, setRead] = useState<{ asked: number; moved: number; read: Read }>();
  /** The read in flight, and whether the branch moved during it. */
  const flight = useRef<{ again: boolean }>(undefined);
  useEffect(() => {
    let gone = false;
    const now = { again: false };
    flight.current = now;
    const told = (got: Read) => {
      if (gone) return;
      flight.current = undefined;
      setRead({ asked, moved, read: got });
      if (now.again) setMoved((n) => n + 1);
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
  }, [plane, cut.workspace, cut.repo, cut.piece, path, asked, moved]);
  useBranchMoved(plane, cut, () => {
    if (flight.current !== undefined) flight.current.again = true;
    else setMoved((n) => n + 1);
  });
  const now: Read = read?.asked === asked ? read.read : { kind: "reading" };
  // The branch moved since the comparison drawn was read, and it is being read again behind it.
  const again = read?.asked === asked && read.moved !== moved;
  return (
    <div className="piece-file" aria-busy={again}>
      <header className="piece-files-head">
        <code>{path}</code>
        {now.kind === "read" && <span>{against(now.shown)}</span>}
        <span className="piece-files-actions">
          <ToYourEditor plane={plane} cut={cut} path={path} line={firstLine(now)} />
          <button type="button" tabIndex={0} onClick={() => setAsked((n) => n + 1)}>
            Compare again
          </button>
        </span>
      </header>
      {again && (
        <p className="piece-files-trouble" role="status">
          {`Comparing ${path} again…`}
        </p>
      )}
      <ChangedLines plane={plane} cut={cut} path={path} read={now} />
      <Compared path={path} read={now} />
    </div>
  );
}

/** How many changes are offered a line of their own: past it, the rest are counted. */
const LINES_OFFERED = 20;

/**
 * Each change's first line on the head's side, to open in the file tab (#984). None for a file
 * the branch deleted: the head has no line of it to land on.
 */
function ChangedLines({
  plane,
  cut,
  path,
  read,
}: {
  plane: PlaneId;
  cut: Place;
  path: string;
  read: Read;
}) {
  if (read.kind !== "read" || read.shown.mark === "deleted" || read.shown.diff.kind !== "text")
    return null;
  const lines = [...new Set(read.shown.diff.hunks.map((hunk) => Math.max(1, hunk.newStart)))];
  if (lines.length === 0) return null;
  const offered = lines.slice(0, LINES_OFFERED);
  const count = lines.length === 1 ? "1 change" : `${lines.length} changes`;
  return (
    <nav className="piece-files-head" aria-label={`Changes in ${path}`}>
      <span>
        {offered.length < lines.length ? `${count}, the first ${offered.length} here` : count}
      </span>
      <span className="piece-files-actions">
        {offered.map((line) => (
          <button
            key={line}
            type="button"
            tabIndex={0}
            onClick={() => jumpTo({ plane, place: cut, path, line })}
          >
            {`Open at line ${line}`}
          </button>
        ))}
      </span>
    </nav>
  );
}

/** The first line the change touches on the head's side, else the top: where *Open in your
 *  editor* opens the file. */
function firstLine(read: Read): number {
  if (read.kind !== "read" || read.shown.diff.kind !== "text") return 1;
  const first = read.shown.diff.hunks[0];
  if (first === undefined) return 1;
  return Math.max(1, first.newStart);
}

/** Where a sentence about a file the tab does not draw sends the operator. */
const TO_YOUR_EDITOR = "Open in your editor, at the top of this tab, shows it.";

/** The sentence for a change no line of which differs: what changed instead. */
function noLine(name: string, shown: WhatChanged, on: string): { headline: string; body?: string } {
  if (shown.mark === "renamed" && shown.from !== null)
    return { headline: `Only its name changed: moved from ${shown.from}` };
  if (shown.mark === "added") return { headline: `${name} was added, and it is empty` };
  if (shown.mark === "deleted") return { headline: `${name} was deleted, and it was empty` };
  return {
    headline: `No line of ${name} differs from ${on}`,
    body: "What changed is not in its lines: its mode, or whether it is a link.",
  };
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
    return (
      <EmptyState mark={LoaderCircle} headline={`Comparing ${path}…`} size="panel" role="status" />
    );
  if (read.kind === "refused")
    return (
      <EmptyState headline={read.why} size="panel" role="status" testid="piece-diff-trouble" />
    );
  const { diff, base } = read.shown;
  const said = noLine(name, read.shown, base ?? "its last commit");
  switch (diff.kind) {
    case "binary":
      return (
        <EmptyState
          mark={FileDiffMark}
          headline={`${name} is a binary file, so its lines are not compared`}
          body={TO_YOUR_EDITOR}
          size="panel"
          role="status"
          testid="piece-diff-trouble"
        />
      );
    case "too-large":
      return (
        <EmptyState
          mark={FileDiffMark}
          headline={`${name} is ${sized(diff.bytes)}, past what the comparison draws (2 MiB)`}
          body={TO_YOUR_EDITOR}
          size="panel"
          role="status"
          testid="piece-diff-trouble"
        />
      );
    case "text":
      return diff.hunks.length === 0 ? (
        <EmptyState
          mark={FileDiffMark}
          headline={said.headline}
          body={said.body}
          size="panel"
          role="status"
          testid="piece-diff-trouble"
        />
      ) : (
        <MergeViewer
          path={path}
          base={diff.base}
          head={diff.head}
          hunks={diff.hunks}
          label={`What changed in ${path} ${against(read.shown)}`}
        />
      );
  }
}
