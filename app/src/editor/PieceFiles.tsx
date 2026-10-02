/**
 * **A piece's files, and one file of a piece** (RC-5): the light editor's two view tabs.
 *
 * - **Files · \<piece\>** lists every file of the piece (what git tracks and what it does not
 *   ignore, `charter_core::piecefiles::list`), narrows the list as the operator types, and
 *   draws the file picked beside it. One tab for a whole piece, so reading through a branch
 *   does not leave a tab per file behind.
 * - **\<file\> · \<piece\>** is one file in a tab of its own: what the list's *Open in a tab
 *   of its own* opens, and what a jump from a diff, a record or the knowledge graph will open.
 *
 * Both only read. The core names the folder from the piece, and refuses a path that leaves it.
 */
import { useEffect, useMemo, useState } from "react";
import { FileText, LoaderCircle } from "lucide-react";
import { EmptyState } from "../EmptyState";
import { commands, type PieceFile, type PlaneId } from "../bindings";
import type { Cut } from "../actions";
import { pieceFileTitle, pieceFileView } from "../pieceViews";
import type { ViewRef } from "../tabs";
import { LightEditor } from "./LightEditor";

/** How many matching paths the list draws at once. A repo of tens of thousands of files is
 *  narrowed by typing, not scrolled; the rest are counted under the list. */
const SHOWN = 500;

/** A size, as a person reads one. */
function sized(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KiB`;
  return `${Math.round(bytes / (1024 * 1024))} MiB`;
}

/** What the core answered for one file, or the sentence it refused with. */
type Read = { file?: PieceFile; trouble?: string };

/** One file of the piece, read from the core each time its path changes. */
function useFile(plane: PlaneId, cut: Cut, path: string | undefined): Read | undefined {
  const [read, setRead] = useState<{ path: string; read: Read }>();
  useEffect(() => {
    if (path === undefined) return;
    let gone = false;
    void commands
      .pieceFile(plane, cut.workspace, cut.repo, cut.piece, path)
      .then((answer) => {
        if (gone) return;
        setRead({
          path,
          read: answer.status === "error" ? { trouble: answer.error } : { file: answer.data },
        });
      })
      .catch((err: unknown) => {
        if (!gone) setRead({ path, read: { trouble: String(err) } });
      });
    return () => {
      gone = true;
    };
  }, [plane, cut.workspace, cut.repo, cut.piece, path]);
  return path !== undefined && read?.path === path ? read.read : undefined;
}

/** A file as the light editor draws it, or the sentence that says why it does not. */
function Shown({ path, read }: { path: string; read: Read | undefined }) {
  if (read === undefined) {
    return <EmptyState mark={LoaderCircle} headline={`Reading ${path}…`} size="panel" />;
  }
  if (read.trouble !== undefined) {
    return <EmptyState headline={read.trouble} size="panel" testid="piece-file-trouble" />;
  }
  const file = read.file;
  if (file === undefined) return null;
  const name = path.slice(path.lastIndexOf("/") + 1);
  switch (file.kind) {
    case "text":
      return <LightEditor path={path} text={file.text} />;
    case "binary":
      return (
        <EmptyState
          mark={FileText}
          headline={`${name} is a binary file (${sized(file.bytes)})`}
          body="The light editor draws text only."
          size="panel"
        />
      );
    case "too-large":
      return (
        <EmptyState
          mark={FileText}
          headline={`${name} is ${sized(file.bytes)}, past what the light editor draws (5 MiB)`}
          body="Open it in your editor."
          size="panel"
        />
      );
  }
}

/** One file of a piece, in a tab of its own. */
export function PieceFileTab({ plane, cut, path }: { plane: PlaneId; cut: Cut; path: string }) {
  const read = useFile(plane, cut, path);
  return (
    <div className="piece-file">
      <Shown path={path} read={read} />
    </div>
  );
}

/** A piece's files: the list, narrowed by what is typed, and the file picked beside it. */
export function PieceFilesTab({
  plane,
  cut,
  onOpenView,
}: {
  plane: PlaneId;
  cut: Cut;
  onOpenView: (view: ViewRef, title: string) => void;
}) {
  const [listed, setListed] = useState<{ files?: string[]; trouble?: string }>();
  const [wanted, setWanted] = useState("");
  const [picked, setPicked] = useState<string>();
  useEffect(() => {
    let gone = false;
    void commands
      .pieceFiles(plane, cut.workspace, cut.repo, cut.piece)
      .then((answer) => {
        if (gone) return;
        setListed(answer.status === "error" ? { trouble: answer.error } : { files: answer.data });
      })
      .catch((err: unknown) => {
        if (!gone) setListed({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [plane, cut.workspace, cut.repo, cut.piece]);
  const read = useFile(plane, cut, picked);

  const matching = useMemo(() => {
    const needle = wanted.trim().toLowerCase();
    const files = listed?.files ?? [];
    return needle === "" ? files : files.filter((path) => path.toLowerCase().includes(needle));
  }, [listed, wanted]);

  if (listed === undefined) {
    return <EmptyState mark={LoaderCircle} headline={`Listing the files of ${cut.piece}…`} />;
  }
  if (listed.trouble !== undefined) {
    return <EmptyState headline={listed.trouble} testid="piece-files-trouble" />;
  }
  return (
    <div className="piece-files">
      <nav className="piece-files-list" aria-label={`Files of ${cut.piece}`}>
        <input
          type="search"
          aria-label="Find a file"
          placeholder="Find a file"
          value={wanted}
          onChange={(event) => setWanted(event.target.value)}
        />
        <ul>
          {matching.slice(0, SHOWN).map((path) => (
            <li key={path}>
              <button
                type="button"
                tabIndex={0}
                aria-current={path === picked ? "true" : undefined}
                onClick={() => setPicked(path)}
                onDoubleClick={() =>
                  onOpenView(pieceFileView(cut, path), pieceFileTitle(cut, path))
                }
              >
                {path}
              </button>
            </li>
          ))}
        </ul>
        {matching.length > SHOWN && (
          <p className="none">{`${matching.length - SHOWN} more: type to narrow the list`}</p>
        )}
        {matching.length === 0 && <p className="none">No file matches</p>}
      </nav>
      <section className="piece-files-shown" aria-label={picked ?? "No file picked"}>
        {picked === undefined ? (
          <EmptyState
            mark={FileText}
            headline={`${listed.files?.length ?? 0} files in ${cut.piece}`}
            body="Pick one to read it."
            size="panel"
          />
        ) : (
          <>
            <header className="piece-files-head">
              <code>{picked}</code>
              <button
                type="button"
                tabIndex={0}
                onClick={() => onOpenView(pieceFileView(cut, picked), pieceFileTitle(cut, picked))}
              >
                Open in a tab of its own
              </button>
            </header>
            <Shown path={picked} read={read} />
          </>
        )}
      </section>
    </div>
  );
}
