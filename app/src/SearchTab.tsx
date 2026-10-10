/**
 * **The left side's Search view** (FM-8, #1111; #1103, V86 F9/F10; #1676): ⌘⇧F's content
 * search. ⌘⇧F shows the view, which draws this component in the side, keyed by what it asks
 * (`PlaneView`'s `sideSearch`). It was a view tab until #1676; since #1701 the side is the one
 * place a search is drawn, and no pane draws one (D-1701-2). The name is the tab's, kept.
 *
 * A query box with its three switches — match case, whole word, regular expression — and where
 * to look: the branch the tab was opened on, its workspace, this project, or every project open
 * in charter. Hits stream in grouped project → branch → file, each file with its matching lines
 * and how many there are; a large result stops at a page and offers "Show more".
 *
 * **From the keyboard**: ↓ from the box goes to the hits, ↑/↓ step through them (Home and End
 * jump), and Enter opens the one stepped to in its branch's file tab, at its line. Escape in the
 * hits goes back to the box.
 *
 * **Shift+Enter hands the hit to a chat** (FM-9, #1151): it opens the preview's chat picker for
 * the hit stepped to, with its line, under the summary — a key on the listbox, never a button
 * inside an option. The pick is typed into that chat as *Add to a chat's context* types it,
 * unsent (`handReference`), and what the core answered, or why nothing was handed, is said in the
 * tab's status line. Escape closes the picker, and the keyboard goes back to the hits either way.
 *
 * What is searched, and what never is, is the core's (`contentSearch.ts`). This draws.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CaseSensitive, LoaderCircle, Regex, Search, WholeWord } from "lucide-react";
import type { PlaneId, SearchedFile } from "./bindings";
import { EmptyState } from "./EmptyState";
import { FileIcon } from "./FileIcon";
import { useFileIcons } from "./projectTheme";
import { iconFor } from "./theme/icons";
import {
  projectCalled,
  scopeCalled,
  scopesOf,
  searchOf,
  searchTitle,
  searchView,
  useContentSearch,
  type Matching,
  type ScopeKind,
  type SearchAsk,
  type SearchHeard,
} from "./contentSearch";
import { jumpTo } from "./fileJump";
import {
  ChatsToPick,
  dragReference,
  handReference,
  referenceSaid,
  useReferenceChats,
  type ChatHere,
  type Referenced,
} from "./references";
import { placeName } from "./pieceViews";
import type { ViewRef } from "./tabs";

/** How long the query is still before it is asked. */
const SETTLE = 250;

export function SearchTab({
  plane,
  view,
  onAsk,
  takesKeyboard = true,
}: {
  plane: PlaneId;
  view: ViewRef;
  /** The tab now asks something else: its view follows, so it is keyed by what it shows. */
  onAsk?: (from: ViewRef, to: ViewRef, title: string) => void;
  /** Whether the box takes the keyboard when it is drawn, as a tab just opened should. The
   *  Search view (#1676) is drawn when it is first shown and may be drawn at launch, so the
   *  window gives it the keyboard when a person asks for it, and never on its own. */
  takesKeyboard?: boolean;
}) {
  const fromView = useMemo(() => searchOf(view), [view]);
  const [ask, setAsk] = useState<SearchAsk | undefined>(fromView);
  const [draft, setDraft] = useState(fromView?.query ?? "");
  // **The view this tab last made itself, and the last one it was drawn with.** A view it did
  // not make — another Search tab drawn in this pane — starts it over from that view.
  const [made, setMade] = useState(view.key);
  const [seen, setSeen] = useState(view.key);
  if (view.key !== seen) {
    setSeen(view.key);
    if (view.key !== made) {
      setMade(view.key);
      setAsk(fromView);
      setDraft(fromView?.query ?? "");
    }
  }

  /** Asks `next`: searched now, and the tab's view follows it, so the tab is keyed by what it
   *  shows. */
  const commit = useCallback(
    (next: SearchAsk) => {
      setAsk(next);
      const to = searchView(next);
      if (to.key === made) return;
      setMade(to.key);
      onAsk?.(view, to, searchTitle(next));
    },
    [made, onAsk, view],
  );

  // The query, asked once it is still.
  useEffect(() => {
    if (ask === undefined || draft === ask.query) return;
    const timer = setTimeout(() => commit({ ...ask, query: draft }), SETTLE);
    return () => clearTimeout(timer);
  }, [ask, commit, draft]);

  if (ask === undefined) {
    return (
      <EmptyState
        headline="This search could not be read"
        body="Show Search again to start a new one."
        size="panel"
      />
    );
  }
  return (
    <SearchBody
      plane={plane}
      takesKeyboard={takesKeyboard}
      ask={ask}
      draft={draft}
      onDraft={setDraft}
      onAsk={(next) => commit({ ...next, query: draft })}
    />
  );
}

