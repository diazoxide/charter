import { useCallback, useEffect, useState } from "react";
import { commands, type ForgeRow, type Recents, type TemplateChoice } from "./bindings";
import type { ForgeAsk } from "./ForgeQuestion";
import { FirstRun } from "./FirstRun";
import { Notice } from "./Notice";
import { GoneProjectNotice } from "./GoneProjectNotice";
import { useNewerTrouble } from "./pickTrouble";

/**
 * The screen a window with no project open draws.
 *
 * **It is not the error page, and the difference is the whole point.** charter used to answer
 * a launch outside a plane with `No plane: …` and nothing to do about it, which is the screen
 * the operator met when he double-clicked the app — an error about a concept he did not have
 * yet, on a window that offered no way to get one. ADR 0033 is the decision that a project IS
 * a plane and that the app opens one; this is the door, and `plane_at_launch`'s three shapes
 * are what decide which sentence it opens with.
 *
 * - **Nothing to go on** (`here` false) — an app started from the dock, whose working
 *   directory is `/`. "You have not opened a project yet." A newcomer's first screen, and it
 *   reports nothing, because nothing went wrong.
 * - **A directory that is in no project** (`here` true) — `charter` run somewhere ordinary.
 *   charter says where it looked and what it looked for, because that operator asked a
 *   question and deserves the answer.
 *
 * Neither is an alert. What both offer is the same: a folder to pick, a path to type, and the
 * projects this machine remembers.
 *
 * **A machine that remembers no project gets the first run instead** (FR-4, #603): a launch
 * with nothing to go on, on a machine whose recent list is empty, is somebody who has never
 * had a project, and "a project is a directory with a `charter.toml` in it" is a concept they
 * should not need before their first chat. `FirstRun` asks for a repository and nothing else,
 * and this screen stays one press behind it.
 */
