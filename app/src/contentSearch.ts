import { useEffect, useRef, useState } from "react";
import {
  commands,
  type FileScope,
  type FilesSearched,
  type PlaneId,
  type SearchedFile,
} from "./bindings";
import { listen } from "./here";
import type { Place } from "./pieceViews";
import type { ViewRef } from "./tabs";

/**
 * **⌘⇧F: the Search view tab's model** (FM-8, #1111; #1103, V86 F9/F10). What it searches, how a
 * tab is keyed, and the run it asks the core for — `purlis_core::files::search`, through
 * `search_files`. The window names a project and branches, never a directory.
 *
 * A Search tab is **keyed by its scope and its query**, so the same search asked twice brings the
 * tab already showing it forward. The key is names only — a scope's kind, the branch it was
 * opened on (a workspace, a repo and a piece, none of which can hold a `|` or a `/`), the match
 * options, then the query, last, so it may hold anything. Search tabs are not kept for the next
 * launch: what was typed is the operator's, and is never written to the record.
 */

/** How wide a search looks. */
export type ScopeKind = "branch" | "workspace" | "project" | "open";

/** How the query matches. */
export type Matching = { regex: boolean; matchCase: boolean; wholeWord: boolean };

/** One Search tab's question. */
export type SearchAsk = {
  kind: ScopeKind;
  /** The branch the tab was opened on, if any: the branch scope's, and the one wider scopes put
   *  first. */
  branch?: Place;
  /** The workspace the tab is on, if any: the workspace scope's. */
  workspace?: string;
  matching: Matching;
  query: string;
};

export const SEARCH = "search";
const PLAIN: Matching = { regex: false, matchCase: false, wholeWord: false };

/** The view a Search tab shows. */
export function searchView(ask: SearchAsk): ViewRef {
  const branch = ask.branch
    ? `${ask.branch.workspace}/${ask.branch.repo}/${ask.branch.piece ?? ""}`
    : "";
  const flags =
    (ask.matching.regex ? "r" : "") +
    (ask.matching.matchCase ? "c" : "") +
    (ask.matching.wholeWord ? "w" : "");
  return {
    from: null,
    view: SEARCH,
    key: [ask.kind, ask.workspace ?? "", branch, flags, ask.query].join("|"),
  };
}

/** Whether a view is a Search tab. */
export function isSearch(view: ViewRef): boolean {
  return view.from === null && view.view === SEARCH;
}

/** The question a Search tab's view asks; `undefined` for any other view, or a key not made by
 *  {@link searchView}. */
export function searchOf(view: ViewRef): SearchAsk | undefined {
  if (!isSearch(view)) return undefined;
  const parts = view.key.split("|");
  if (parts.length < 5) return undefined;
  const [kind, workspace, branch, flags] = parts;
  const query = parts.slice(4).join("|");
  if (!["branch", "workspace", "project", "open"].includes(kind)) return undefined;
  let place: Place | undefined;
  if (branch !== "") {
    const [ws, repo, piece, ...rest] = branch.split("/");
    if (!ws || !repo || piece === undefined || rest.length > 0) return undefined;
    place = { workspace: ws, repo, piece: piece === "" ? null : piece };
  }
  return {
    kind: kind as ScopeKind,
    branch: place,
    workspace: workspace === "" ? undefined : workspace,
    matching: {
      regex: flags.includes("r"),
      matchCase: flags.includes("c"),
      wholeWord: flags.includes("w"),
    },
    query,
  };
}

/** What a Search tab is called. */
export function searchTitle(ask: SearchAsk): string {
  const query = ask.query.trim();
  return query === ""
    ? "Search"
    : `Search · ${query.length > 40 ? `${query.slice(0, 40)}…` : query}`;
}

/**
 * The Search tab ⌘⇧F opens: as narrow as the window's focus — the branch the explorer picked,
 * else the workspace in front, else the whole project — with an empty query.
 */
export function searchFromFocus(
  branch: Place | undefined,
  workspace: string | undefined,
): SearchAsk {
  return {
    kind: branch !== undefined ? "branch" : workspace !== undefined ? "workspace" : "project",
    branch,
    workspace: branch?.workspace ?? workspace,
    matching: PLAIN,
    query: "",
  };
}

/** The scopes the tab offers, narrowest first: a branch or a workspace only when it has one. */
export function scopesOf(ask: SearchAsk): ScopeKind[] {
  const kinds: ScopeKind[] = [];
  if (ask.branch !== undefined) kinds.push("branch");
  if (ask.workspace !== undefined) kinds.push("workspace");
  kinds.push("project", "open");
  return kinds;
}

/** What a scope is called in the tab's picker. */
export function scopeCalled(kind: ScopeKind, ask: SearchAsk): string {
  switch (kind) {
    case "branch":
      return `Branch ${ask.branch?.piece ?? ask.branch?.repo ?? ""}`;
    case "workspace":
      return `Workspace ${ask.workspace ?? ""}`;
    case "project":
      return "This project";
    case "open":
      return "All open projects";
  }
}

/** The scope the core is asked for: the tab's project, and the branch it was opened on first. */
export function scopeFor(plane: PlaneId, ask: SearchAsk): FileScope {
  const near = ask.branch ? { ...ask.branch } : null;
  switch (ask.kind) {
    case "branch":
      return ask.branch
        ? { kind: "branch", plane, ...ask.branch }
        : { kind: "project", plane, near: null };
    case "workspace":
      return ask.workspace !== undefined
        ? { kind: "workspace", plane, workspace: ask.workspace, near }
        : { kind: "project", plane, near };
    case "project":
      return { kind: "project", plane, near };
    case "open":
      return { kind: "open-projects", front: plane, near };
  }
}

