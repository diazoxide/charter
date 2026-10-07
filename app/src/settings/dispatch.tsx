import { useCallback, useEffect, useState, type ReactNode } from "react";
import {
  commands,
  type DispatchLimit,
  type DispatchLimits,
  type DispatchLimitRow,
  type PlaneId,
  type SettingsEdit,
  type SettingsStep,
  type SettingsValue,
  type SettingsWhich,
} from "../bindings";
import { Notice } from "../Notice";
import type { RowIds } from "./components";
import { DispatchGrantsList, useDispatchLocks } from "./DispatchGrants";
import type { LiveSetting, SettingsGroup } from "./groups";

/**
 * **Settings › Project › Dispatch** (#1439, #1440; spec #1434): how many persona chats a chat
 * may have running, how many a lineage may hold, how deep a chain may go and how many messages a
 * minute may pass, with a row for the project and one per workspace or persona that differs.
 *
 * **Every rule is the core's** (`purlis_core::dispatchlimits`): which level is in force, the
 * ceiling on depth, what a 0 does and what a policy caps. The window draws the rows the core
 * lists ({@link DispatchLimits}) and writes one key of a settings file through
 * `save_project_settings`, which refuses what the next read would refuse and says why.
 *
 * **All of it is kept in the project's committed file**, a persona's limits included, never the
 * persona's own file. Your own row is this machine's file, and only ever lowers a limit.
 *
 * The same table, held to one row, is a workspace's settings' ({@link workspaceDispatchGroup})
 * and the persona view's ({@link DispatchLimitsTable} with a `scope`).
 */

/** The page's address. */
export const DISPATCH = "project.dispatch";

/** The table that holds the limits, and the keys under it that hold each level's own. */
const TABLE = "dispatch";
const UNDER = { workspace: "workspaces", persona: "personas" } as const;

/** The one workspace or persona a table is held to. */
export type DispatchScope = { kind: "workspace" | "persona"; name: string };

/** One row as it is drawn: which file it is in, and whose limits they are. */
type Drawn = {
  which: SettingsWhich;
  scope: "project" | "workspace" | "persona";
  name: string;
  row: DispatchLimitRow | undefined;
  /** Whether the file holds this row: one that it does not is there to fill in. */
  written: boolean;
};

/** What a row is called in the table. */
function heading(one: Drawn, held: boolean): string {
  if (held) return one.scope === "workspace" ? "This workspace" : "This persona";
  const whose =
    one.scope === "project"
      ? "Project"
      : one.scope === "workspace"
        ? `Workspace ${one.name}`
        : `Persona ${one.name}`;
  if (one.which === "shared") return whose;
  return one.scope === "project" ? "Me on this machine" : `Me on this machine, ${lower(whose)}`;
}

/** What a row's boxes and buttons are named for: "the workspace alpha". */
function named(one: Drawn, held: boolean): string {
  if (held) return one.scope === "workspace" ? "this workspace" : "this persona";
  const whose = one.scope === "project" ? "the project" : `the ${one.scope} ${one.name}`;
  if (one.which === "shared") return whose;
  return one.scope === "project" ? "me on this machine" : `me on this machine, ${whose}`;
}

function lower(text: string): string {
  return text.length > 0 ? text[0].toLowerCase() + text.slice(1) : text;
}

/** The key a row's limit `word` is at; without `word`, the row's own table. */
function pathOf(one: Pick<Drawn, "scope" | "name">, word?: string): SettingsStep[] {
  const steps = one.scope === "project" ? [TABLE] : [TABLE, UNDER[one.scope], one.name];
  return [...steps, ...(word === undefined ? [] : [word])].map((key) => ({ key }));
}

/** A row's identity among the rows drawn. */
const idOf = (one: Pick<Drawn, "which" | "scope" | "name">) =>
  `${one.which}\u0000${one.scope}\u0000${one.name}`;

/**
 * What is in force beneath a row the file does not hold yet: the project's own value, else what
 * is beneath that. A persona's two own limits have no cap until one is written.
 */
function beneathUnwritten(page: DispatchLimits): (number | null)[] {
  const project = page.rows.find((one) => one.scope === "project");
  return page.limits.map((limit, at) =>
    limit.persona_only ? null : (project?.values[at] ?? project?.beneath[at] ?? limit.default),
  );
}

