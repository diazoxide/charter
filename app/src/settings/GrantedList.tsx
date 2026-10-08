import { useCallback, useEffect, useState } from "react";
import { commands, type PlaneId, type SandboxGrant } from "../bindings";
import { Notice } from "../Notice";
import { sandboxCommandReturned } from "../sandboxAsked";
import { DispatchGrantsList } from "./DispatchGrants";
import type { RowIds } from "./components";
import type { SettingsGroup } from "./groups";

/** The address of the Granted list (#1348): a sub-page of Sandbox. */
export const GRANTED = "project.sandbox.granted";

/** What each level is called on the list. */
const LEVELS: Readonly<Record<SandboxGrant["level"], string>> = {
  chat: "One chat",
  you: "Me on this machine",
  project: "Everyone in this project",
};

/** When, as the list says it: the day and time, or nothing where it is not known. */
function when(at: number | null): string {
  if (at === null) return "";
  return new Date(at * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** One grant, as a sentence: what it allows, for whom, who granted it and when. */
export function grantSaid(one: SandboxGrant): string {
  const allows =
    one.what === "host"
      ? `Reach ${one.target}`
      : one.what === "vault"
        ? `Use vault ${one.target} as ${one.persona ?? "a persona"}`
        : one.what === "persona-hosts"
          ? `Chats as ${one.persona ?? "a persona"}${one.for_no_persona ? " and every chat that names no persona" : ""} reach ${one.target}`
          : `Write ${one.target} and everything in it`;
  const by =
    one.level !== "project"
      ? "granted by you"
      : one.by === null
        ? "not committed yet"
        : `committed by ${one.by}`;
  const from = one.chat === null ? "" : `, from ${one.chat}`;
  const at = when(one.at);
  return `${allows} · ${LEVELS[one.level]} · ${by}${at === "" ? "" : `, ${at}`}${from}`;
}

/** What Revoke takes away, as its button says it. */
function revoked(one: SandboxGrant): string {
  if (one.what === "host") return `reaching ${one.target}`;
  if (one.what === "vault") return `using vault ${one.target} as ${one.persona ?? "a persona"}`;
  if (one.what === "persona-hosts") return `${one.persona ?? "a persona"}'s hosts`;
  return `writing ${one.target}`;
}

/** What the core said went wrong, through the Notice every surface says it with. */
function Trouble({ said, onDismiss }: { said: string; onDismiss: () => void }) {
  return (
    <Notice cause={`granted:${said}`} at="pane" tone="trouble" onDismiss={onDismiss}>
      {said}
    </Notice>
  );
}

/**
 * **Every grant, each revocable** (#1348): what a person allowed past this project's sandbox,
 * at every level — this chat, every chat here on this machine, and the project's own hosts —
 * with who granted it and when. A vault you let a persona's chats use on this machine although
 * it is not tagged for the persona is one of them (#1430). **Revoke** takes it out of every later start, through the core,
 * which audits it; a project host's revoke is a change to the committed file, which teammates
 * follow. One a policy locks out (a host it does not allow, a folder where it forbids write
 * grants) is kept and not in force: it says so, with the policy and who set it, and is drawn
 * locked.
 */
function GrantedRows({ plane, file, ids }: { plane: PlaneId; file: string; ids: RowIds }) {
  const [grants, setGrants] = useState<readonly SandboxGrant[]>();
  const [said, setSaid] = useState<string>();

  const read = useCallback(() => {
    void commands
      .sandboxGrants(plane)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else setGrants(done.data);
      })
      .catch((err: unknown) => setSaid(`purlis could not list what was granted: ${String(err)}`));
  }, [plane]);
  useEffect(read, [read]);

  const revoke = (one: SandboxGrant) => {
    setSaid(undefined);
    void commands
      .revokeSandboxGrant(plane, one.id)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else setGrants(done.data);
      })
      .catch((err: unknown) => setSaid(`purlis could not revoke it: ${String(err)}`))
      // What chats may write moved, or did not: the Notice for chats left behind asks again.
      .finally(sandboxCommandReturned);
  };

  return (
    <div id={ids.id} aria-labelledby={ids.labelledBy}>
      {grants === undefined ? (
        said === undefined && <p>Reading what was granted…</p>
      ) : grants.length === 0 ? (
        <p>Nothing is granted past this project&apos;s sandbox.</p>
      ) : (
        <ul className="granted-list" aria-label="Granted">
          {grants.map((one) => (
            <li key={one.id}>
              <span>{grantSaid(one)}</span>
              {one.level === "project" && one.locked === null && (
                <span className="granted-note">Revoking it edits the committed {file}.</span>
              )}
              {one.waiting !== null && (
                // An Allow whose list changed since (#1362, D-1362-13): kept only to be revoked or
                // allowed anew, never in force again by itself.
                <span className="granted-note"> Not in force: {one.waiting}.</span>
              )}
              {one.locked !== null ? (
                // Kept, and not in force (#1423): the core's sentence ends "Locked by policy",
                // with who set it.
                <span className="granted-locked"> Not in force. {one.locked}</span>
              ) : (
                <button
                  type="button"
                  tabIndex={0}
                  aria-label={`Revoke ${revoked(one)}`}
                  onClick={() => revoke(one)}
                >
                  Revoke
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      {said !== undefined && <Trouble said={said} onDismiss={() => setSaid(undefined)} />}
    </div>
  );
}

/**
 * **The folders chats may be granted** (D-1342-10), this machine only: what a block's Allow may
 * name besides the project. The core refuses one a denial class, `PATH` or a harness's home
 * holds, warns of one that holds what later code loads, and drops one that has come to resolve
 * elsewhere since it was listed, which is said here.
 */
function GrantableFolders({ plane, ids }: { plane: PlaneId; ids: RowIds }) {
  const [folders, setFolders] = useState<readonly string[]>();
  const [typed, setTyped] = useState("");
  const [said, setSaid] = useState<string>();
  /** What the folder last listed holds that later code loads: said once it is listed. */
  const [holds, setHolds] = useState<readonly string[]>([]);
  /** The listed folders the core dropped because they resolve elsewhere now (R8). */
  const [dropped, setDropped] = useState<readonly string[]>([]);

  useEffect(() => {
    void commands
      .grantableFolders(plane)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else {
          setFolders(done.data.folders);
          setDropped(done.data.dropped);
        }
      })
      .catch((err: unknown) => setSaid(`purlis could not list the folders: ${String(err)}`));
  }, [plane]);

  const add = () => {
    setSaid(undefined);
    setHolds([]);
    void commands
      .listGrantableFolder(plane, typed.trim())
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else {
          setFolders(done.data.folders);
          setHolds(done.data.holds);
          setTyped("");
        }
      })
      .catch((err: unknown) => setSaid(`purlis could not add the folder: ${String(err)}`))
      // What chats may write moved, or did not: the Notice for chats left behind asks again.
      .finally(sandboxCommandReturned);
  };
  const remove = (folder: string) => {
    setSaid(undefined);
    void commands
      .unlistGrantableFolder(plane, folder)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else setFolders(done.data);
      })
      .catch((err: unknown) => setSaid(`purlis could not remove the folder: ${String(err)}`))
      // What chats may write moved, or did not: the Notice for chats left behind asks again.
      .finally(sandboxCommandReturned);
  };

  return (
    <div aria-labelledby={ids.labelledBy}>
      <ul className="granted-list" aria-label="Folders chats may be granted">
        {(folders ?? []).map((folder) => (
          <li key={folder}>
            <code>{folder}</code>
            <button
              type="button"
              tabIndex={0}
              aria-label={`Remove ${folder}`}
              onClick={() => remove(folder)}
            >
              Remove
            </button>
          </li>
        ))}
      </ul>
      <input
        id={ids.id}
        value={typed}
        onChange={(event) => setTyped(event.target.value)}
        placeholder="/Users/you/tools/cache"
        aria-describedby={ids.describedBy}
      />
      <button type="button" tabIndex={0} disabled={typed.trim() === ""} onClick={add}>
        Add folder
      </button>
      {dropped.length > 0 && (
        <Notice
          cause={`grantable-dropped:${dropped.join(",")}`}
          at="pane"
          tone="trouble"
          onDismiss={() => setDropped([])}
        >
          purlis took{" "}
          {dropped.map((one, at) => (
            <span key={one}>
              {at > 0 && ", "}
              <code>{one}</code>
            </span>
          ))}{" "}
          off this list: it now leads somewhere else, through a link, so it no longer names the
          folder you listed. List it again if you still mean it.
        </Notice>
      )}
      {holds.length > 0 && (
        <Notice
          cause={`grantable-holds:${holds.join(",")}`}
          at="pane"
          tone="trouble"
          onDismiss={() => setHolds([])}
        >
          That folder holds folders other programs load code from, outside any sandbox:{" "}
          {holds.map((one, at) => (
            <span key={one}>
              {at > 0 && ", "}
              <code>{one}</code>
            </span>
          ))}
          . A chat granted a folder inside it may change what they run. Remove it unless you meant
          that.
        </Notice>
      )}
      {said !== undefined && <Trouble said={said} onDismiss={() => setSaid(undefined)} />}
    </div>
  );
}

