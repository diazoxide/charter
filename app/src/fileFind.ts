import { useEffect, useRef, useState } from "react";
import { commands, type FileScope, type FoundFile, type PlaneId } from "./bindings";
import { placeName, type Place } from "./pieceViews";

/**
 * **⌘P's files** (FM-7, #1110; #1103, V86 F10): a file found by its fuzzy name, in a scope that
 * follows the window's focus and that Tab widens.
 *
 * The ranking, and what is never offered, are `purlis_core::files::find`'s (`find_files`):
 * the window names a project and a branch, never a directory. This module decides the scope
 * ladder, how a hit is said on its row, and asks once per keystroke, the newest answer winning.
 */

/**
 * The scopes ⌘P steps through, narrowest first: **the branch picked in the explorer, the project
 * in front, every open project.** A rung the window has nothing for is left out — no branch
 * picked, no project in front, or only the one project open, where "every open project" is the
 * project rung again — so Tab never lands on a scope that adds nothing.
 *
 * **The wider rungs keep the focus as their order**: the picked branch leads the project, and
 * the project in front leads every open project, so of two equal hits the nearer comes first.
 */
export function scopeLadder(
  inFront: PlaneId | undefined,
  branch: Place | undefined,
  open: number,
): FileScope[] {
  const ladder: FileScope[] = [];
  const near = branch === undefined ? null : { ...branch };
  if (inFront !== undefined && branch !== undefined) {
    ladder.push({ kind: "branch", plane: inFront, ...branch });
  }
  if (inFront !== undefined) ladder.push({ kind: "project", plane: inFront, near });
  if (inFront === undefined || open > 1) {
    ladder.push({ kind: "open-projects", front: inFront ?? null, near });
  }
  return ladder;
}

/** What the scope is called beside the files it found. */
export function scopeSaid(scope: FileScope, nameOf: (plane: PlaneId) => string): string {
  switch (scope.kind) {
    case "branch":
      return `branch ${placeName(scope)}`;
    case "workspace":
      return `workspace ${scope.workspace}`;
    case "project":
      return `project ${nameOf(scope.plane)}`;
    case "open-projects":
      return "all open projects";
  }
}

/** A hit, as its row says it: the file's name, then where it is — its folder, its branch and
 *  its project — so two files of the same name are told apart. */
export function hitSaid(
  hit: FoundFile,
  nameOf: (plane: PlaneId) => string,
): { name: string; where: string } {
  const cut = hit.path.lastIndexOf("/");
  const name = hit.path.slice(cut + 1);
  const folder = cut < 0 ? "" : hit.path.slice(0, cut);
  const where = [folder, placeName(hit), nameOf(hit.plane)].filter((part) => part !== "");
  return { name, where: where.join(" · ") };
}

/** The branch a hit is in, as the file tab names one. */
export function placeOf(hit: FoundFile): Place {
  return { workspace: hit.workspace, repo: hit.repo, piece: hit.piece };
}

/** What the palette draws for its files group. */
export type FilesFoundNow = {
  /** The files of the newest answer, best first. */
  files: readonly FoundFile[];
  /** Each branch the scope could not list, in the core's words. */
  refused: readonly string[];
  /** Each branch listed only in part, in the core's words. */
  partial: readonly string[];
  /** Whether the answer for what is typed now has not arrived yet. */
  looking: boolean;
};

/**
 * Each palette opening is a session of its own: the core lists each branch once in it. Counted
 * from the time this window's code was loaded, so a reloaded window never reuses a session the
 * core may still hold from before the reload. Inside a `u32`, which is what crosses.
 */
let sessions = Date.now() % 2_000_000_000;

/**
 * Asks for the files of `scope` matching `query`, while `open`. A palette session starts when
 * it opens, asking with an empty query so the scope is listed before the first keystroke, and
 * ends when it closes, so the core lets go of what it listed.
 */
export function useFileFind(
  open: boolean,
  scope: FileScope | undefined,
  query: string,
): FilesFoundNow {
  const [found, setFound] = useState<{
    asked: string;
    files: FoundFile[];
    refused: string[];
    partial: string[];
  }>({ asked: "", files: [], refused: [], partial: [] });
  const session = useRef(0);
  const newest = useRef(0);

  useEffect(() => {
    if (!open) return;
    const mine = ++sessions;
    session.current = mine;
    return () => {
      void commands.findFilesEnd(mine).catch(() => undefined);
      setFound({ asked: "", files: [], refused: [], partial: [] });
    };
  }, [open]);

  const key = scope === undefined ? "" : JSON.stringify(scope);
  const asked = `${key}\n${query.trim()}`;
  useEffect(() => {
    if (!open || scope === undefined) return;
    const ask = ++newest.current;
    void commands
      .findFiles(session.current, scope, query.trim())
      .then((said) => {
        if (ask !== newest.current) return;
        setFound(
          said.status === "error"
            ? { asked, files: [], refused: [said.error], partial: [] }
            : {
                asked,
                files: said.data.files,
                refused: said.data.refused,
                partial: said.data.partial,
              },
        );
      })
      .catch((err: unknown) => {
        if (ask === newest.current) {
          setFound({ asked, files: [], refused: [String(err)], partial: [] });
        }
      });
    // `key` stands for `scope`: a new object with the same scope is not a new question.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, key, query, asked]);

  const current = found.asked === asked;
  return {
    files: query.trim() === "" ? [] : found.files,
    refused: current ? found.refused : [],
    partial: current ? found.partial : [],
    looking: query.trim() !== "" && !current,
  };
}
