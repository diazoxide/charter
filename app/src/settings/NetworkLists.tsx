import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import {
  commands,
  type BlockedLately,
  type GrantLevel,
  type PlaneId,
  type SandboxNetwork,
} from "../bindings";
import { Notice } from "../Notice";
import { sandboxCommandReturned, useSandboxCommands } from "../sandboxAsked";
import type { RowIds } from "./components";

/**
 * **Settings › Sandbox › Network** (#1662, spec #1661): the hosts every sandboxed chat here
 * reaches without asking (**Open hosts**), and what chats here were refused lately (**Blocked
 * lately**), each with Allow. Both are the core's, read from the project's files and this
 * machine's network record; nothing here lists a host of its own. The **Allowed** list beside
 * them is `GrantedList.tsx`'s.
 */

/** How many Allows on a refusals list have returned: the Allowed list beside it reads again
 *  when it moves. A count and not an event, as `sandboxAsked.ts`'s is. */
let allowedHere = 0;
const allowListeners = new Set<() => void>();

function allowedOnAList(): void {
  allowedHere += 1;
  for (const listener of [...allowListeners]) listener();
}

/** How many Allows on a refusals list have returned. */
export function useAllowedHere(): number {
  return useSyncExternalStore(
    (listener) => {
      allowListeners.add(listener);
      return () => void allowListeners.delete(listener);
    },
    () => allowedHere,
    () => allowedHere,
  );
}