/** The Granted list's group, for the project at `plane`, whose committed file is `file`. */
/**
 * The Granted page. Where an administrator's policy forbids write grants (#1343), `writesLocked`
 * is what it says — "Locked by policy" and who set it — and the folders list offers no control.
 */
export function grantedGroup(
  plane: PlaneId,
  file: string,
  writesLocked: string | null = null,
): SettingsGroup {
  return {
    id: GRANTED,
    label: "Granted",
    help: "What chats here may reach or write beyond the project's sandbox, and who allowed it. Revoke takes it away from each chat when it next starts.",
    sub: true,
    settings: [
      {
        id: `${GRANTED}.list`,
        label: "Granted",
        help: `One chat lasts until that chat closes. Me on this machine is kept on this machine only. Everyone in this project is kept in ${file}, which your team follows.`,
        useControl: function useGranted() {
          return { control: (ids) => <GrantedRows plane={plane} file={file} ids={ids} /> };
        },
      },
      {
        id: `${GRANTED}.folders`,
        label: "Folders chats may be granted",
        help: "Folders outside this project that a block's Allow may name, such as a tool's cache. Kept on this machine only, never committed.",
        useControl: function useGrantableFolders() {
          return {
            grouped: writesLocked !== null,
            control: (ids) =>
              writesLocked === null ? (
                <GrantableFolders plane={plane} ids={ids} />
              ) : (
                <p
                  id={ids.id}
                  className="ui-setting-status"
                  aria-labelledby={ids.labelledBy}
                  aria-describedby={ids.describedBy}
                >
                  {writesLocked}
                </p>
              ),
          };
        },
      },
      {
        id: `${GRANTED}.dispatch`,
        // Its own words: "Dispatch grants" is the row of the Dispatch page, and a label is one row's.
        label: "Who may dispatch to whom",
        help: `Which personas' chats may dispatch to which, and who allowed it. Revoke makes the next dispatch ask again. Everyone in this project is kept in ${file}, which your team follows.`,
        useControl: function useDispatchGrants() {
          return {
            grouped: true,
            control: (ids) => <DispatchGrantsList plane={plane} file={file} ids={ids} />,
          };
        },
      },
    ],
  };
}