export function Opener({
  here,
  reason,
  adding,
  onOpen,
  onGoneListed,
  onGoneSettled,
  goneChanged,
  trouble,
  onOpenRepo,
  onSignInToForge,
  openingRepo,
  repoTrouble,
  repoForgeAsk,
}: {
  /** Whether the launch had a directory to go on at all. */
  here: boolean;
  /** Why no project was opened, in the resolver's own words. */
  reason: string;
  /**
   * Whether this window already holds projects, so this is a ninth rather than a first.
   *
   * Then neither sentence above is the right one: nothing went wrong and nothing is missing —
   * the operator pressed `+` on a window full of projects. Saying "you have not opened a
   * project yet" to somebody looking at eight of them is the app not knowing where it is.
   */
  adding?: boolean;
  /** Asks the core to open this path. It answers, or asks the operator first. */
  onOpen: (path: string) => void;
  /** The gone projects this opener draws, told whenever they change (NO-5): while the opener
   *  is drawn, the window draws only the ones this list does not hold. `"unread"` until the
   *  list has answered and again when the opener goes, so the window holds its own back
   *  rather than drawing a line that the opener's would replace under the pointer. */
  onGoneListed?: (paths: readonly string[] | "unread") => void;
  /** A gone project was located, forgotten or dismissed here. */
  onGoneSettled?: (path: string) => void;
  /** Changes when the window located or forgot a gone project, so the list is read again. */
  goneChanged?: number;
  /** Why the last attempt opened nothing. */
  trouble?: string;
  /** Opens a repository into the local project, for the first run. Left out, the first run is
   *  never drawn: the window passes it only until it has held a project. */
  onOpenRepo?: (path: string, template: TemplateChoice) => void;
  /** Opens the local project with `<cli> auth login` in a shell tab, for the first run. */
  onSignInToForge?: (row: ForgeRow) => void;
  /** Whether that is happening right now. */
  openingRepo?: boolean;
  /** Why the last repository opened nothing. */
  repoTrouble?: string;
  /** The first run's question about the forge, when the repo's remote did not say (#839). */
  repoForgeAsk?: ForgeAsk;
}) {
  const [recents, setRecents] = useState<Recents>();
  /** The Notices dismissed here, by cause, for this run (NO-2 keeps them across a relaunch). */
  const [dismissed, setDismissed] = useState<ReadonlySet<string>>(() => new Set());
  /** Whether the recent list has answered at all, so the first run is never drawn over a
   *  machine whose list simply has not arrived yet, and never over one that could not say. */
  const [heard, setHeard] = useState(false);
  /** The operator asked for this screen rather than the first run. */
  const [passed, setPassed] = useState(false);
  const [typed, setTyped] = useState("");
  /** Bumped when a gone recent was forgotten or re-pointed, so the list is read again. */
  const [changed, setChanged] = useState(0);

  // The list is read when the opener appears and re-read whenever an attempt did not open
  // anything, because a refused open is exactly when a row may have gone. It is a command of
  // its own and it stats each row off the thread that draws, so nothing here waits on a
  // network mount that is not coming back.
  useEffect(() => {
    let gone = false;
    void commands
      .recentPlanes()
      .then((answer) => {
        if (!gone && answer.status === "ok") setRecents(answer.data ?? undefined);
        if (!gone) setHeard(true);
      })
      // A window that cannot ask simply offers no list. The picker and the path box still
      // work, which is the whole of what this screen has to do.
      .catch(() => {
        if (!gone) setHeard(true);
      });
    return () => {
      gone = true;
    };
  }, [trouble, changed, goneChanged]);

  // The gone projects drawn here, said to the window, which then leaves them to this list.
  const listed = heard ? (recents?.gone ?? []).map((gone) => gone.path).join("\n") : undefined;
  useEffect(() => {
    onGoneListed?.(listed === undefined ? "unread" : listed === "" ? [] : listed.split("\n"));
    return () => onGoneListed?.("unread");
  }, [listed, onGoneListed]);

  // Why the folder dialog could not open (#1291): said where a refused open is, until any open
  // starts or a newer refusal comes. A cancelled dialog is null and is not a failure: nothing is
  // said and nothing moves.
  const { said, pickFailed, started } = useNewerTrouble(trouble);
  const open = useCallback(
    (path: string) => {
      started();
      onOpen(path);
    },
    [onOpen, started],
  );
  const pick = useCallback(() => {
    started();
    void commands
      .pickProject()
      .catch((err: unknown) => ({ status: "error" as const, error: String(err) }))
      .then((answer) => {
        if (answer.status === "error") pickFailed(answer.error);
        else if (answer.data) open(answer.data);
      });
  }, [open, pickFailed, started]);

  // The first run is for a launch with nothing to go on, on a machine that remembers nothing.
  // Until the list has answered, a launch like that draws nothing rather than the wrong screen.
  const firstRun = onOpenRepo !== undefined && !adding && !here && !passed;
  if (firstRun && !heard) return null;
  // "Remembers nothing" is all of it: no row, no row dropped for having moved, and a store to
  // remember in — a machine that keeps none cannot hold the local project either.
  const remembersNothing =
    recents !== undefined &&
    (recents.planes?.length ?? 0) === 0 &&
    (recents.dropped?.length ?? 0) === 0 &&
    (recents.gone?.length ?? 0) === 0 &&
    !recents.forgetful;
  if (firstRun && remembersNothing)
    return (
      <FirstRun
        onOpenRepo={onOpenRepo}
        onOpenProject={() => setPassed(true)}
        onSignInToForge={onSignInToForge ?? (() => undefined)}
        opening={openingRepo ?? false}
        trouble={repoTrouble}
        forgeAsk={repoForgeAsk}
      />
    );

  return (
    <section className="opener" aria-labelledby="opener-heading">
      {adding ? (
        <>
          <h1 id="opener-heading">Open another project</h1>
          <p className="came-back" role="status">
            It opens as another tab in this window, beside the ones already here. Every project
            keeps its own chats — nothing running in them stops.
          </p>
        </>
      ) : here ? (
        <>
          <h1 id="opener-heading">purlis found no project here</h1>
          {/* The resolver's own words. An operator who ran `charter` in a directory asked a
              question, and "a project is the nearest directory at or above this one with a
              charter.toml" is the answer to it. */}
          <p className="came-back" role="status">
            {reason}
          </p>
        </>
      ) : (
        <>
          <h1 id="opener-heading">You have not opened a project yet</h1>
          <p className="came-back" role="status">
            A project is a directory with a <code>charter.toml</code> in it. Open one, and charter
            opens its workspaces, its chats and its personas with it.
          </p>
        </>
      )}

      {/* Every button here says `tabIndex={0}`: WebKit leaves a `<button>` out of the tab
          sequence unless it is written down (`docs/ui-primitives.md`, charter-app#189). */}
      <div className="doing">
        <button type="button" tabIndex={0} onClick={pick}>
          Open project…
        </button>
      </div>

      {/* The path box is not a lesser picker. A native folder dialog cannot be driven by the
          scenario tests, and an operator who already knows the path types faster than they
          click — so the two are one command with two ways in, and neither resolves anything
          itself. */}
      <form
        className="by-path"
        onSubmit={(event) => {
          event.preventDefault();
          if (typed.trim()) open(typed.trim());
        }}
      >
        <label htmlFor="open-by-path">Or type a path</label>
        <input
          id="open-by-path"
          type="text"
          value={typed}
          placeholder="/path/to/project"
          onChange={(event) => setTyped(event.target.value)}
        />
        <button type="submit" tabIndex={0} disabled={!typed.trim()}>
          Open
        </button>
      </form>

      {said && (
        <p className="trouble" role="alert">
          {said}
        </p>
      )}

      {/* `?.` on the list as well as on the answer: a core that answered oddly — the shape
          changed, a command stubbed out — must cost this screen its recent list and not its
          picker. The opener is the one screen an operator with no project can reach. */}
      {(recents?.planes?.length ?? 0) > 0 && (
        <>
          <h2>Recent projects</h2>
          <ul className="recents">
            {recents?.planes?.map((plane) => (
              <li key={plane.path}>
                <button type="button" tabIndex={0} onClick={() => open(plane.path)}>
                  <span className="tab-name">{plane.name}</span>
                  <code className="where">{plane.path}</code>
                </button>
                {/* What the store holds, and nothing more: whether this project still
                    contributes what was approved is asked when it is opened. */}
                {!plane.approved && <span className="value">purlis will ask about this one</span>}
              </li>
            ))}
          </ul>
        </>
      )}

      {/* An entry the store itself would not take back is dropped with a line saying so, never
          an error dialog (ADR 0034): the record is a convenience and the project is the truth. */}
      {recents?.dropped
        ?.filter((line) => !dismissed.has(`not-remembered:${line}`))
        .map((line) => (
          <Notice
            key={line}
            cause={`not-remembered:${line}`}
            onDismiss={() => setDismissed((was) => new Set(was).add(`not-remembered:${line}`))}
          >
            {line}
          </Notice>
        ))}

      {/* A project that has moved or gone offers Locate… and Forget (NO-5): never done
          without a press, because a disk that is unplugged may come back. */}
      {recents?.gone
        ?.filter((gone) => !dismissed.has(`project-gone:${gone.path}`))
        .map((gone) => (
          <GoneProjectNotice
            key={gone.path}
            gone={gone}
            cause={`project-gone:${gone.path}`}
            onLocated={(found) => {
              onGoneSettled?.(gone.path);
              setChanged((n) => n + 1);
              open(found);
            }}
            onForgotten={() => {
              onGoneSettled?.(gone.path);
              setChanged((n) => n + 1);
            }}
            onDismiss={() => {
              onGoneSettled?.(gone.path);
              setDismissed((was) => new Set(was).add(`project-gone:${gone.path}`));
            }}
          />
        ))}

      {/* A machine with no store — Windows, where `0600` has no expression, so charter's guard
          refuses rather than degrades (ADR 0031). The app works; it just cannot remember. */}
      {recents?.forgetful && !dismissed.has("no-machine-store") && (
        <Notice
          cause="no-machine-store"
          onDismiss={() => setDismissed((was) => new Set(was).add("no-machine-store"))}
        >
          purlis cannot remember projects on this machine ({recents.forgetful}), so there is no
          recent list and it will ask about every project you open.
        </Notice>
      )}
    </section>
  );
}
