import { useCallback, useEffect, useState } from "react";
import {
  commands,
  type DispatchGrant,
  type DispatchGrants,
  type DispatchNever,
  type DispatchStanding,
  type PlaneId,
} from "../bindings";
import { Notice } from "../Notice";
import type { RowIds } from "./components";

/** What each level is called on the list. */
const LEVELS: Readonly<Record<DispatchGrant["level"], string>> = {
  chat: "One chat",
  you: "Me on this machine",
  project: "Everyone in this project",
};

/** No grants and no locks: what a core that answers nothing is read as. */
const NONE: DispatchGrants = {
  grants: [],
  all_locked: null,
  locked_pairs: [],
  locked_by: null,
  changed: null,
};

/** Nothing said never, and nothing granted for any persona. */
const NOTHING_STANDS: DispatchStanding = { nevers: [], any: [], nevers_unread: null };

/** When, as the list says it: the day and time, or nothing where it is not known. */
function when(at: number | null): string {
  if (at === null) return "";
  return new Date(at * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** One dispatch grant, as a sentence: who may dispatch to whom, for whom, who granted it and
 *  when. */
export function dispatchGrantSaid(one: DispatchGrant): string {
  const allows =
    one.asking === null
      ? `This chat may dispatch to ${one.target}`
      : `${one.asking} chats may dispatch to ${one.target}`;
  const by =
    one.level !== "project"
      ? "granted by you"
      : one.by === null
        ? "not committed yet"
        : `committed by ${one.by}`;
  const from = one.chat === null ? "" : `, from ${one.chat}`;
  const at = when(one.at);
  // The project's, and nobody at this machine has allowed it yet: it covers nothing here.
  const waiting = one.waiting ? " · not allowed on this machine yet" : "";
  return `${allows} · ${LEVELS[one.level]} · ${by}${at === "" ? "" : `, ${at}`}${from}${waiting}`;
}

/**
 * **Every dispatch grant, each revocable** (#1437): which personas' chats may dispatch to which,
 * at every level — one chat, me on this machine, everyone in this project — with who granted it
 * and when. **Revoke** takes it out, through the core, which audits it: the next dispatch across
 * the pair asks again, and a persona chat already running is left as it is. A project grant's
 * revoke is a change to the committed file, which teammates follow.
 *
 * What an administrator's policy locks is drawn locked, with who set it: a lock on all dispatch,
 * each locked pair, and any grant a lock now holds, which offers no Revoke.
 *
 * **The pairs you said never to are listed under the grants, each with Lift** (#1503): a never
 * is said on a dispatch's Notice, and this is where it is taken back. Lifting it goes through
 * the core, which audits it; what then covers the pair is whatever grant stands, and with none
 * the next dispatch asks. Where the list of nevers does not read, the core's sentence saying so
 * is drawn in its place. The table of every grant with where it comes from replaces this row.
 *
 * A component a Settings page mounts: Settings › Project › Dispatch, and today the Granted page.
 */
export function DispatchGrantsList({
  plane,
  file,
  ids,
  locks = true,
}: {
  plane: PlaneId;
  /** The project's committed file, by name. */
  file: string;
  ids: RowIds;
  /** Whether what policy locks is listed under the grants: off where a page has a row of its
   *  own for it ({@link useDispatchLocks}). */
  locks?: boolean;
}) {
  const [held, setHeld] = useState<DispatchGrants>();
  const [said, setSaid] = useState<string>();
  const [standing, setStanding] = useState<DispatchStanding>(NOTHING_STANDS);

  const read = useCallback(() => {
    void commands
      .dispatchGrants(plane)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else setHeld(done.data ?? NONE);
      })
      .catch((err: unknown) =>
        setSaid(`purlis could not list the dispatch grants: ${String(err)}`),
      );
    void commands
      .dispatchStanding(plane)
      .then((done) => {
        if (done.status === "ok") setStanding(done.data ?? NOTHING_STANDS);
      })
      // A list that cannot be read here lists nothing; the core still holds every never.
      .catch(() => {});
  }, [plane]);
  useEffect(read, [read]);

  const lift = (one: DispatchNever) => {
    setSaid(undefined);
    void commands
      .liftDispatchNever(plane, one.asking, one.target)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else setStanding(done.data ?? NOTHING_STANDS);
      })
      .catch((err: unknown) => setSaid(`purlis could not lift it: ${String(err)}`));
  };

  const revoke = (one: DispatchGrant) => {
    setSaid(undefined);
    void commands
      .revokeDispatchGrant(plane, one.id)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else setHeld(done.data ?? NONE);
      })
      .catch((err: unknown) => setSaid(`purlis could not revoke it: ${String(err)}`));
  };

  return (
    <div id={ids.id} aria-labelledby={ids.labelledBy}>
      {held === undefined ? (
        said === undefined && <p>Reading the dispatch grants…</p>
      ) : (
        <>
          {locks && held.all_locked !== null && <p className="granted-locked">{held.all_locked}</p>}
          {held.grants.length === 0 ? (
            <p>No persona&apos;s chats may dispatch to another persona yet.</p>
          ) : (
            <ul className="granted-list" aria-label="Dispatch grants">
              {held.grants.map((one) => (
                <li key={one.id}>
                  <span>{dispatchGrantSaid(one)}</span>
                  {one.level === "project" && one.locked === null && (
                    <span className="granted-note">Revoking it edits the committed {file}.</span>
                  )}
                  {one.locked !== null ? (
                    <span className="granted-locked">{one.locked}</span>
                  ) : (
                    <button
                      type="button"
                      tabIndex={0}
                      aria-label={`Revoke ${one.asking ?? "this chat"} dispatching to ${one.target}`}
                      onClick={() => revoke(one)}
                    >
                      Revoke
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
          {standing.nevers_unread !== null && (
            <p className="granted-locked">{standing.nevers_unread}</p>
          )}
          {standing.nevers.length > 0 && (
            <ul className="granted-list" aria-label="Never">
              {standing.nevers.map((one) => (
                <li key={`${one.asking}\u001f${one.target}`}>
                  <span>
                    {one.asking} chats never dispatch to {one.target} · Me on this machine · said by
                    you
                  </span>
                  <span className="granted-note">
                    No grant covers it, and no {one.asking} chat is asked, until you lift it.
                  </span>
                  <button
                    type="button"
                    tabIndex={0}
                    aria-label={`Lift never for ${one.asking} dispatching to ${one.target}`}
                    onClick={() => lift(one)}
                  >
                    Lift
                  </button>
                </li>
              ))}
            </ul>
          )}
          {locks && held.locked_pairs.length > 0 && (
            <>
              <ul className="granted-list" aria-label="Locked by policy">
                {held.locked_pairs.map((pair) => (
                  <li key={`${pair.asking}\u001f${pair.target}`}>
                    {pair.asking} to {pair.target}
                  </li>
                ))}
              </ul>
              {held.locked_by !== null && <p className="granted-note">{held.locked_by}</p>}
            </>
          )}
        </>
      )}
      {said !== undefined && (
        <Notice
          cause={`dispatch-grants:${said}`}
          at="pane"
          tone="trouble"
          onDismiss={() => setSaid(undefined)}
        >
          {said}
        </Notice>
      )}
    </div>
  );
}

/**
 * **What an administrator's policy locks of who may dispatch to whom**, each as one sentence
 * with who set it: a lock on all dispatch, then each locked pair. `undefined` until it is read;
 * none where no policy locks any. For a page that lists policy locks in a row of their own.
 */
export function useDispatchLocks(plane: PlaneId): readonly string[] | undefined {
  const [lines, setLines] = useState<readonly string[]>();
  useEffect(() => {
    let live = true;
    void commands
      .dispatchGrants(plane)
      .then((done) => {
        if (!live) return;
        const held = done.status === "ok" ? (done.data ?? NONE) : NONE;
        setLines([
          ...(held.all_locked === null ? [] : [held.all_locked]),
          ...held.locked_pairs.map(
            (pair) =>
              `${pair.asking} chats may not dispatch to ${pair.target}. ${held.locked_by ?? ""}`,
          ),
        ]);
      })
      // A policy that cannot be read here lists nothing; the core still enforces it.
      .catch(() => {
        if (live) setLines([]);
      });
    return () => {
      live = false;
    };
  }, [plane]);
  return lines;
}