/** One limit's box: empty where the row does not set it, showing what is in force beneath. */
function Cell({
  limit,
  value,
  beneath,
  label,
  onWrite,
}: {
  limit: DispatchLimit;
  value: number | null;
  beneath: number | null;
  label: string;
  onWrite: (typed: string) => void;
}) {
  const onDisk = value === null ? "" : String(value);
  const [typed, setTyped] = useState(onDisk);
  // What is on disk is what is shown, whenever it is read again and differs.
  const [seen, setSeen] = useState(onDisk);
  if (seen !== onDisk) {
    setSeen(onDisk);
    setTyped(onDisk);
  }
  const write = () => {
    const now = typed.trim();
    if (now === onDisk) return;
    onWrite(now);
  };
  return (
    <input
      className="dispatch-limit"
      inputMode="numeric"
      size={5}
      value={typed}
      aria-label={label}
      title={limit.help}
      placeholder={beneath === null ? "no cap" : String(beneath)}
      onChange={(event) => setTyped(event.target.value)}
      onBlur={write}
      onKeyDown={(event) => {
        if (event.key === "Enter") write();
      }}
    />
  );
}

/** The core's page, read once and again after every write. */
function useDispatchLimits(plane: PlaneId) {
  const [page, setPage] = useState<DispatchLimits>();
  const [read, setRead] = useState(false);
  const [said, setSaid] = useState<readonly string[]>([]);
  /** Bumped when a write was refused, so each box goes back to what is on disk. */
  const [refusals, setRefusals] = useState(0);

  const reread = useCallback(
    () =>
      commands
        .dispatchLimits(plane)
        .then((done) => {
          if (done.status === "error") setSaid([done.error]);
          // A whole-window test answers a command it does not care about with nothing.
          else setPage(done.data ?? undefined);
        })
        .catch((err: unknown) =>
          setSaid([`purlis could not read the dispatch limits: ${String(err)}`]),
        )
        .finally(() => setRead(true)),
    [plane],
  );
  useEffect(() => void reread(), [reread]);

  /** What puts the last change back: the one Undo on offer, until the next change or its use. */
  const [back, setBack] = useState<{ which: SettingsWhich; edits: SettingsEdit[] }>();

  /**
   * Makes `edits` to `which` through the core. `undoes` is what would put them back, kept as the
   * one Undo once the core has written them; none where there is nothing to put back.
   */
  const write = useCallback(
    (which: SettingsWhich, edits: SettingsEdit[], undoes: SettingsEdit[]) => {
      if (page === undefined) return;
      setSaid([]);
      void commands
        .saveProjectSettings(plane, which, which === "shared" ? page.base : page.local_base, {
          kind: "edits",
          edits,
        })
        .then((done) => {
          if (done.status === "error") setSaid([done.error]);
          else if (done.data.kind === "refused") setSaid(done.data.reasons);
          if (done.status === "error" || done.data.kind === "refused")
            setRefusals((was) => was + 1);
          else setBack(undoes.length > 0 ? { which, edits: undoes } : undefined);
          return reread();
        })
        .catch((err: unknown) => setSaid([`purlis could not save the limit: ${String(err)}`]));
    },
    [plane, page, reread],
  );
  /** Puts the last change back, through the same write: it leaves no Undo of its own. */
  const undo = back === undefined ? undefined : () => write(back.which, back.edits, []);

  return { page, read, said, refusals, write, undo, dismiss: () => setSaid([]) };
}

/** What a workspace or a persona an override may be for is listed as. */
type Offer = { kind: "workspace" | "persona"; name: string };

/** The project's workspaces and personas, as the core lists them. */
function useOffers(plane: PlaneId, wanted: boolean): Offer[] {
  const [offers, setOffers] = useState<Offer[]>([]);
  useEffect(() => {
    if (!wanted) return;
    void Promise.all([
      commands.planeSidebar(plane).catch(() => undefined),
      commands.startOptions(plane).catch(() => undefined),
    ]).then(([sidebar, options]) => {
      const workspaces = sidebar?.status === "ok" ? (sidebar.data?.workspaces ?? []) : [];
      const personas = options?.status === "ok" ? (options.data?.personas ?? []) : [];
      setOffers([
        ...workspaces.map((one) => ({ kind: "workspace" as const, name: one.name })),
        ...personas.map((name) => ({ kind: "persona" as const, name })),
      ]);
    });
  }, [plane, wanted]);
  return offers;
}