function SearchBody({
  plane,
  takesKeyboard,
  ask,
  draft,
  onDraft,
  onAsk,
}: {
  plane: PlaneId;
  takesKeyboard: boolean;
  ask: SearchAsk;
  draft: string;
  onDraft: (draft: string) => void;
  onAsk: (ask: SearchAsk) => void;
}) {
  const { heard, more } = useContentSearch(plane, ask, 0);
  const box = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (takesKeyboard) box.current?.focus();
  }, [takesKeyboard]);

  const toggle = (which: keyof Matching) =>
    onAsk({ ...ask, matching: { ...ask.matching, [which]: !ask.matching[which] } });

  return (
    <div className="search-tab">
      <header className="search-head">
        <Search className="node-icon" aria-hidden="true" />
        <input
          ref={box}
          type="search"
          role="searchbox"
          className="search-query"
          aria-label="Search the files"
          placeholder="Search the files"
          spellCheck={false}
          value={draft}
          onChange={(e) => onDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown" || e.key === "Enter") {
              e.preventDefault();
              list.current?.focus();
            }
          }}
        />
        <Switch on={ask.matching.matchCase} label="Match case" onPress={() => toggle("matchCase")}>
          <CaseSensitive aria-hidden="true" />
        </Switch>
        <Switch on={ask.matching.wholeWord} label="Whole word" onPress={() => toggle("wholeWord")}>
          <WholeWord aria-hidden="true" />
        </Switch>
        <Switch on={ask.matching.regex} label="Regular expression" onPress={() => toggle("regex")}>
          <Regex aria-hidden="true" />
        </Switch>
        <select
          className="search-scope"
          role="combobox"
          // #190: WebKit leaves a control out of the tab sequence without `tabIndex`.
          tabIndex={0}
          aria-label="Where to search"
          value={ask.kind}
          onChange={(e) => onAsk({ ...ask, kind: e.target.value as ScopeKind })}
        >
          {scopesOf(ask).map((kind) => (
            <option key={kind} value={kind}>
              {scopeCalled(kind, ask)}
            </option>
          ))}
        </select>
      </header>
      <Hits
        heard={heard}
        asked={ask.query.trim() !== ""}
        query={ask.query}
        list={list}
        onBack={() => box.current?.focus()}
        onMore={more}
      />
    </div>
  );
}

/** One of the query's switches, pressed or not. */
function Switch({
  on,
  label,
  onPress,
  children,
}: {
  on: boolean;
  label: string;
  onPress: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      className="search-switch"
      tabIndex={0}
      aria-pressed={on}
      aria-label={label}
      title={label}
      onClick={onPress}
    >
      {children}
    </button>
  );
}

/** One file's lines, and the headings it starts: its project's and its branch's, when they are
 *  new. */
type Shown = {
  file: SearchedFile;
  project?: string;
  branch?: string;
  /** Where its first line is among every line shown. */
  first: number;
};

/** The files as drawn: grouped project → branch → file, in the order they were found. */
function grouped(files: readonly SearchedFile[]): { shown: Shown[]; lines: number } {
  // Every project's files together, then every branch's, each in the order first found: a page
  // that comes back to a project already shown adds to it rather than heading it again.
  const projects = new Map<string, Map<string, SearchedFile[]>>();
  for (const file of files) {
    const branch = `${file.workspace}/${file.repo}/${file.piece ?? ""}`;
    let of = projects.get(file.plane);
    if (of === undefined) {
      of = new Map();
      projects.set(file.plane, of);
    }
    let held = of.get(branch);
    if (held === undefined) {
      held = [];
      of.set(branch, held);
    }
    held.push(file);
  }
  const shown: Shown[] = [];
  let lines = 0;
  for (const [plane, branches] of projects) {
    let firstOfProject = true;
    for (const branchFiles of branches.values()) {
      let firstOfBranch = true;
      for (const file of branchFiles) {
        shown.push({
          file,
          project: firstOfProject ? projectCalled(plane) : undefined,
          branch: firstOfBranch ? `${placeName(file)} · ${file.workspace}` : undefined,
          first: lines,
        });
        firstOfProject = false;
        firstOfBranch = false;
        lines += file.lines.length;
      }
    }
  }
  return { shown, lines };
}

/** What the status line says of a search. */
function summary(heard: SearchHeard, files: number, lines: number): string {
  const found =
    files === 0
      ? "No matches"
      : `${lines} matching line${lines === 1 ? "" : "s"} in ${files} file${files === 1 ? "" : "s"}`;
  if (heard.state === "searching") return files === 0 ? "Searching…" : `${found} so far…`;
  if (heard.state === "more") {
    return heard.short === "out-of-time"
      ? `${found}; the search paused after its time`
      : `${found}; there may be more`;
  }
  return found;
}