/** What a run has heard so far. */
export type SearchHeard = {
  /** Files with matches, in the order the scope was walked. */
  files: SearchedFile[];
  /** Branches the scope could not search, in the core's words. */
  refused: string[];
  /** Files not searched, and why, in the core's words. */
  unsearched: string[];
  /** Why the query was not searched at all, in the core's words. */
  trouble?: string;
  /** How many branches the scope covers, once the core has said. */
  branches?: number;
  /** "searching" while a page runs; "more" when a page stopped short of the end; "done". */
  state: "idle" | "searching" | "more" | "done";
  /** Why the last page stopped short, when it did. */
  short?: "capped" | "out-of-time";
};

const IDLE: SearchHeard = { files: [], refused: [], unsearched: [], state: "idle" };

/** A Search tab's id, unique in this window for as long as the page is loaded: what the core
 *  keeps its run under, and what stops it. */
let tabs = Date.now() % 1_000_000_000;

/**
 * Runs `ask` over `plane`'s scope, and hears its hits as they stream in. A new question stops
 * the old run as it is asked (the core stops a tab's run when the tab asks again, by its id),
 * and a hit of an older run arriving late is dropped. Leaving stops the run.
 *
 * **Typed, then asked**: the query is asked once it has been still for `settle` milliseconds,
 * so typing never queues work.
 */
export function useContentSearch(
  plane: PlaneId,
  ask: SearchAsk,
  settle = 250,
): { heard: SearchHeard; more: () => void } {
  const [tab] = useState(() => ++tabs);
  const id = useRef(tab);
  const run = useRef(0);
  const [heard, setHeard] = useState<{ run: number; heard: SearchHeard }>({
    run: 0,
    heard: IDLE,
  });

  // The core's batches, for this tab's current run only.
  useEffect(() => {
    let gone = false;
    let stop: (() => void) | undefined;
    void listen<FilesSearched>("files-searched", (event) => {
      const batch = event.payload;
      if (batch.id !== id.current || batch.run !== run.current) return;
      setHeard((was) => {
        if (was.run !== batch.run) return was;
        const ended = batch.ended;
        return {
          run: was.run,
          heard: {
            ...was.heard,
            files: batch.files.length ? [...was.heard.files, ...batch.files] : was.heard.files,
            refused: batch.refused.length
              ? [...was.heard.refused, ...batch.refused]
              : was.heard.refused,
            unsearched: batch.unsearched.length
              ? [...was.heard.unsearched, ...batch.unsearched]
              : was.heard.unsearched,
            state:
              ended === null
                ? was.heard.state
                : ended === "capped" || ended === "out-of-time"
                  ? "more"
                  : ended === "done"
                    ? "done"
                    : was.heard.state,
            short: ended === "capped" || ended === "out-of-time" ? ended : was.heard.short,
          },
        };
      });
    }).then((unlisten) => {
      if (gone) unlisten();
      else stop = unlisten;
    });
    return () => {
      gone = true;
      stop?.();
    };
  }, []);

  // Leaving the tab stops its run.
  useEffect(() => {
    const mine = id.current;
    return () => {
      void commands.searchFilesEnd(mine).catch(() => undefined);
    };
  }, []);

  const scope = JSON.stringify(scopeFor(plane, ask));
  const matching = JSON.stringify(ask.matching);
  const query = ask.query;
  useEffect(() => {
    const mine = ++run.current;
    if (query.trim() === "") {
      // Nothing to ask: the last run stops. A new question needs no `end` first — asking it
      // stops the tab's last run in the core, so the two can never arrive out of order.
      void commands.searchFilesEnd(id.current).catch(() => undefined);
      setHeard({ run: mine, heard: IDLE });
      return;
    }
    setHeard({ run: mine, heard: { ...IDLE, state: "searching" } });
    const timer = setTimeout(() => {
      void commands
        .searchFiles(
          id.current,
          mine,
          JSON.parse(scope) as FileScope,
          query,
          JSON.parse(matching) as Matching,
        )
        .then((said) => {
          setHeard((was) => {
            if (was.run !== mine) return was;
            return said.status === "error"
              ? { run: mine, heard: { ...IDLE, state: "done", trouble: said.error } }
              : { run: mine, heard: { ...was.heard, branches: said.data } };
          });
        })
        .catch((err: unknown) =>
          setHeard((was) =>
            was.run === mine
              ? { run: mine, heard: { ...IDLE, state: "done", trouble: String(err) } }
              : was,
          ),
        );
    }, settle);
    return () => clearTimeout(timer);
  }, [scope, matching, query, settle]);

  const more = () => {
    const mine = run.current;
    setHeard((was) =>
      was.run === mine ? { run: mine, heard: { ...was.heard, state: "searching" } } : was,
    );
    void commands
      .searchFilesMore(id.current, mine)
      .then((said) => {
        if (said.status === "error") {
          setHeard((was) =>
            was.run === mine
              ? { run: mine, heard: { ...was.heard, state: "done", trouble: said.error } }
              : was,
          );
        }
      })
      .catch(() => undefined);
  };

  return { heard: heard.heard, more };
}

/** A project's name as the tab shows it: its folder's. */
export function projectCalled(plane: PlaneId): string {
  const parts = plane.split(/[\\/]/).filter((part) => part !== "");
  return parts[parts.length - 1] ?? plane;
}
