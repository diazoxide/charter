/**
 * **A branch's files, and one file of a branch** (RC-5, FM-2): the light editor's two view tabs.
 *
 * - **Files · \<branch\>** is the branch as a tree (`BranchTree`, the explorer's own file rows)
 *   beside a read-only preview of the file picked, with a divider between them that can be
 *   dragged or moved from the keyboard. Where it was left is the tab's (`tabs.splits`), and the
 *   record keeps it for a tab open at a quit (`reopen::View::split`). One tab for a whole
 *   branch, so reading through it does not leave a tab per file behind.
 *   A jump to a file at a line (a search hit, FM-8; `fileJump.ts`) lands here: the preview picks
 *   the file and brings the line into view, and the tree opens the folders above the file and
 *   scrolls to its row (#1137).
 * - **\<file\> · \<branch\>** is one file in a tab of its own: what the preview's *Open in a
 *   tab of its own* opens, and what a jump from a diff, a record or the knowledge graph opens.
 *
 * **The preview** (#1103 F4) draws code in the light editor, an image as an image, markdown
 * rendered (with its source a press away), and for a binary file or one past 2 MiB a sentence
 * with its size instead of its contents. Both tabs only read. The core names the folder from the
 * branch, refuses a path that leaves it, and decides what a file is by its bytes.
 *
 * **Show what changed** (FM-11) is beside a file the branch changed against its base, and only
 * there, in both tabs (#1189): it opens the file's comparison in a view tab of its own
 * (`PieceDiff.tsx`).
 *
 * **Copy path** and **Reveal** (#1143) are beside the file in both, as a tree row's menu has
 * them (FM-10): the core places the file, as it does for the row.
 *
 * **Open in your editor** (RC-20, ADR 0081 §3) is beside the file in both: the file and the
 * line the cursor is on go to the editor chosen in Settings. The window sends the
 * branch, the path, the line and which editor; the core checks the path as it checks a read,
 * and builds the URL or the program's arguments itself.
 */
import { useEffect, useRef, useState, type ReactNode } from "react";
import Markdown, { type Components } from "react-markdown";
import { Group, Panel, Separator, type Layout } from "react-resizable-panels";
import { FileText, LoaderCircle } from "lucide-react";
import { EmptyState } from "../EmptyState";
import { ExternalLink } from "../ReleaseNotes";
import { commands, type ChangeMark, type PieceFile, type PlaneId } from "../bindings";
import {
  pieceDiffTitle,
  pieceDiffView,
  pieceFileTitle,
  pieceFileView,
  type Place,
} from "../pieceViews";
import { REVEAL_SAID, type Offer } from "../actions";
import type { ViewRef } from "../tabs";
import { BranchTree } from "./BranchTree";
import { LightEditor } from "./LightEditor";
import { useYourEditor } from "../yourEditor";
import { settleJump, usePendingJump } from "../fileJump";
import { DragHandle, PickAChat, type Referenced } from "../references";