/** The hits, stepped through from the keyboard. */
function Hits({
  heard,
  asked,
  list,
  onBack,
  onMore,
  query,
}: {
  heard: SearchHeard;
  asked: boolean;
  /** What was asked, said back when nothing matched it. */
  query: string;
  list: React.RefObject<HTMLDivElement | null>;
  onBack: () => void;
  onMore: () => void;
}) {
  const { shown, lines } = useMemo(() => grouped(heard.files), [heard.files]);
  const [active, setActive] = useState(0);
  const at = Math.min(active, Math.max(0, lines - 1));
  const counted = heard.files.reduce((sum, file) => sum + file.count, 0);
  const lent = useReferenceChats();
  /** The hit whose chat is being picked (Shift+Enter). */
  const [picking, setPicking] = useState<Referenced>();
  /** What the last hand to a chat answered, said in the status line. */
  const [handed, setHanded] = useState<{ words: string; refused: boolean }>();
  const picker = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (picking !== undefined)
      picker.current?.querySelector<HTMLElement>("[role=menuitem]")?.focus();
  }, [picking]);

  // The line stepped to stays in view.
  useEffect(() => {
    list.current?.querySelector(`[data-hit="${at}"]`)?.scrollIntoView?.({ block: "nearest" });
  }, [at, list]);

  if (!asked) {
    return (
      <EmptyState
        mark={Search}
        headline="Search the content of the files"
        body="What git ignores, links, vaults, files named like a credential and binary files are never searched."
        size="panel"
        testid="search-empty"
      />
    );
  }
  if (heard.trouble !== undefined) {
    return <EmptyState headline={heard.trouble} size="panel" testid="search-trouble" />;
  }
  // **A finished search that found nothing says so, and what to try** (FR-19, #614), where the
  // hits would be. The status line above still counts it, and still names a branch it could
  // not search, which is the other reason nothing came back.
  const nothing = heard.state === "done" && heard.files.length === 0;

  /** The hit `n` lines into what is shown: its file and its line's number. */
  const hitAt = (n: number) => {
    const found = shown.find((one) => n >= one.first && n < one.first + one.file.lines.length);
    if (found === undefined) return undefined;
    return { file: found.file, line: found.file.lines[n - found.first].number };
  };

  const open = (n: number) => {
    const hit = hitAt(n);
    if (hit === undefined) return;
    const { file, line } = hit;
    jumpTo({
      plane: file.plane,
      place: { workspace: file.workspace, repo: file.repo, piece: file.piece },
      path: file.path,
      line,
    });
  };

  /** Shift+Enter: the chat picker for hit `n`, or the sentence that says why there is none. */
  const pick = (n: number) => {
    const hit = hitAt(n);
    if (hit === undefined) return;
    const { file, line } = hit;
    const r: Referenced = {
      plane: file.plane,
      workspace: file.workspace,
      repo: file.repo,
      piece: file.piece,
      path: file.path,
      folder: false,
      lines: { first: line, last: line },
    };
    const said = referenceSaid(r);
    if (lent === undefined || lent.chats.length === 0)
      setHanded({ words: `No chat is open in this project to add ${said} to.`, refused: true });
    else if (r.plane !== lent.plane)
      setHanded({
        words: `${said} is in another project, so this project's chats cannot take it.`,
        refused: true,
      });
    else {
      setHanded(undefined);
      setPicking(r);
    }
  };

  /** The picker is done: closed, and the keyboard back on the hits. */
  const picked = () => {
    setPicking(undefined);
    list.current?.focus();
  };

  const handTo = (r: Referenced, chat: ChatHere) => {
    if (lent === undefined) return;
    picked();
    void handReference(lent.plane, chat.session, chat.name, r).then((ran) =>
      setHanded(
        ran.ok ? { words: ran.said ?? "", refused: false } : { words: ran.refused, refused: true },
      ),
    );
  };

  const pickerKeys = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      picked();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = [...(picker.current?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];
    const now = items.indexOf(document.activeElement as HTMLElement);
    const step = e.key === "ArrowDown" ? 1 : -1;
    items[(now + step + items.length) % items.length]?.focus();
  };

  const keys = (e: React.KeyboardEvent) => {
    const to = (n: number) => {
      e.preventDefault();
      setActive(Math.max(0, Math.min(lines - 1, n)));
    };
    if (e.key === "ArrowDown") to(at + 1);
    else if (e.key === "ArrowUp") {
      if (at === 0) {
        e.preventDefault();
        onBack();
      } else to(at - 1);
    } else if (e.key === "Home") to(0);
    else if (e.key === "End") to(lines - 1);
    else if (e.key === "Enter" && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      pick(at);
    } else if (e.key === "Enter") {
      e.preventDefault();
      open(at);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onBack();
    }
  };

  return (
    <>
      <p className="search-summary" role="status" aria-label="Search progress">
        {heard.state === "searching" && (
          <LoaderCircle className="node-icon spinning" aria-hidden="true" />
        )}
        {summary(heard, heard.files.length, counted)}
        {heard.refused.map((why, n) => (
          <span key={n} className="search-refused">
            {why}
          </span>
        ))}
        {heard.unsearched.length > 0 && (
          <span
            className="search-unsearched"
            title={heard.unsearched.join("\n")}
            data-testid="search-unsearched"
          >
            {`${heard.unsearched.length} file${heard.unsearched.length === 1 ? "" : "s"} not searched: ${heard.unsearched.join("; ")}`}
          </span>
        )}
        {handed !== undefined && (
          <span className={handed.refused ? "search-refused" : "search-handed"}>
            {handed.words}
          </span>
        )}
      </p>
      {picking !== undefined && lent !== undefined && (
        <div className="search-pick" ref={picker} onKeyDown={pickerKeys}>
          <ChatsToPick
            chats={lent.chats}
            label={`Add ${referenceSaid(picking)} to a chat's context`}
            onPick={(chat) => handTo(picking, chat)}
          />
        </div>
      )}
      {nothing ? (
        <EmptyState
          mark={Search}
          headline={`Nothing matches “${query.trim()}”`}
          body="Search a wider scope, or loosen the match: turn off match case, whole word or the regular expression."
          size="panel"
          testid="search-none"
        />
      ) : (
        <div
          ref={list}
          className="search-hits"
          role="listbox"
          aria-label="Search results"
          tabIndex={0}
          aria-activedescendant={lines > 0 ? `search-hit-${at}` : undefined}
          aria-keyshortcuts="Shift+Enter"
          onKeyDown={keys}
        >
          {shown.map(({ file, project, branch, first }) => {
            const cut = file.path.lastIndexOf("/");
            const name = file.path.slice(cut + 1);
            const folder = cut < 0 ? "" : file.path.slice(0, cut);
            const where = [folder, placeName(file), projectCalled(file.plane)]
              .filter((part) => part !== "")
              .join(" · ");
            return (
              <div
                key={`${file.plane}\u0000${file.workspace}/${file.repo}/${file.piece ?? ""}\u0000${file.path}`}
                role="group"
                aria-label={`${name}, ${where}`}
                className="search-file"
              >
                {project !== undefined && (
                  <div className="search-project" aria-hidden="true">
                    {project}
                  </div>
                )}
                {branch !== undefined && (
                  <div className="search-branch" aria-hidden="true">
                    {branch}
                  </div>
                )}
                <div className="search-file-head" aria-hidden="true">
                  <HitIcon file={file} name={name} />
                  <span className="search-file-name">{name}</span>
                  {folder !== "" && <span className="search-file-folder">{folder}</span>}
                  <span className="search-count">{file.count}</span>
                </div>
                {file.lines.map((line, i) => {
                  const n = first + i;
                  return (
                    <div
                      key={line.number}
                      id={`search-hit-${n}`}
                      data-hit={n}
                      role="option"
                      aria-selected={n === at}
                      className={n === at ? "search-line active" : "search-line"}
                      // A hit dragged onto a chat carries its line (FM-9).
                      draggable
                      onDragStart={(event) =>
                        dragReference(event, {
                          plane: file.plane,
                          workspace: file.workspace,
                          repo: file.repo,
                          piece: file.piece,
                          path: file.path,
                          folder: false,
                          lines: { first: line.number, last: line.number },
                        })
                      }
                      onClick={() => {
                        setActive(n);
                        open(n);
                      }}
                    >
                      <span className="search-line-number">{line.number}</span>
                      <span className="search-line-text">
                        {line.clipped && "…"}
                        {line.parts.map((part, j) =>
                          part.hit ? (
                            <mark key={j}>{part.text}</mark>
                          ) : (
                            <span key={j}>{part.text}</span>
                          ),
                        )}
                      </span>
                    </div>
                  );
                })}
                {file.count > file.lines.length && (
                  <div className="search-file-rest" aria-hidden="true">
                    {`${file.count - file.lines.length} more in this file`}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
      {heard.state === "more" && (
        <button type="button" className="search-more" tabIndex={0} onClick={onMore}>
          Show more
        </button>
      )}
    </>
  );
}

/**
 * **A hit's file, drawn as the file trees draw it** (#1145; #1103 story 27): through `FileIcon`,
 * with the icon theme its own project picks in its own workspace — a search over every open
 * project draws each project's hits in that project's icons.
 */
function HitIcon({ file, name }: { file: SearchedFile; name: string }) {
  const icons = useFileIcons(file.plane, file.workspace);
  return <FileIcon symbol={iconFor(icons, { name, folder: false })} />;
}
