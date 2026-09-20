import { useCallback, useEffect, useState } from "react";
import { commands, type Recents } from "./bindings";

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
 */
export function Opener({
  here,
  reason,
  onOpen,
  trouble,
}: {
  /** Whether the launch had a directory to go on at all. */
  here: boolean;
  /** Why no project was opened, in the resolver's own words. */
  reason: string;
  /** Asks the core to open this path. It answers, or asks the operator first. */
  onOpen: (path: string) => void;
  /** Why the last attempt opened nothing. */
  trouble?: string;
}) {
  const [recents, setRecents] = useState<Recents>();
  const [typed, setTyped] = useState("");

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
      })
      // A window that cannot ask simply offers no list. The picker and the path box still
      // work, which is the whole of what this screen has to do.
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [trouble]);

  const pick = useCallback(() => {
    void commands
      .pickProject()
      .then((answer) => {
        // A cancelled dialog is null and is not a failure: nothing is said and nothing moves.
        if (answer.status === "ok" && answer.data) onOpen(answer.data);
      })
      .catch(() => undefined);
  }, [onOpen]);

  return (
    <section className="opener" aria-labelledby="opener-heading">
      {here ? (
        <>
          <h1 id="opener-heading">charter found no project here</h1>
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

      <div className="doing">
        <button type="button" onClick={pick}>
          Open Project…
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
          if (typed.trim()) onOpen(typed.trim());
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
        <button type="submit" disabled={!typed.trim()}>
          Open
        </button>
      </form>

      {trouble && (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      )}

      {recents && recents.planes.length > 0 && (
        <>
          <h2>Recent projects</h2>
          <ul className="recents">
            {recents.planes.map((plane) => (
              <li key={plane.path}>
                <button type="button" onClick={() => onOpen(plane.path)}>
                  <span className="tab-name">{plane.name}</span>
                  <code className="where">{plane.path}</code>
                </button>
                {/* What the store holds, and nothing more: whether this project still
                    contributes what was approved is asked when it is opened. */}
                {!plane.approved && <span className="value">charter will ask about this one</span>}
              </li>
            ))}
          </ul>
        </>
      )}

      {/* A project that has moved or gone is dropped with a line saying so, never an error
          dialog (ADR 0034): the record is a convenience and the project is the truth. */}
      {recents?.dropped.map((line) => (
        <p className="came-back" role="status" key={line}>
          {line}
        </p>
      ))}

      {/* A machine with no store — Windows, where `0600` has no expression, so charter's guard
          refuses rather than degrades (ADR 0031). The app works; it just cannot remember. */}
      {recents?.forgetful && (
        <p className="came-back" role="status">
          charter cannot remember projects on this machine ({recents.forgetful}), so there is no
          recent list and it will ask about every project you open.
        </p>
      )}
    </section>
  );
}