/**
 * **The limits table.** Without a `scope` it is the page's: the project's row, a row per
 * override, yours on this machine, and Add an override. With one it is that workspace's or
 * persona's row alone, there to fill in whether or not the file holds it yet.
 *
 * A box is written when it is left or on Enter. An emptied box takes the key out, so the level
 * beneath is in force. What is typed is sent as it is: the core says why it takes none of it.
 */
function useDispatchTable(
  plane: PlaneId,
  scope?: DispatchScope,
): { control: (ids?: RowIds) => ReactNode; undo?: () => void } {
  const { page, read, said, refusals, write, undo, dismiss } = useDispatchLimits(plane);
  /** Overrides added here and not written yet: a row to fill in. */
  const [drafts, setDrafts] = useState<readonly Offer[]>([]);
  const [picked, setPicked] = useState("");
  const held = scope !== undefined;
  const offers = useOffers(plane, !held);

  const control = (ids?: RowIds) => {
    const trouble = said.length > 0 && (
      <Notice
        cause={`dispatch-limits:${said.join("|")}`}
        at="pane"
        tone="trouble"
        onDismiss={dismiss}
      >
        {said.map((one) => (
          <span key={one}>{one} </span>
        ))}
      </Notice>
    );
    if (page === undefined)
      return (
        <div id={ids?.id} aria-labelledby={ids?.labelledBy}>
          {!read && <p>Reading the dispatch limits…</p>}
          {trouble}
        </div>
      );

    const committed: Drawn[] = page.rows.map((row) => ({
      which: "shared",
      scope: row.scope as Drawn["scope"],
      name: row.name,
      row,
      written: true,
    }));
    const has = (one: Offer) =>
      committed.some((row) => row.scope === one.kind && row.name === one.name);
    const drafted: Drawn[] = drafts
      .filter((one) => !has(one))
      .map((one) => ({
        which: "shared",
        scope: one.kind,
        name: one.name,
        row: undefined,
        written: false,
      }));
    // Yours, on this machine: read and drawn even while git would carry the file, since a limit
    // of yours can only lower one.
    const mine: Drawn[] = page.mine.map((row) => ({
      which: "local",
      scope: row.scope as Drawn["scope"],
      name: row.name,
      row,
      written: true,
    }));
    const own = held
      ? (committed.find((row) => row.scope === scope.kind && row.name === scope.name) ?? {
          which: "shared" as const,
          scope: scope.kind,
          name: scope.name,
          row: undefined,
          written: false,
        })
      : undefined;
    const rows = own !== undefined ? [own] : [...committed, ...drafted, ...mine];
    // A persona's two own limits are drawn where a persona's row may be.
    const columns = page.limits
      .map((limit, at) => ({ limit, at }))
      .filter(({ limit }) => !limit.persona_only || !held || scope.kind === "persona");
    const unwritten = beneathUnwritten(page);
    const open = offers.filter(
      (one) => !has(one) && !drafted.some((row) => row.scope === one.kind && row.name === one.name),
    );
    const offerId = (one: Offer) => `${one.kind}\u0000${one.name}`;
    const chosen = open.find((one) => offerId(one) === picked) ?? open[0];
    const ignored = mine.flatMap((one) => one.row?.ignored ?? []);

    /** What `value` is as a key of a settings file; none where the row does not set it. */
    const whole = (value: number | null | undefined): SettingsValue | null =>
      value === null || value === undefined ? null : { kind: "integer", value };
    const remove = (one: Drawn) => {
      if (one.written)
        write(
          one.which,
          [{ path: pathOf(one), value: null }],
          // Undo writes back each limit the row set: its table comes back with them.
          page.limits.flatMap((limit, at) => {
            const value = whole(one.row?.values[at]);
            return value === null ? [] : [{ path: pathOf(one, limit.word), value }];
          }),
        );
      setDrafts((was) => was.filter((it) => !(it.kind === one.scope && it.name === one.name)));
    };

    return (
      <div id={ids?.id} className="dispatch-limits" aria-labelledby={ids?.labelledBy}>
        <table aria-label="Dispatch limits" aria-describedby={ids?.describedBy}>
          <thead>
            <tr>
              <th scope="col">Level</th>
              {columns.map(({ limit }) => (
                <th key={limit.word} scope="col" title={limit.help}>
                  {limit.label}
                </th>
              ))}
              <th scope="col" aria-label="" />
            </tr>
          </thead>
          <tbody>
            {rows.map((one) => {
              const whose = named(one, held);
              const removable = one.scope !== "project" && (one.row !== undefined || !held);
              return (
                <tr key={idOf(one)}>
                  <th scope="row">{heading(one, held)}</th>
                  {columns.map(({ limit, at }) => (
                    <td key={limit.word}>
                      {limit.persona_only && one.scope !== "persona" ? null : (
                        <Cell
                          // A refused write draws the box again from what is on disk.
                          key={refusals}
                          limit={limit}
                          value={one.row?.values[at] ?? null}
                          beneath={one.row !== undefined ? one.row.beneath[at] : unwritten[at]}
                          label={`${limit.label} for ${whose}`}
                          onWrite={(typed) =>
                            write(
                              one.which,
                              [
                                {
                                  path: pathOf(one, limit.word),
                                  value:
                                    typed === ""
                                      ? null
                                      : /^\d+$/.test(typed)
                                        ? { kind: "integer", value: Number(typed) }
                                        : { kind: "text", value: typed },
                                },
                              ],
                              // Undo puts back what the box held: its number, or nothing.
                              [
                                {
                                  path: pathOf(one, limit.word),
                                  value: whole(one.row?.values[at]),
                                },
                              ],
                            )
                          }
                        />
                      )}
                    </td>
                  ))}
                  <td>
                    {removable && (
                      <button
                        type="button"
                        tabIndex={0}
                        aria-label={`Remove the override for ${whose}`}
                        onClick={() => remove(one)}
                      >
                        Remove override
                      </button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <p className="granted-note">
          {held
            ? scope.kind === "persona"
              ? `Kept in ${page.file}, which your team sees, and never in the persona's own file. An empty box takes the value shown in it, from the workspace or the project.`
              : `Kept in ${page.file}, which your team sees. An empty box takes the project's value, shown in it.`
            : `Kept in ${page.file}, which your team sees. The most specific row wins: a persona's over a workspace's over the project's. An empty box takes the value shown in it. A 0 switches dispatch off for that row, and a more specific row may set it back; 0 messages per minute stops messages only.`}
        </p>
        {!held && (
          <p className="granted-note">
            Me on this machine is kept in {page.local_file}, on this machine only, and can only
            lower a limit.
          </p>
        )}
        {!held && page.local_left_out !== null && (
          <p className="granted-note">
            {page.local_left_out} Your dispatch limits in it still apply, because they can only
            lower one.
          </p>
        )}
        {ignored.map((one) => (
          <p key={one} className="granted-note">
            {one}
          </p>
        ))}
        {!held && (
          <div className="ui-setting-status">
            <select
              aria-label="Override for"
              value={chosen === undefined ? "" : offerId(chosen)}
              disabled={open.length === 0}
              onChange={(event) => setPicked(event.target.value)}
            >
              {open.map((one) => (
                <option key={offerId(one)} value={offerId(one)}>
                  {one.kind === "workspace" ? `Workspace ${one.name}` : `Persona ${one.name}`}
                </option>
              ))}
            </select>
            <button
              type="button"
              tabIndex={0}
              disabled={chosen === undefined}
              onClick={() => {
                if (chosen === undefined) return;
                setDrafts((was) => [...was, chosen]);
                setPicked("");
              }}
            >
              Add an override
            </button>
          </div>
        )}
        {trouble}
      </div>
    );
  };
  return { control, undo };
}

/**
 * The table on a page that is no settings row (the persona view): the same control, with the
 * last change's Undo under it, where a settings row draws its own.
 */
export function DispatchLimitsTable({ plane, scope }: { plane: PlaneId; scope?: DispatchScope }) {
  const { control, undo } = useDispatchTable(plane, scope);
  return (
    <>
      {control()}
      {undo && (
        <button type="button" className="ui-setting-reset" tabIndex={0} onClick={undo}>
          Undo
        </button>
      )}
    </>
  );
}

/** What an administrator's policy caps, each line locked: said, with who set it, and no control. */
function PolicyLocks({ plane, ids }: { plane: PlaneId; ids: RowIds }) {
  const { page, read, said } = useDispatchLimits(plane);
  /** What policy locks of who may dispatch to whom (#1437): all dispatch, or a pair. */
  const pairs = useDispatchLocks(plane);
  if (page === undefined || pairs === undefined)
    return (
      <p id={ids.id} className="ui-setting-status" aria-labelledby={ids.labelledBy}>
        {read && page === undefined ? said.join(" ") : "Reading the policy…"}
      </p>
    );
  // A policy file that was refused sets no limit to anything: it is said once, as refused, with
  // why. The pairs' lines say the same of who may dispatch to whom.
  if (page.policy_refused)
    return (
      <ul id={ids.id} className="sandbox-reasons" aria-label="Policy locks">
        <li>Dispatch is off on this machine. {page.locked_by}</li>
      </ul>
    );
  const capped =
    page.locked_by === null ? [] : page.limits.filter((limit) => limit.ceiling !== null);
  if (capped.length === 0 && pairs.length === 0)
    return (
      <p id={ids.id} className="ui-setting-status" aria-labelledby={ids.labelledBy}>
        No policy limits dispatch on this machine.
      </p>
    );
  return (
    <ul id={ids.id} className="sandbox-reasons" aria-label="Policy locks">
      {capped.map((limit) => (
        <li key={limit.word}>
          {limit.ceiling !== 0
            ? `${limit.label} is at most ${limit.ceiling}, whatever is set here.`
            : limit.word === "messages-per-minute"
              ? `${limit.label} is 0, so no chat sends another a message on this machine.`
              : `${limit.label} is 0, so dispatch is off on this machine.`}{" "}
          {page.locked_by}
        </li>
      ))}
      {pairs.map((line) => (
        <li key={line}>{line}</li>
      ))}
    </ul>
  );
}

/**
 * **The dispatch grants** on the page (#1437): which persona's chats may dispatch to which, at
 * each level, with who granted it and when, each revocable. What policy locks is the next row's.
 */
export function dispatchGrantsSetting(plane: PlaneId, file: string): LiveSetting {
  return {
    id: `${DISPATCH}.grants`,
    label: "Dispatch grants",
    help: `Which persona may dispatch to which without asking you. Revoke makes the next dispatch ask again. Everyone in this project is kept in ${file}, which your team follows.`,
    useControl: function useDispatchGrants() {
      return {
        grouped: true,
        control: (ids) => <DispatchGrantsList plane={plane} file={file} ids={ids} locks={false} />,
      };
    },
  };
}

/** The Dispatch page of the project at `plane`, whose committed file is `file`. */
export function dispatchGroup(plane: PlaneId, file = "the project's settings file"): SettingsGroup {
  return {
    id: DISPATCH,
    label: "Dispatch",
    help: "How many persona chats a chat may start, how deep a chain may go and how fast chats may message each other.",
    settings: [
      {
        id: `${DISPATCH}.limits`,
        label: "Limits",
        help: "A dispatch past a limit is refused, and the chat is told which limit.",
        useControl: function useLimits() {
          // The row draws the last change's Undo, as it does for every setting.
          return { grouped: true, ...useDispatchTable(plane) };
        },
      },
      dispatchGrantsSetting(plane, file),
      {
        id: `${DISPATCH}.locks`,
        label: "Policy locks",
        help: "What this machine's administrator caps or forbids. No setting or grant here goes past it.",
        useControl: function useLocks() {
          return { grouped: true, control: (ids) => <PolicyLocks plane={plane} ids={ids} /> };
        },
      },
    ],
  };
}

/** A workspace's own dispatch limits, on its settings. */
export function workspaceDispatchGroup(plane: PlaneId, workspace: string): SettingsGroup {
  return {
    id: "workspace.dispatch",
    label: "Dispatch",
    help: "This workspace's own dispatch limits, where they differ from the project's.",
    settings: [
      {
        id: "workspace.dispatch.limits",
        label: "Limits",
        help: "A chat working in this workspace is held to these.",
        useControl: function useWorkspaceLimits() {
          return {
            grouped: true,
            ...useDispatchTable(plane, { kind: "workspace", name: workspace }),
          };
        },
      },
    ],
  };
}
