import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  commands,
  type DispatchAny,
  type DispatchDormant,
  type DispatchGrant,
  type DispatchGrants,
  type DispatchKeptBlocked,
  type DispatchNever,
  type DispatchStanding,
  type PlaneId,
} from "../bindings";
import { Notice } from "../Notice";
import type { RowIds } from "./components";

/** No grants and no locks: what a core that answers nothing is read as. */
const NONE: DispatchGrants = {
  grants: [],
  all_locked: null,
  locked_pairs: [],
  locked_by: null,
  changed: null,
};

/** Nothing said never, nothing granted for any persona, nothing set aside. */
const NOTHING_STANDS: DispatchStanding = {
  nevers: [],
  any: [],
  nevers_unread: null,
  personas: null,
  kept_blocked: [],
  dormant: [],
};

/** "Any persona", as a target is spelled to the core. Never a persona's name. */
const ANY = "*";

/** When, as the table says it: the day and time, or nothing where it is not known. */
function when(at: number | null): string {
  if (at === null) return "";
  return new Date(at * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** Where one grant comes from, as the table says it: this chat and which, me, or the project. */
export function dispatchSourceSaid(one: DispatchGrant): string {
  const at = when(one.at);
  const then = at === "" ? "" : `, ${at}`;
  if (one.level === "chat")
    return `This chat only${one.chat === null ? "" : `: ${one.chat}`}${then}`;
  if (one.level === "you")
    return `Me on this machine${one.chat === null ? "" : `, from ${one.chat}`}${then}`;
  const by = one.by === null ? "not committed yet" : `committed by ${one.by}`;
  return `The project, ${by}${then}`;
}

/**
 * **Which workspaces a grant holds in.** Every grant holds in any workspace today. The
 * workspace condition (#1505) is said here, and nowhere else in the table.
 */
function workspaceOf(): string {
  return "Any workspace";
}

/**
 * **What a persona says it wants to dispatch to.** A persona's definition gains a `wants` list
 * that grants nothing (#1502). It is not read here yet: this is the one place the table shows
 * it, under the persona's name, once `dispatch_standing` carries it.
 */
function wantsOf(): ReactNode {
  return null;
}

/** One thing a press will do, once the person confirms it. */
interface Ask {
  /** Which button asked, so the focus goes back to it on Cancel. */
  readonly key: string;
  /** What will happen, in a sentence, before it does. */
  readonly says: string;
  /** The confirming button's word. */
  readonly yes: string;
  /** The confirming button's name for a screen reader. */
  readonly label: string;
  /** What is said once it is done. */
  readonly done: string;
  readonly run: () => Promise<{ status: "ok" } | { status: "error"; error: string }>;
}

/** One line under a target: where a grant comes from, or why nothing gets through. */
interface Line {
  readonly key: string;
  readonly says: ReactNode;
  /** A second, quieter sentence. */
  readonly note?: string;
  readonly workspace: string;
  readonly asks: readonly Omit<Ask, "key">[];
  /** Words in place of a button, where there is nothing to press. */
  readonly fixed?: string;
  /** Drawn greyed: in force for no chat. */
  readonly dormant?: boolean;
}

/** One target under a persona, with every line about it. */
interface Target {
  readonly key: string;
  readonly name: string;
  readonly lines: Line[];
}

/** One persona's part of the table. */
interface Group {
  readonly key: string;
  readonly heading: string;
  /** Why the whole group is greyed, where it is. */
  readonly unknown?: string;
  readonly targets: Target[];
}

/** `error` where a command was refused, for {@link Ask.run}. */
const ran = (done: { status: "ok" } | { status: "error"; error: string }) =>
  done.status === "ok" ? ({ status: "ok" } as const) : done;

/**
 * **The one table of who may dispatch to whom** (#1504): a row group per persona, and under it
 * each persona its chats may dispatch to, with every grant that covers the pair, where each
 * comes from (this chat, me on this machine, the project), which workspaces it holds in, and
 * the action for that source.
 *
 * - **Revoke** takes back a grant for one chat or one of mine. A project grant offers **Remove
 *   for everyone**, which edits the committed file and says so before it does, and **Not on my
 *   machine**, which leaves the file to the team. A teammate's grant nobody here has accepted
 *   is drawn waiting, with **Accept**.
 * - **Any persona** is set and cleared here and nowhere else, per persona, for me and for the
 *   project, and its confirmation says it covers personas added later.
 * - **Never** is a row with **Lift**, and says it also holds for chats working for that
 *   persona. A pair kept blocked for one chat's life is drawn read-only, with the chat.
 * - **Where the list of nevers does not read**, the core's sentence is at the top, no never is
 *   drawn, and every grant says it does not count.
 * - **A removed persona's grants** are drawn set aside and greyed, with **Remove**, and **Give
 *   back** once a persona has the name again.
 *
 * Every press asks first, in the row, and says what it will and will not do: taking a grant
 * back stops new dispatches only, and a task already running is left as it is. Every write
 * goes through the core, which reads its record again and audits before it writes; the window
 * sends names, never the table it drew, and reads everything again after each press.
 *
 * A component a Settings page mounts: Settings › Project › Dispatch.
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
  /** Whether what policy locks is listed under the table: off where a page has a row of its
   *  own for it ({@link useDispatchLocks}). */
  locks?: boolean;
}) {
  const [held, setHeld] = useState<DispatchGrants>();
  const [standing, setStanding] = useState<DispatchStanding>(NOTHING_STANDS);
  const [said, setSaid] = useState<string>();
  const [done, setDone] = useState<string>();
  const [asking, setAsking] = useState<Ask>();
  const [busy, setBusy] = useState(false);
  const whole = useRef<HTMLDivElement>(null);
  const yes = useRef<HTMLButtonElement>(null);
  /** The button a cancelled question came from, by its key. */
  const back = useRef<string | undefined>(undefined);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  /** Reads everything again. What stands is read first: reading it is what sets aside the
   *  grants of a persona that is gone, so the grants read after it are the ones in force. */
  const read = useCallback(
    () =>
      commands
        .dispatchStanding(plane)
        .then((stands) => {
          if (live.current && stands.status === "ok")
            setStanding({ ...NOTHING_STANDS, ...(stands.data ?? {}) });
        })
        // What stands could not be read: nothing is drawn as standing, and the core still
        // holds it.
        .catch(() => {})
        .then(() => commands.dispatchGrants(plane))
        .then((grants) => {
          if (!live.current) return;
          if (grants.status === "error") setSaid(grants.error);
          else setHeld(grants.data ?? NONE);
        })
        .catch((err: unknown) => {
          if (live.current) setSaid(`purlis could not list the dispatch grants: ${String(err)}`);
        }),
    [plane],
  );
  useEffect(() => {
    void read();
  }, [read]);

  // The confirming button takes the focus as it is drawn, so Enter and Escape answer it; and
  // once the question is put away, the button that asked has it again.
  useEffect(() => {
    if (asking !== undefined) {
      yes.current?.focus();
      return;
    }
    const key = back.current;
    back.current = undefined;
    if (key === undefined) return;
    for (const button of whole.current?.querySelectorAll<HTMLButtonElement>("[data-ask]") ?? [])
      if (button.dataset.ask === key) button.focus();
  }, [asking]);

  const cancel = () => {
    const key = asking?.key;
    setAsking(undefined);
    // Back to the button that asked, once it is drawn again.
    if (key !== undefined) back.current = key;
  };

  const confirm = async () => {
    if (asking === undefined) return;
    const ask = asking;
    setBusy(true);
    setSaid(undefined);
    setDone(undefined);
    let refused: string | undefined;
    try {
      const answer = await ask.run();
      if (answer.status === "error") refused = answer.error;
    } catch (err: unknown) {
      refused = typeof err === "string" ? err : `purlis could not do it: ${String(err)}`;
    }
    if (!live.current) return;
    // Read again whatever was answered: a refusal often means the table was out of date.
    await read();
    if (!live.current) return;
    setBusy(false);
    setAsking(undefined);
    if (refused === undefined) setDone(ask.done);
    else setSaid(refused);
    // The row may be gone: the focus stays in the table.
    whole.current?.focus();
  };

  const grants = held?.grants ?? [];
  const unread = standing.nevers_unread;
  const personas = standing.personas;
  const isPersona = (name: string) => personas === null || personas.includes(name);
  const notCounted = unread === null ? undefined : "Does not count until the list above reads.";
  const tasksLeft = "Tasks already running are left as they are.";

  /** The lines of one grant by name. */
  const grantLine = (one: DispatchGrant): Line => {
    const who = one.asking ?? "this chat";
    const pair = `${who} to ${one.target}`;
    const key = `grant:${one.id}`;
    if (one.locked !== null)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: one.locked,
        workspace: workspaceOf(),
        asks: [],
        fixed: "Locked by policy",
      };
    if (one.level !== "project")
      return {
        key,
        says: dispatchSourceSaid(one),
        note: notCounted,
        workspace: workspaceOf(),
        asks: [
          {
            says: `Revoke this grant? The next dispatch from ${who} to ${one.target} asks you again. ${tasksLeft}`,
            yes: "Revoke",
            label:
              one.level === "chat"
                ? `Revoke this chat's grant for ${pair}`
                : `Revoke my grant for ${pair}`,
            done: `Revoked. The next dispatch from ${who} to ${one.target} asks you. ${tasksLeft}`,
            run: async () => ran(await commands.revokeDispatchGrant(plane, one.id)),
          },
        ],
      };
    const asking = one.asking ?? "";
    const real = isPersona(asking) && isPersona(one.target);
    const remove: Omit<Ask, "key"> = {
      says: `Remove this grant for everyone? This changes ${file}, the project's committed file: your teammates lose the grant when they pull it. ${tasksLeft}`,
      yes: "Remove for everyone",
      label: `Remove the project's grant for ${pair} for everyone`,
      done: `Removed from ${file}. Commit and push the change for your team to follow it. ${tasksLeft}`,
      run: async () => ran(await commands.revokeDispatchGrant(plane, one.id)),
    };
    const decline: Omit<Ask, "key"> = {
      says: `Stop following this grant on this machine? ${file} is not changed, so your teammates keep it. The next dispatch from ${asking} to ${one.target} here asks you. ${tasksLeft}`,
      yes: "Not on my machine",
      label: `Do not follow the project's grant for ${pair} on this machine`,
      done: `Not followed on this machine. ${file} was not changed. ${tasksLeft}`,
      run: async () => ran(await commands.declineProjectDispatch(plane, asking, one.target)),
    };
    const accept: Omit<Ask, "key"> = {
      says: `Follow this grant on this machine? ${asking} chats will dispatch to ${one.target} here without asking you.`,
      yes: "Accept",
      label: `Accept the project's grant for ${pair} on this machine`,
      done: `Accepted. ${asking} chats dispatch to ${one.target} on this machine without asking.`,
      run: async () => ran(await commands.acceptProjectDispatch(plane, asking, one.target)),
    };
    if (!real)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: "It names something that is not a persona of this project, so it allows nothing.",
        workspace: workspaceOf(),
        asks: [remove],
        dormant: true,
      };
    if (one.declined)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: "Not followed on this machine: you said so. It allows nothing here.",
        workspace: workspaceOf(),
        asks: [accept, remove],
      };
    if (one.waiting)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: "Waiting for you: a teammate added it, and it allows nothing on this machine until you accept it.",
        workspace: workspaceOf(),
        asks: [accept, decline, remove],
      };
    return {
      key,
      says: dispatchSourceSaid(one),
      note: notCounted,
      workspace: workspaceOf(),
      asks: [remove, decline],
    };
  };

  /** The two lines of "any persona" for one persona: me, and the project. */
  const anyLines = (persona: string): Line[] => {
    const covers = `${persona} chats will dispatch to every persona of this project without asking you, and to any persona added later.`;
    const mine = standing.any.find((one) => one.asking === persona && one.level === "you");
    const ours = standing.any.find((one) => one.asking === persona && one.level === "project");
    const allow = (level: "you" | "project"): Omit<Ask, "key"> => ({
      says:
        level === "you"
          ? `Let ${persona} dispatch to any persona, for you on this machine? ${covers}`
          : `Let ${persona} dispatch to any persona, for everyone in this project? This changes ${file}, the project's committed file. ${covers} Each teammate accepts it on their own machine.`,
      yes: level === "you" ? "Allow for me" : "Allow for everyone",
      label:
        level === "you"
          ? `Allow ${persona} to dispatch to any persona, for me on this machine`
          : `Allow ${persona} to dispatch to any persona, for everyone in this project`,
      done: `${persona} chats may dispatch to any persona, ${level === "you" ? "for you on this machine" : "for everyone in this project"}. It covers personas added later.`,
      run: async () => ran(await commands.allowDispatchToAny(plane, persona, level)),
    });
    const clear = (level: "you" | "project"): Omit<Ask, "key"> => ({
      says:
        level === "you"
          ? `Stop letting ${persona} dispatch to any persona? Grants that name a persona stay. The next dispatch nothing else covers asks you. ${tasksLeft}`
          : `Remove "any persona" for ${persona} for everyone? This changes ${file}, the project's committed file: your teammates lose it when they pull it. Grants that name a persona stay. ${tasksLeft}`,
      yes: level === "you" ? "Clear" : "Remove for everyone",
      label:
        level === "you"
          ? `Clear any persona for ${persona}, for me on this machine`
          : `Remove any persona for ${persona} for everyone in this project`,
      done: `Cleared. ${tasksLeft}`,
      run: async () => ran(await commands.revokeDispatchToAny(plane, persona, level)),
    });
    const accept: Omit<Ask, "key"> = {
      says: `Follow the project's "any persona" for ${persona} on this machine? ${covers}`,
      yes: "Accept",
      label: `Accept any persona for ${persona} on this machine`,
      done: `Accepted. ${persona} chats may dispatch to any persona on this machine. It covers personas added later.`,
      run: async () => ran(await commands.acceptProjectDispatch(plane, persona, ANY)),
    };
    const decline: Omit<Ask, "key"> = {
      says: `Stop following the project's "any persona" for ${persona} on this machine? ${file} is not changed, so your teammates keep it. ${tasksLeft}`,
      yes: "Not on my machine",
      label: `Do not follow any persona for ${persona} on this machine`,
      done: `Not followed on this machine. ${file} was not changed. ${tasksLeft}`,
      run: async () => ran(await commands.declineProjectDispatch(plane, persona, ANY)),
    };
    const projectLine = (one: DispatchAny | undefined): Line => {
      const key = `any:project:${persona}`;
      const workspace = workspaceOf();
      if (one === undefined)
        return { key, says: "The project: not allowed", workspace, asks: [allow("project")] };
      if (one.declined)
        return {
          key,
          says: "The project: allowed",
          note: "Not followed on this machine: you said so. It allows nothing here.",
          workspace,
          asks: [accept, clear("project")],
        };
      if (one.waiting)
        return {
          key,
          says: "The project: allowed",
          note: "Waiting for you: a teammate added it, and it allows nothing on this machine until you accept it.",
          workspace,
          asks: [accept, decline, clear("project")],
        };
      return {
        key,
        says: "The project: allowed",
        note: notCounted,
        workspace,
        asks: [clear("project"), decline],
      };
    };
    return [
      mine === undefined
        ? {
            key: `any:you:${persona}`,
            says: "Me on this machine: not allowed",
            workspace: workspaceOf(),
            asks: [allow("you")],
          }
        : {
            key: `any:you:${persona}`,
            says: "Me on this machine: allowed",
            note: notCounted,
            workspace: workspaceOf(),
            asks: [clear("you")],
          },
      projectLine(ours),
    ];
  };

  const neverLine = (one: DispatchNever): Line => ({
    key: `never:${one.asking}\u001f${one.target}`,
    says: <strong>Never</strong>,
    note: `You said so on this machine. No grant covers it, and no ${one.asking} chat is asked. It also holds for a chain that starts from ${one.asking}: a chat working for a ${one.asking} chat does not dispatch to ${one.target} either.`,
    workspace: workspaceOf(),
    asks: [
      {
        says: `Lift this never? ${one.asking} chats may dispatch to ${one.target} again where a grant covers it, and where none does the next dispatch asks you.`,
        yes: "Lift",
        label: `Lift never for ${one.asking} dispatching to ${one.target}`,
        done: `Lifted. Where no grant covers ${one.asking} to ${one.target}, the next dispatch asks you.`,
        run: async () => ran(await commands.liftDispatchNever(plane, one.asking, one.target)),
      },
    ],
  });

  const keptLine = (one: DispatchKeptBlocked, at: number): Line => ({
    key: `kept:${at}`,
    says: `Kept blocked in ${one.chat === "" ? "one chat" : one.chat}`,
    note: "For that chat only, until it closes. Other chats are still asked.",
    workspace: workspaceOf(),
    asks: [],
    fixed: "Ends with the chat",
  });

  /** One group's targets, built by adding lines under each target's name. */
  const build = () => {
    const groups = new Map<string, Group>();
    const group = (asking: string | null): Group => {
      const key = asking === null ? "\u001fnone" : `p:${asking}`;
      let one = groups.get(key);
      if (one === undefined) {
        const known = asking === null || isPersona(asking);
        one = {
          key,
          heading: asking ?? "Chats on no persona",
          unknown: known
            ? undefined
            : "Not a persona of this project, so nothing here allows anything.",
          targets: [],
        };
        groups.set(key, one);
      }
      return one;
    };
    const target = (asking: string | null, name: string): Target => {
      const within = group(asking);
      let one = within.targets.find((each) => each.name === name);
      if (one === undefined) {
        one = { key: `${within.key}\u001f${name}`, name, lines: [] };
        within.targets.push(one);
      }
      return one;
    };
    // A row group for every persona, whether or not anything is granted for it.
    for (const persona of personas ?? []) group(persona);
    for (const one of grants) target(one.asking, one.target).lines.push(grantLine(one));
    if (unread === null)
      for (const one of standing.nevers) target(one.asking, one.target).lines.push(neverLine(one));
    standing.kept_blocked.forEach((one, at) =>
      target(one.asking, one.target).lines.push(keptLine(one, at)),
    );
    for (const one of standing.any) group(one.asking);
    return [...groups.values()];
  };

  const dormantLine = (one: DispatchDormant): Line => {
    const pair = one.any ? `${one.asking} to any persona` : `${one.asking} to ${one.target}`;
    const remove: Omit<Ask, "key"> = {
      says: "Remove this grant for good? It allows nothing now, so nothing changes for any chat.",
      yes: "Remove",
      label: `Remove the grant set aside for ${pair}`,
      done: "Removed.",
      run: async () => ran(await commands.removeDormantDispatch(plane, one.asking, one.target)),
    };
    const revive: Omit<Ask, "key"> = {
      says: one.any
        ? `Give this grant to the ${one.asking} this project has now? ${one.asking} chats will dispatch to every persona of this project without asking you, and to any persona added later.`
        : `Give this grant to the personas of these names this project has now? ${one.asking} chats will dispatch to ${one.target} without asking you.`,
      yes: "Give back",
      label: `Give back the grant for ${pair}`,
      done: "Given back, for you on this machine.",
      run: async () => ran(await commands.reviveDormantDispatch(plane, one.asking, one.target)),
    };
    return {
      key: `dormant:${one.asking}\u001f${one.target}\u001f${one.any ? "any" : "pair"}`,
      says: "Set aside. It was yours on this machine.",
      note: one.revivable
        ? `${one.was} was removed, and a persona has that name again. It does not get this grant unless you give it back.`
        : `${one.was} is not a persona of this project. This allows nothing.`,
      workspace: workspaceOf(),
      asks: one.revivable ? [revive, remove] : [remove],
      dormant: true,
    };
  };

  const action = (line: Line) => {
    if (asking !== undefined && asking.key.startsWith(`${line.key}\u001e`))
      return (
        <div
          className="dispatch-confirm"
          role="group"
          aria-label="Confirm"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              cancel();
            }
          }}
        >
          <p id={`${ids.id}-confirm`}>{asking.says}</p>
          <button
            type="button"
            className="ui-setting-reset"
            ref={yes}
            tabIndex={0}
            disabled={busy}
            aria-describedby={`${ids.id}-confirm`}
            onClick={() => void confirm()}
          >
            {asking.yes}
          </button>
          <button
            type="button"
            className="ui-setting-reset"
            tabIndex={0}
            disabled={busy}
            onClick={cancel}
          >
            Cancel
          </button>
        </div>
      );
    if (line.asks.length === 0) return <span className="granted-note">{line.fixed ?? ""}</span>;
    return (
      <div className="dispatch-actions">
        {line.asks.map((ask) => {
          const key = `${line.key}\u001e${ask.yes}`;
          return (
            <button
              key={key}
              type="button"
              className="ui-setting-reset"
              tabIndex={0}
              disabled={busy}
              data-ask={key}
              aria-label={ask.label}
              onClick={() => {
                setDone(undefined);
                setAsking({ ...ask, key });
              }}
            >
              {ask.yes}
            </button>
          );
        })}
      </div>
    );
  };

  /** One target's rows: its name once, down the side, and a row for each line about it. */
  const rows = (name: ReactNode, lines: readonly Line[]) =>
    lines.map((line, at) => (
      <tr key={line.key} className={line.dormant ? "dispatch-dormant" : undefined}>
        {at === 0 && (
          <th scope="row" rowSpan={lines.length}>
            {name}
          </th>
        )}
        <td>
          <span>{line.says}</span>
          {line.note !== undefined && <span className="granted-note"> {line.note}</span>}
        </td>
        <td>{line.workspace}</td>
        <td>{action(line)}</td>
      </tr>
    ));

  const groups = held === undefined ? [] : build();

  return (
    <div
      id={ids.id}
      className="dispatch-grants"
      aria-labelledby={ids.labelledBy}
      ref={whole}
      tabIndex={-1}
    >
      {unread !== null && (
        <Notice
          cause="dispatch-nevers-unread"
          at="pane"
          tone="trouble"
          // The way out is the person's: mend the file or delete it, then have it read again.
          fixes={[{ label: "Read again", onPress: () => void read() }]}
        >
          {unread} Until then purlis cannot say which pairs you said never to, so none is listed
          below and every dispatch to another persona asks you.
        </Notice>
      )}
      {held === undefined ? (
        said === undefined && <p>Reading the dispatch grants…</p>
      ) : (
        <>
          {locks && held.all_locked !== null && <p className="granted-locked">{held.all_locked}</p>}
          {groups.length === 0 && standing.dormant.length === 0 ? (
            <p>No persona&apos;s chats may dispatch to another persona yet.</p>
          ) : (
            <table className="dispatch-table">
              <caption>Who may dispatch to whom, and where each grant comes from</caption>
              <thead>
                <tr>
                  <th scope="col">May dispatch to</th>
                  <th scope="col">Where it comes from</th>
                  <th scope="col">In which workspace</th>
                  <th scope="col">Action</th>
                </tr>
              </thead>
              {groups.map((group) => {
                const persona = group.key.startsWith("p:") ? group.heading : undefined;
                const known = persona !== undefined && group.unknown === undefined;
                // What the project's file says of a name that is no persona: only its removal
                // is offered.
                const ghost =
                  persona !== undefined &&
                  !known &&
                  standing.any.some((one) => one.asking === persona && one.level === "project")
                    ? anyLines(persona)[1]
                    : undefined;
                return (
                  <tbody
                    key={group.key}
                    className={group.unknown === undefined ? undefined : "dispatch-dormant"}
                  >
                    <tr>
                      <th colSpan={4} scope="rowgroup" className="dispatch-persona">
                        <span>{group.heading}</span>
                        {group.unknown !== undefined && (
                          <span className="granted-note"> {group.unknown}</span>
                        )}
                        {known && wantsOf()}
                      </th>
                    </tr>
                    {group.targets.length === 0 && (
                      <tr>
                        <td colSpan={4} className="granted-note">
                          No grant names a persona. Its first dispatch to one asks you.
                        </td>
                      </tr>
                    )}
                    {group.targets.map((target) =>
                      rows(
                        isPersona(target.name) ? (
                          target.name
                        ) : (
                          <>
                            {target.name} <span className="granted-note">(not a persona)</span>
                          </>
                        ),
                        target.lines,
                      ),
                    )}
                    {known && rows("Any persona", anyLines(group.heading))}
                    {ghost !== undefined &&
                      rows("Any persona", [
                        {
                          ...ghost,
                          note: "It allows nothing: this is not a persona of this project.",
                          asks: ghost.asks.filter((ask) => ask.yes === "Remove for everyone"),
                          dormant: true,
                        },
                      ])}
                  </tbody>
                );
              })}
              {standing.dormant.length > 0 && (
                <tbody className="dispatch-dormant">
                  <tr>
                    <th colSpan={4} scope="rowgroup" className="dispatch-persona">
                      <span>Set aside</span>
                      <span className="granted-note">
                        {" "}
                        Grants that named a persona which was removed. They allow nothing.
                      </span>
                    </th>
                  </tr>
                  {standing.dormant.map((one) =>
                    rows(
                      one.any ? `${one.asking} to any persona` : `${one.asking} to ${one.target}`,
                      [dormantLine(one)],
                    ),
                  )}
                </tbody>
              )}
            </table>
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
      {done !== undefined && (
        <Notice
          cause={`dispatch-done:${done}`}
          at="pane"
          tone="news"
          onDismiss={() => setDone(undefined)}
        >
          {done}
        </Notice>
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