/** A size, as a person reads one. */
export function sized(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KiB`;
  return `${Math.round(bytes / (1024 * 1024))} MiB`;
}

/**
 * One file, as the tabs draw it: still being read, refused with the core's sentence, or what
 * the core found it to be. One union, so every state is drawn in one place (#988).
 */
type FileRead = { kind: "reading" } | { kind: "refused"; why: string } | PieceFile;

const READING: FileRead = { kind: "reading" };

/**
 * One file of the branch, read from the core each time its path changes: the one path both tabs
 * read a file through (#988). `undefined` while no file is picked.
 */
function useFile(plane: PlaneId, cut: Place, path: string | undefined): FileRead | undefined {
  const [read, setRead] = useState<{ path: string; read: FileRead }>();
  useEffect(() => {
    if (path === undefined) return;
    let gone = false;
    const told = (read: FileRead) => {
      if (!gone) setRead({ path, read });
    };
    void commands
      .pieceFile(plane, cut.workspace, cut.repo, cut.piece, path)
      .then((answer) =>
        told(answer.status === "error" ? { kind: "refused", why: answer.error } : answer.data),
      )
      .catch((err: unknown) => told({ kind: "refused", why: String(err) }));
    return () => {
      gone = true;
    };
  }, [plane, cut.workspace, cut.repo, cut.piece, path]);
  if (path === undefined) return undefined;
  return read?.path === path ? read.read : READING;
}

/**
 * **Whether the branch changed the file at `path`** against its base, committed or not (FM-11):
 * its mark in `branch_status`, the explorer's own markers (FM-4), or `undefined` when it did not
 * or the status could not be read. Read when the tab opens and again when another file is picked,
 * **one read at a time**: a pick during a read asks for one more after it, never one each.
 */
function useChanged(plane: PlaneId, cut: Place, path: string | undefined): ChangeMark | undefined {
  const [marks, setMarks] = useState<ReadonlyMap<string, ChangeMark>>();
  const flight = useRef<{ again: boolean } | undefined>(undefined);
  const read = useRef<() => void>(() => {});
  useEffect(() => {
    read.current = () => {
      if (flight.current !== undefined) {
        flight.current.again = true;
        return;
      }
      const now = { again: false };
      flight.current = now;
      const told = (got: ReadonlyMap<string, ChangeMark>) => {
        flight.current = undefined;
        setMarks(got);
        if (now.again) read.current();
      };
      void commands
        .branchStatus(plane, cut.workspace, cut.repo, cut.piece)
        .then((said) =>
          told(
            said.status === "error"
              ? new Map()
              : new Map(said.data.changes.map((one) => [one.path, one.mark])),
          ),
        )
        .catch(() => told(new Map()));
    };
  });
  useEffect(() => {
    if (path !== undefined) read.current();
  }, [plane, cut.workspace, cut.repo, cut.piece, path]);
  if (path === undefined) return undefined;
  const mark = marks?.get(path);
  // A deleted file has nothing to pick; one that was, a moment ago, is not offered either.
  return mark === "deleted" ? undefined : mark;
}

/**
 * The line the cursor is on in the file at `path`: `start` (else 1) until it moves, and back to
 * that when another file is drawn.
 */
function useCursorLine(path: string | undefined, start?: number) {
  const [moved, setMoved] = useState<{ path: string | undefined; start?: number; line: number }>();
  const line =
    moved !== undefined && moved.path === path && moved.start === start ? moved.line : (start ?? 1);
  return { line, moved: (line: number) => setMoved({ path, start, line }) };
}

/**
 * What a header's button answered — the core's refusal, or what was done — said under the
 * buttons as a status: *Open in your editor*'s, *Copy path*'s and Reveal's.
 */
function Said({ children }: { children: ReactNode }) {
  return (
    <p className="piece-files-trouble" role="status">
      {children}
    </p>
  );
}

/**
 * *Open in your editor*: the button, and the sentence when nothing opened. With no editor
 * chosen it asks for one rather than guess.
 */
export function ToYourEditor({
  plane,
  cut,
  path,
  line,
}: {
  plane: PlaneId;
  cut: Place;
  path: string;
  line: number;
}) {
  const editor = useYourEditor();
  // What went wrong, for the file and editor it went wrong for: another file, or another
  // editor chosen, says nothing until it is tried.
  const [said, setSaid] = useState<{ about: string; trouble: string }>();
  const about = `${path}\n${editor ?? ""}`;
  const trouble = said?.about === about ? said.trouble : undefined;
  const say = (trouble: string | undefined) =>
    setSaid(trouble === undefined ? undefined : { about, trouble });
  const open = () => {
    if (editor === undefined) {
      say("Choose your editor in Settings first.");
      return;
    }
    say(undefined);
    void commands
      .openInYourEditor(plane, cut.workspace, cut.repo, cut.piece, path, line, editor)
      .then((answer) => {
        if (answer.status === "error") say(answer.error);
      })
      .catch((err: unknown) => say(String(err)));
  };
  return (
    <>
      <button type="button" tabIndex={0} onClick={open}>
        {`Open in your editor at line ${line}`}
      </button>
      {trouble !== undefined && <Said>{trouble}</Said>}
    </>
  );
}

/**
 * **Copy path and Reveal** (#1143): the file's path in the branch copied, or the file shown in
 * the platform's file manager — the core's `copy_branch_path` and `reveal_branch_path`, the same
 * commands a tree row's menu runs (FM-10). The core places the file and refuses what it will not
 * place; the sentence it answers is said under the buttons, as *Open in your editor*'s is.
 */
function CopyAndReveal({ plane, cut, path }: { plane: PlaneId; cut: Place; path: string }) {
  // What was said, for the file it was said about: another file says nothing until it is tried.
  const [said, setSaid] = useState<{ about: string; sentence: string }>();
  const sentence = said?.about === path ? said.sentence : undefined;
  const answered = (
    asked: Promise<{ status: "ok" } | { status: "error"; error: string }>,
    done?: string,
  ) => {
    setSaid(undefined);
    void asked
      .then((answer) => {
        const told = answer.status === "error" ? answer.error : done;
        if (told !== undefined) setSaid({ about: path, sentence: told });
      })
      .catch((err: unknown) => setSaid({ about: path, sentence: String(err) }));
  };
  return (
    <>
      <button
        type="button"
        tabIndex={0}
        onClick={() =>
          answered(
            commands.copyBranchPath(plane, cut.workspace, cut.repo, cut.piece, path, false),
            `Copied the path of ${path}.`,
          )
        }
      >
        Copy path
      </button>
      <button
        type="button"
        tabIndex={0}
        onClick={() =>
          answered(commands.revealBranchPath(plane, cut.workspace, cut.repo, cut.piece, path))
        }
      >
        {REVEAL_SAID}
      </button>
      {sentence !== undefined && <Said>{sentence}</Said>}
    </>
  );
}

/** The lines selected in the file at `path`, forgotten when another file is drawn. */
function useSelectedLines(path: string | undefined) {
  const [held, setHeld] = useState<{ path?: string; lines?: Lines }>({});
  const lines = held.path === path ? held.lines : undefined;
  return { lines, selected: (lines: Lines | undefined) => setHeld({ path, lines }) };
}

type Lines = { first: number; last: number };

/**
 * **The file into a chat** (FM-9): a handle to drag onto a chat, and *Ask a chat about this* and
 * *Add to a chat's context*, each naming the file — or the lines selected in it.
 */
function ToAChat({
  plane,
  cut,
  path,
  lines,
}: {
  plane: PlaneId;
  cut: Place;
  path: string;
  lines?: Lines;
}) {
  const referenced: Referenced = { plane, ...cut, path, folder: false, lines };
  return (
    <>
      <DragHandle referenced={referenced} />
      <PickAChat referenced={referenced} how="ask" />
      <PickAChat referenced={referenced} how="add" />
    </>
  );
}

/** Whether a file is markdown, by its name: what the preview renders rather than lists. */
function isMarkdown(path: string): boolean {
  return /\.(md|markdown)$/i.test(path);
}

/**
 * Markdown's headings one level under the preview's own, its links opened in the browser only
 * when they are http or https (`ReleaseNotes`), and an image said by its words: the window
 * loads nothing a file names (the CSP has no source for it), so a picture is never fetched.
 */
const MARKDOWN: Components = {
  h1: "h3",
  h2: "h4",
  h3: "h5",
  a: ({ href, children }) => <ExternalLink href={href}>{children}</ExternalLink>,
  img: ({ alt }) => (
    <span className="piece-markdown-image">{`[image${alt ? `: ${alt}` : ""}]`}</span>
  ),
};

/**
 * An image, drawn on a canvas from its bytes.
 *
 * **Decoded here, never loaded from a URL**: the window's CSP gives images no source but the
 * app's own (`tauri.conf.json`), and a `data:` or `blob:` source would be one more way for
 * whatever the window shows to fetch. `createImageBitmap` decodes bytes it is handed and loads
 * nothing. An animated image shows its first frame.
 */
function ImagePreview({ name, mime, base64 }: { name: string; mime: string; base64: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [drawn, setDrawn] = useState<{ of: string; size?: string; trouble?: string }>();
  useEffect(() => {
    let gone = false;
    const say = (got: { size?: string; trouble?: string }) => {
      if (!gone) setDrawn({ of: base64, ...got });
    };
    void (async () => {
      try {
        const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
        const bitmap = await createImageBitmap(new Blob([bytes], { type: mime }));
        const to = canvas.current;
        if (gone || to === null) {
          bitmap.close();
          return;
        }
        to.width = bitmap.width;
        to.height = bitmap.height;
        to.getContext("2d")?.drawImage(bitmap, 0, 0);
        say({ size: `${bitmap.width} × ${bitmap.height}` });
        bitmap.close();
      } catch (err) {
        say({ trouble: `purlis could not draw ${name}: ${String(err)}` });
      }
    })();
    return () => {
      gone = true;
    };
  }, [base64, mime, name]);
  const now = drawn?.of === base64 ? drawn : undefined;
  return (
    <figure className="piece-image">
      <canvas ref={canvas} role="img" aria-label={name} />
      <figcaption>
        {now?.trouble ?? (now?.size === undefined ? mime : `${now.size} · ${mime}`)}
      </figcaption>
    </figure>
  );
}

/** A file as the preview draws it, or the sentence that says why it does not. */
function Shown({
  path,
  read,
  line,
  onLine,
  onLines,
  source = false,
}: {
  path: string;
  read: FileRead;
  line?: number;
  onLine?: (line: number) => void;
  onLines?: (lines: Lines | undefined) => void;
  /** Markdown's source rather than the rendering. */
  source?: boolean;
}) {
  const name = path.slice(path.lastIndexOf("/") + 1);
  switch (read.kind) {
    case "reading":
      return <EmptyState mark={LoaderCircle} headline={`Reading ${path}…`} size="panel" />;
    case "refused":
      return <EmptyState headline={read.why} size="panel" testid="piece-file-trouble" />;
    case "text":
      return isMarkdown(path) && !source ? (
        <article className="piece-markdown release-notes" data-testid="piece-markdown">
          <Markdown skipHtml components={MARKDOWN}>
            {read.text}
          </Markdown>
        </article>
      ) : (
        <LightEditor path={path} text={read.text} line={line} onLine={onLine} onLines={onLines} />
      );
    case "image":
      return <ImagePreview name={name} mime={read.mime} base64={read.base64} />;
    case "huge-image":
      return (
        <EmptyState
          mark={FileText}
          headline={`${name} is ${read.width} × ${read.height} pixels, past what the preview draws (40 megapixels)`}
          body="Open it in your editor."
          size="panel"
        />
      );
    case "binary":
      return (
        <EmptyState
          mark={FileText}
          headline={`${name} is a binary file (${sized(read.bytes)})`}
          body="The preview draws text and images only."
          size="panel"
        />
      );
    case "too-large":
      return (
        <EmptyState
          mark={FileText}
          headline={`${name} is ${sized(read.bytes)}, past what the preview draws (2 MiB)`}
          body="Open it in your editor."
          size="panel"
        />
      );
  }
}

/**
 * One file of a branch, in a tab of its own, brought to `line` when one is given. *Show what
 * changed* is beside it where the branch changed it (#1189), as in the Files tab's preview.
 */
export function PieceFileTab({
  plane,
  cut,
  path,
  line,
  onOpenView,
}: {
  plane: PlaneId;
  cut: Place;
  path: string;
  line?: number;
  /** Opens a view tab: the file's comparison. No *Show what changed* without it. */
  onOpenView?: (view: ViewRef, title: string) => void;
}) {
  const read = useFile(plane, cut, path) ?? READING;
  const changed = useChanged(plane, cut, path);
  const at = useCursorLine(path, line);
  const selection = useSelectedLines(path);
  // A jump to a line (a diff, a record) lands in the text at that line; a file opened to be read
  // is rendered.
  const [source, setSource] = useState(line !== undefined);
  return (
    <div className="piece-file">
      <header className="piece-files-head">
        <code>{path}</code>
        <span className="piece-files-actions">
          <SourceToggle path={path} read={read} source={source} onSource={setSource} />
          <ToYourEditor plane={plane} cut={cut} path={path} line={at.line} />
          <CopyAndReveal plane={plane} cut={cut} path={path} />
          {changed !== undefined && onOpenView !== undefined && (
            <ShowWhatChanged cut={cut} path={path} onOpenView={onOpenView} />
          )}
          <ToAChat plane={plane} cut={cut} path={path} lines={selection.lines} />
        </span>
      </header>
      <Shown
        path={path}
        read={read}
        line={line}
        onLine={at.moved}
        onLines={selection.selected}
        source={source}
      />
    </div>
  );
}

/** *Show what changed*: the file's comparison against the branch's base, in a view tab. */
function ShowWhatChanged({
  cut,
  path,
  onOpenView,
}: {
  cut: Place;
  path: string;
  onOpenView: (view: ViewRef, title: string) => void;
}) {
  return (
    <button
      type="button"
      tabIndex={0}
      onClick={() => onOpenView(pieceDiffView(cut, path), pieceDiffTitle(cut, path))}
    >
      Show what changed
    </button>
  );
}

/** *Source*, beside a markdown file: its text in the light editor rather than rendered. */
function SourceToggle({
  path,
  read,
  source,
  onSource,
}: {
  path: string;
  read: FileRead;
  source: boolean;
  onSource: (source: boolean) => void;
}) {
  if (read.kind !== "text" || !isMarkdown(path)) return null;
  return (
    <button type="button" tabIndex={0} aria-pressed={source} onClick={() => onSource(!source)}>
      Source
    </button>
  );
}

/** The tree's panel, as the divider's layout names it. */
const TREE = "piece-files-tree";
/** The tree's share of the tab when it was never moved, and the shares it is held between. */
const SPLIT = { start: 30, least: 10, most: 70 };
/** Where the operator last left a file tab's divider in this window: where a branch's tab that
 *  has never been moved starts, so a second branch opens at the width the first was given. */
let lastSplit: number | undefined;

/**
 * A branch's files: the tree, and the file picked beside it in the preview, with a divider
 * between them that is dragged or moved with the arrow keys (`react-resizable-panels`, as the
 * window's own regions are). Where it is left is told to the tab (`onSplit`).
 */
export function PieceFilesTab({
  plane,
  cut,
  onOpenView,
  split,
  onSplit,
  onPress,
}: {
  plane: PlaneId;
  cut: Place;
  onOpenView: (view: ViewRef, title: string) => void;
  /** A row of the tree had its menu used (FM-10): carried out as the window carries any row. */
  onPress?: (offer: Offer) => void;
  /** Where the tab's divider was left: the tree's share in percent. */
  split?: number;
  /** The operator moved the divider. */
  onSplit?: (split: number) => void;
}) {
  const [picked, setPicked] = useState<string>();
  const [source, setSource] = useState<{ path: string; on: boolean }>();
  // **A jump lands here** (FM-8): the file it names is picked and its line brought into view.
  // Taken while drawing, the way React keeps what an earlier render saw, so the preview never
  // draws the old file first; settled after, so a tab opened later does not land on it again.
  const jump = usePendingJump(plane, cut);
  const [landed, setLanded] = useState<{ at: number; path: string; line: number }>();
  if (jump !== undefined && jump.at !== landed?.at) {
    setLanded({ at: jump.at, path: jump.path, line: jump.line });
    setPicked(jump.path);
  }
  useEffect(() => {
    if (landed !== undefined) settleJump(landed.at);
  }, [landed]);
  const line = landed !== undefined && landed.path === picked ? landed.line : undefined;
  const at = useCursorLine(picked, line);
  const selection = useSelectedLines(picked);
  const read = useFile(plane, cut, picked);
  const changed = useChanged(plane, cut, picked);
  // **Read once**: a panel's size is a constraint, and a constraint that changes re-registers
  // the panel (`RegionFrame.tsx`). The divider is where the operator's hand put it already.
  const [started] = useState(() =>
    Math.min(SPLIT.most, Math.max(SPLIT.least, Math.round(split ?? lastSplit ?? SPLIT.start))),
  );
  const settled = (layout: Layout, meta: { isUserInteraction: boolean }) => {
    // The layout is reported on mount too; only a drag or a key is the operator's.
    const tree = layout[TREE];
    if (!meta.isUserInteraction || typeof tree !== "number") return;
    lastSplit = Math.round(tree);
    onSplit?.(lastSplit);
  };
  // A line jumped to is in the text, so a markdown file shows its source, as its own tab does.
  const showsSource =
    source !== undefined && source.path === picked ? source.on : line !== undefined;

  return (
    <Group className="piece-files" orientation="horizontal" onLayoutChanged={settled}>
      <Panel
        id={TREE}
        defaultSize={`${started}%`}
        minSize={`${SPLIT.least}%`}
        maxSize={`${SPLIT.most}%`}
      >
        <BranchTree
          plane={plane}
          place={cut}
          picked={picked}
          reveal={landed?.at}
          onPick={setPicked}
          onPress={onPress}
        />
      </Panel>
      <Separator className="piece-files-split" aria-label="Resize the file tree" />
      <Panel id="piece-files-preview" minSize={`${100 - SPLIT.most}%`}>
        <section className="piece-files-shown" aria-label={picked ?? "No file picked"}>
          {picked === undefined || read === undefined ? (
            <EmptyState
              mark={FileText}
              headline="Pick a file to read it"
              body="Arrows move through the tree, Right opens a folder, and Enter shows a file here."
              size="panel"
            />
          ) : (
            <>
              <header className="piece-files-head">
                <code>{picked}</code>
                <span className="piece-files-actions">
                  <SourceToggle
                    path={picked}
                    read={read}
                    source={showsSource}
                    onSource={(on) => setSource({ path: picked, on })}
                  />
                  <ToYourEditor plane={plane} cut={cut} path={picked} line={at.line} />
                  <CopyAndReveal plane={plane} cut={cut} path={picked} />
                  <button
                    type="button"
                    tabIndex={0}
                    onClick={() =>
                      onOpenView(pieceFileView(cut, picked), pieceFileTitle(cut, picked))
                    }
                  >
                    Open in a tab of its own
                  </button>
                  {changed !== undefined && (
                    <ShowWhatChanged cut={cut} path={picked} onOpenView={onOpenView} />
                  )}
                  <ToAChat plane={plane} cut={cut} path={picked} lines={selection.lines} />
                </span>
              </header>
              <Shown
                path={picked}
                read={read}
                line={line}
                onLine={at.moved}
                onLines={selection.selected}
                source={showsSource}
              />
            </>
          )}
        </section>
      </Panel>
    </Group>
  );
}