/** When, as the lists say it: the day and time. */
export function when(at: number): string {
  return new Date(at * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** What the core said went wrong, through the Notice every surface says it with. */
function Trouble({ said, onDismiss }: { said: string; onDismiss: () => void }) {
  return (
    <Notice cause={`network:${said}`} at="pane" tone="trouble" onDismiss={onDismiss}>
      {said}
    </Notice>
  );
}

/** The page's reading of the core: asked again whenever a sandbox command of the window's
 *  returns, so an Allow here or a Remove beside it shows at once. */
function useNetwork(plane: PlaneId): {
  network: SandboxNetwork | undefined;
  said: string | undefined;
  setSaid: (said: string | undefined) => void;
} {
  const [network, setNetwork] = useState<SandboxNetwork>();
  const [said, setSaid] = useState<string>();
  const moved = useSandboxCommands();
  const asking = useRef(0);
  const read = useCallback(() => {
    const mine = ++asking.current;
    void commands
      .sandboxNetwork(plane)
      .then((done) => {
        if (mine !== asking.current) return;
        if (done.status === "error") setSaid(done.error);
        else setNetwork(done.data);
      })
      .catch(
        (err: unknown) =>
          mine === asking.current &&
          setSaid(`purlis could not read this project's network: ${String(err)}`),
      );
  }, [plane]);
  useEffect(read, [read, moved]);
  return { network, said, setSaid };
}

/** **Open hosts**: each preset in force, host by host, then the project's own. */
export function OpenHostsList({ plane, ids }: { plane: PlaneId; ids: RowIds }) {
  const { network, said, setSaid } = useNetwork(plane);
  return (
    <div id={ids.id} aria-labelledby={ids.labelledBy}>
      {network === undefined ? (
        said === undefined && <p>Reading this project&apos;s network…</p>
      ) : !network.on ? (
        <p>Chats here run without a sandbox, so they can reach any host.</p>
      ) : network.open.length === 0 ? (
        <p>No host is open: a chat here reaches only what you allow.</p>
      ) : (
        network.open.map((group) => (
          <div key={group.title} className="sandbox-hosts">
            <span className="sandbox-what">{group.title}</span>
            <ul aria-label={group.title}>
              {group.hosts.map((host) => (
                <li key={host}>
                  <code>{host}</code>
                </li>
              ))}
            </ul>
          </div>
        ))
      )}
      {said !== undefined && <Trouble said={said} onDismiss={() => setSaid(undefined)} />}
    </div>
  );
}

/** What each scope's Allow says on its button. */
const ALLOW: Readonly<Record<GrantLevel, string>> = {
  chat: "Allow for this chat",
  you: "Allow",
  project: "Allow for everyone in the project",
};

/** What a row of a refused lookup says beside its host (#1663): a program's own printed words
 *  named it, so it is shown as text and never offered to allow. */
export const LOOKED_UP = "Looked up by a program, not offered.";

/** One row of a refusals list, as a sentence: what, from which chat, when and how often. */
export function blockedSaid(row: BlockedLately): string {
  const from = row.chat === null ? "" : ` · ${row.chat}`;
  const times = row.times > 1 ? ` · ${row.times} times, last` : "";
  return `${capital(row.said)}${from}${times} · ${when(row.at)}`;
}

/** `said` starting with a capital, as a list row does. */
function capital(said: string): string {
  return said.length > 0 ? said[0].toUpperCase() + said.slice(1) : said;
}

/**
 * The rows of a refusals list (Blocked lately, or a chat's own), each with Allow where the core
 * says one may be kept: **Allow** keeps the host for this project on this machine, and **Allow
 * for everyone in the project** commits it. A row a chat reaches now says so. A refused
 * lookup's host is shown and never has an Allow: a program's printed words named it (#1663).
 */
export function RefusedRows({
  plane,
  rows,
  label,
}: {
  plane: PlaneId;
  rows: readonly BlockedLately[];
  label: string;
}) {
  const [said, setSaid] = useState<string>();
  const [trouble, setTrouble] = useState<string>();
  const allow = (host: string, level: GrantLevel) => {
    setSaid(undefined);
    setTrouble(undefined);
    void commands
      .allowBlockedHost(plane, host, level)
      .then((done) => {
        if (done.status === "error") setTrouble(done.error);
        else {
          setSaid(done.data.said);
          allowedOnAList();
        }
      })
      .catch((err: unknown) => setTrouble(`purlis could not allow ${host}: ${String(err)}`))
      // What chats here reach moved, or did not: whatever reads it asks again.
      .finally(sandboxCommandReturned);
  };
  return (
    <>
      <ul className="granted-list" aria-label={label}>
        {rows.map((row) => (
          <li key={`${row.target ?? ""}\u0000${row.looked_up ?? ""}\u0000${row.said}`}>
            {row.target !== null && <code>{row.target}</code>}
            {row.looked_up !== null && <code>{row.looked_up}</code>}
            <span>{blockedSaid(row)}</span>
            {row.reached && <span className="granted-note"> Reached now.</span>}
            {row.looked_up !== null && <span className="granted-note"> {LOOKED_UP}</span>}
            {row.target !== null &&
              row.looked_up === null &&
              row.levels.map((level) => (
                <button
                  key={level}
                  type="button"
                  tabIndex={0}
                  aria-label={`${ALLOW[level]}: ${row.target}`}
                  onClick={() => row.target !== null && allow(row.target, level)}
                >
                  {ALLOW[level]}
                </button>
              ))}
          </li>
        ))}
      </ul>
      {said !== undefined && (
        <Notice cause={`network-allowed:${said}`} at="pane" onDismiss={() => setSaid(undefined)}>
          {said}
        </Notice>
      )}
      {trouble !== undefined && <Trouble said={trouble} onDismiss={() => setTrouble(undefined)} />}
    </>
  );
}

/** **Blocked lately**: what chats here were refused in the last 30 days, newest first. */
export function BlockedLatelyList({ plane, ids }: { plane: PlaneId; ids: RowIds }) {
  const { network, said, setSaid } = useNetwork(plane);
  return (
    <div id={ids.id} aria-labelledby={ids.labelledBy}>
      {network === undefined ? (
        said === undefined && <p>Reading what was blocked…</p>
      ) : network.blocked.length === 0 ? (
        <p>Nothing was blocked here in the last 30 days.</p>
      ) : (
        <RefusedRows plane={plane} rows={network.blocked} label="Blocked lately" />
      )}
      {said !== undefined && <Trouble said={said} onDismiss={() => setSaid(undefined)} />}
    </div>
  );
}
