import { Fragment, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
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
};

/** Nothing said never, nothing granted for any persona, nothing set aside. */
const NOTHING_STANDS: DispatchStanding = {
  nevers: [],
  any: [],
  nevers_unread: null,
  personas: null,
  kept_blocked: [],
  dormant: [],
  returned: [],
  back: [],
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

/** What a command answered, as a press needs it. */
type Ran = { status: "ok" } | { status: "error"; error: string };

/** One thing a press will do, once the person confirms it. */
interface Offer {
  /** The button's words, and the confirming button's. */
  readonly yes: string;
  /** Whose grant it is about. The button's name for a screen reader is its words, then this:
   *  so a person who says the words they see names the button. */
  readonly about: string;
  /** What will happen, in a sentence, before it does. */
  readonly says: string;
  /** What is said once it is done. */
  readonly done: string;
  readonly run: () => Promise<Ran>;
}

/** An offer the person pressed, waiting on their confirmation. */
interface Ask extends Offer {
  /** Which button asked: the focus goes back to it on Cancel. */
  readonly key: string;
  /** The line it is on: the confirmation is drawn under that line. */
  readonly line: string;
}

/** One line under a target: where a grant comes from, or why nothing gets through. */
interface Line {
  readonly key: string;
  readonly says: ReactNode;
  /** A second, quieter sentence. */
  readonly note?: string;
  readonly workspace: string;
  readonly offers: readonly Offer[];
  /** Words in place of a button, where there is nothing to press. */
  readonly fixed?: string;
  /** Drawn greyed: in force for no chat. */
  readonly dormant?: boolean;
}

/** One target under a persona, with every line about it. */
interface Target {
  readonly name: string;
  readonly lines: Line[];
}

/** One persona's part of the table. */
interface Group {
  readonly key: string;
  /** The persona, where the group is one's; none for chats on no persona. */
  readonly persona?: string;
  readonly heading: string;
  readonly targets: Target[];
}

/** A command's answer, as {@link Offer.run} gives it. */
const ran = (done: { status: "ok" } | { status: "error"; error: string }): Ran =>
  done.status === "ok" ? { status: "ok" } : done;

/** A button's accessible name: its visible words first, then whose grant it is about. */
const named = (offer: Offer) => `${offer.yes}: ${offer.about}`;

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
 * - **A grant is in force only while both personas exist.** One that names a persona the
 *   project does not have now is drawn greyed and says so; nothing was moved, and it counts
 *   again when the persona is back. Where a persona of that name is there again and is not
 *   the one that left, its grants wait for **Give back**, one press for the name, and what was
 *   set aside is listed with **Remove**.
 *
 * Every press asks first, in a line under its row, and says what it will and will not do:
 * taking a grant back stops new dispatches only, and a task already running is left as it is.
 * Every write goes through the core, which reads its record again and audits before it writes;
 * the window sends names, never the table it drew, and reads everything again after each
 * press. Reading the table changes no grant.
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
  /** Why what stands beside the grants could not be read, where it could not. */
  const [unknown, setUnknown] = useState<string>();
  const [said, setSaid] = useState<string>();
  const [done, setDone] = useState<string>();
  const [asking, setAsking] = useState<Ask>();
  const [busy, setBusy] = useState(false);
  const whole = useRef<HTMLDivElement>(null);
  const yes = useRef<HTMLButtonElement>(null);
  /** Where the focus goes once a question is put away: the button that asked (Cancel), or
   *  the line acted on and how far down the table it was (a press that was confirmed). */
  const next = useRef<{ key?: string; line: string; at: number } | undefined>(undefined);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  /** Reads everything again: what stands beside the grants, then the grants. */
  const read = useCallback(
    () =>
      commands
        .dispatchStanding(plane)
        .then((stands) => {
          if (!live.current) return;
          if (stands.status === "error") setUnknown(stands.error);
          else {
            setUnknown(undefined);
            setStanding({ ...NOTHING_STANDS, ...(stands.data ?? {}) });
          }
        })
        .catch((err: unknown) => {
          if (live.current) setUnknown(String(err));
        })
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

  // The confirming button takes the focus as it is drawn, so Enter and Escape answer it. Once
  // the question is put away the focus goes to the button that asked; where that is gone, to
  // the first button of the same line; where the line is gone, to the button now at its place
  // in the table; and only with no button left, to the table itself.
  useEffect(() => {
    if (asking !== undefined) {
      yes.current?.focus();
      return;
    }
    const to = next.current;
    next.current = undefined;
    if (to === undefined) return;
    const buttons = [...(whole.current?.querySelectorAll<HTMLButtonElement>("[data-ask]") ?? [])];
    const target =
      buttons.find((one) => one.dataset.ask === to.key) ??
      buttons.find((one) => one.dataset.line === to.line) ??
      buttons[Math.min(to.at, buttons.length - 1)];
    (target ?? whole.current)?.focus();
  }, [asking]);

  /** How far down the table the buttons of `line` start. */
  const placeOf = (line: string) =>
    Math.max(
      0,
      [...(whole.current?.querySelectorAll<HTMLButtonElement>("[data-ask]") ?? [])].findIndex(
        (one) => one.dataset.line === line,
      ),
    );

  const cancel = () => {
    if (asking !== undefined)
      next.current = { key: asking.key, line: asking.line, at: placeOf(asking.line) };
    setAsking(undefined);
  };

  const confirm = async () => {
    if (asking === undefined) return;
    const ask = asking;
    const at = placeOf(ask.line);
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
    if (refused === undefined) setDone(ask.done);
    else setSaid(refused);
    next.current = { line: ask.line, at };
    setAsking(undefined);
  };

  const grants = held?.grants ?? [];
  const unread = standing.nevers_unread;
  const personas = standing.personas;
  const isPersona = (name: string) => personas === null || personas.includes(name);
  const isBack = (name: string) => standing.returned.includes(name);
  const tasksLeft = "Tasks already running are left as they are.";

  /** Why a grant for `names` is in force for no chat just now, where it is not; the first
   *  reason that holds. */
  const notInForce = (names: readonly string[]): string | undefined => {
    const gone = names.find((name) => !isPersona(name));
    if (gone !== undefined)
      return `Not in force while ${gone} is not a persona of this project. Nothing was moved: it counts again when ${gone} is back.`;
    const back = names.find(isBack);
    if (back !== undefined)
      return `Not in force: ${back} was gone, and the persona of that name now is not the one that left. Give back to ${back}, or take this back.`;
    return undefined;
  };

  /** What is added to "it is allowed" where it does not count yet, for `asking` to `target`
   *  (`*` for any persona): said after Accept, Allow and Give back, so none says more than
   *  is true. */
  const yetToCount = (asking: string, target: string): string => {
    if (unread !== null) return " It does not count until the list of nevers reads.";
    const never = standing.nevers.some(
      (one) => one.asking === asking && (target === ANY || one.target === target),
    );
    if (!never) return "";
    return target === ANY
      ? ` Where you said never for ${asking}, the never still holds.`
      : ` You said never to this pair, so it does not count until you lift that.`;
  };

  /** The quieter sentence of a grant that would be in force: why it does not count, if so. */
  const countsNote = (names: readonly string[]): string | undefined =>
    notInForce(names) ??
    (unread === null ? undefined : "Does not count until the list above reads.");

  /** The line of one grant by name. */
  const grantLine = (one: DispatchGrant): Line => {
    const who = one.asking ?? "this chat";
    const pair = `${who} to ${one.target}`;
    const key = `grant:${one.id}`;
    const names = one.asking === null ? [one.target] : [one.asking, one.target];
    const stalled = notInForce(names);
    const workspace = workspaceOf();
    if (one.locked !== null)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: one.locked,
        workspace,
        offers: [],
        fixed: "Locked by policy",
      };
    if (one.level !== "project")
      return {
        key,
        says: dispatchSourceSaid(one),
        note: countsNote(names),
        workspace,
        dormant: stalled !== undefined,
        offers: [
          {
            yes: "Revoke",
            about: one.level === "chat" ? `this chat's grant for ${pair}` : `my grant for ${pair}`,
            says: `Revoke this grant? The next dispatch from ${who} to ${one.target} asks you again. ${tasksLeft}`,
            done: `Revoked. The next dispatch from ${who} to ${one.target} asks you. ${tasksLeft}`,
            run: async () => ran(await commands.revokeDispatchGrant(plane, one.id)),
          },
        ],
      };
    const asking = one.asking ?? "";
    const remove: Offer = {
      yes: "Remove for everyone",
      about: `the project's grant for ${pair}`,
      says: `Remove this grant for everyone? This changes ${file}, the project's committed file: your teammates lose the grant when they pull it. ${tasksLeft}`,
      done: `Removed from ${file}. Commit and push the change for your team to follow it. ${tasksLeft}`,
      run: async () => ran(await commands.revokeDispatchGrant(plane, one.id)),
    };
    const decline: Offer = {
      yes: "Not on my machine",
      about: `stop following the project's grant for ${pair}`,
      says: `Stop following this grant on this machine? ${file} is not changed, so your teammates keep it. The next dispatch from ${asking} to ${one.target} here asks you. ${tasksLeft}`,
      done: `Not followed on this machine. ${file} was not changed. ${tasksLeft}`,
      run: async () => ran(await commands.declineProjectDispatch(plane, asking, one.target)),
    };
    const accept: Offer = {
      yes: "Accept",
      about: `the project's grant for ${pair}, on this machine`,
      says: `Follow this grant on this machine? ${asking} chats will dispatch to ${one.target} here without asking you.`,
      done: `Accepted on this machine: the project's grant for ${asking} to ${one.target}.${yetToCount(asking, one.target)}`,
      run: async () => ran(await commands.acceptProjectDispatch(plane, asking, one.target)),
    };
    const gone = names.find((name) => !isPersona(name));
    if (gone !== undefined)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: `It names ${gone}, which is not a persona of this project now, so it allows nothing.`,
        workspace,
        offers: one.waiting ? [remove] : [remove, decline],
        dormant: true,
      };
    if (one.declined)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: "Not followed on this machine: you said so. It allows nothing here.",
        workspace,
        offers: [accept, remove],
      };
    if (one.waiting)
      return {
        key,
        says: dispatchSourceSaid(one),
        note: "Waiting for you: it is the project's, and it allows nothing on this machine until you accept it.",
        workspace,
        offers: [accept, decline, remove],
      };
    return {
      key,
      says: dispatchSourceSaid(one),
      note: countsNote(names),
      workspace,
      dormant: stalled !== undefined,
      offers: [remove, decline],
    };
  };

  /** The two lines of "any persona" for one persona: me, and the project. For a name that is
   *  no persona now, only what is there is drawn, with only its taking back. */
  const anyLines = (persona: string): Line[] => {
    const here = isPersona(persona);
    const covers = `${persona} chats will dispatch to every persona of this project without asking you, and to any persona added later.`;
    const mine = standing.any.find((one) => one.asking === persona && one.level === "you");
    const ours = standing.any.find((one) => one.asking === persona && one.level === "project");
    const workspace = workspaceOf();
    const allow = (level: "you" | "project"): Offer => ({
      yes: level === "you" ? "Allow for me" : "Allow for everyone",
      about: `${persona} may dispatch to any persona`,
      says:
        level === "you"
          ? `Let ${persona} dispatch to any persona, for you on this machine? ${covers}`
          : `Let ${persona} dispatch to any persona, for everyone in this project? This changes ${file}, the project's committed file. ${covers} Each teammate accepts it on their own machine.`,
      done: `Allowed ${level === "you" ? "for you on this machine" : "for everyone in this project"}: ${persona} to any persona. It covers personas added later.${yetToCount(persona, ANY)}`,
      run: async () => ran(await commands.allowDispatchToAny(plane, persona, level)),
    });
    const clear = (level: "you" | "project"): Offer => ({
      yes: level === "you" ? "Clear" : "Remove for everyone",
      about:
        level === "you"
          ? `any persona for ${persona}, for me on this machine`
          : `any persona for ${persona}, in this project`,
      says:
        level === "you"
          ? `Stop letting ${persona} dispatch to any persona? Grants that name a persona stay. The next dispatch nothing else covers asks you. ${tasksLeft}`
          : `Remove "any persona" for ${persona} for everyone? This changes ${file}, the project's committed file: your teammates lose it when they pull it. Grants that name a persona stay. ${tasksLeft}`,
      done: `Cleared. ${tasksLeft}`,
      run: async () => ran(await commands.revokeDispatchToAny(plane, persona, level)),
    });
    const accept: Offer = {
      yes: "Accept",
      about: `the project's any persona for ${persona}, on this machine`,
      says: `Follow the project's "any persona" for ${persona} on this machine? ${covers}`,
      done: `Accepted on this machine: ${persona} to any persona. It covers personas added later.${yetToCount(persona, ANY)}`,
      run: async () => ran(await commands.acceptProjectDispatch(plane, persona, ANY)),
    };
    const decline: Offer = {
      yes: "Not on my machine",
      about: `stop following the project's any persona for ${persona}`,
      says: `Stop following the project's "any persona" for ${persona} on this machine? ${file} is not changed, so your teammates keep it. ${tasksLeft}`,
      done: `Not followed on this machine. ${file} was not changed. ${tasksLeft}`,
      run: async () => ran(await commands.declineProjectDispatch(plane, persona, ANY)),
    };
    const gone = `Not in force while ${persona} is not a persona of this project.`;
    const mineLine = (): Line | undefined => {
      const key = `any:you:${persona}`;
      if (mine === undefined)
        return here
          ? { key, says: "Me on this machine: not allowed", workspace, offers: [allow("you")] }
          : undefined;
      return {
        key,
        says: "Me on this machine: allowed",
        note: here ? countsNote([persona]) : gone,
        workspace,
        dormant: notInForce([persona]) !== undefined,
        offers: [clear("you")],
      };
    };
    const oursLine = (one: DispatchAny | undefined): Line | undefined => {
      const key = `any:project:${persona}`;
      if (one === undefined)
        return here
          ? { key, says: "The project: not allowed", workspace, offers: [allow("project")] }
          : undefined;
      if (!here)
        return {
          key,
          says: "The project: allowed",
          note: gone,
          workspace,
          dormant: true,
          offers: [clear("project")],
        };
      if (one.declined)
        return {
          key,
          says: "The project: allowed",
          note: "Not followed on this machine: you said so. It allows nothing here.",
          workspace,
          offers: [accept, clear("project")],
        };
      if (one.waiting)
        return {
          key,
          says: "The project: allowed",
          note: "Waiting for you: it is the project's, and it allows nothing on this machine until you accept it.",
          workspace,
          offers: [accept, decline, clear("project")],
        };
      return {
        key,
        says: "The project: allowed",
        note: countsNote([persona]),
        workspace,
        dormant: notInForce([persona]) !== undefined,
        offers: [clear("project"), decline],
      };
    };
    return [mineLine(), oursLine(ours)].filter((one): one is Line => one !== undefined);
  };

  const neverLine = (one: DispatchNever): Line => {
    // A never is not set aside when its persona goes: it holds for whoever has the name.
    const names = [one.asking, one.target];
    const gone = names.find((name) => !isPersona(name));
    const earlier = names.find((name) => standing.back.includes(name));
    const whose =
      gone !== undefined
        ? ` ${gone} is not a persona of this project now: the never still holds, and will hold for a persona made under that name.`
        : earlier !== undefined
          ? ` It was said of an earlier persona named ${earlier}, and still holds for this one.`
          : "";
    return {
      key: `never:${one.asking}\u001f${one.target}`,
      says: <strong>Never</strong>,
      note: `You said so on this machine. No grant covers it, and no ${one.asking} chat is asked. It also holds for a chain that starts from ${one.asking}: a chat working for a ${one.asking} chat does not dispatch to ${one.target} either.${whose}`,
      workspace: workspaceOf(),
      offers: [
        {
          yes: "Lift",
          about: `never for ${one.asking} dispatching to ${one.target}`,
          says: `Lift this never? ${one.asking} chats may dispatch to ${one.target} again where a grant covers it, and where none does the next dispatch asks you.`,
          done: `Lifted. Where no grant covers ${one.asking} to ${one.target}, the next dispatch asks you.`,
          run: async () => ran(await commands.liftDispatchNever(plane, one.asking, one.target)),
        },
      ],
    };
  };

  const keptLine = (one: DispatchKeptBlocked, at: number): Line => ({
    key: `kept:${at}`,
    says: `Kept blocked in ${one.chat === "" ? "one chat" : one.chat}`,
    note: "For that chat only, until it closes. Other chats are still asked.",
    workspace: workspaceOf(),
    offers: [],
    fixed: "Ends with the chat",
  });

  const dormantLine = (one: DispatchDormant): Line => {
    const pair = one.any ? `${one.asking} to any persona` : `${one.asking} to ${one.target}`;
    const here = one.was !== "" && isPersona(one.was);
    return {
      key: `dormant:${one.asking}\u001f${one.target}\u001f${one.any ? "any" : "pair"}`,
      says: "Set aside. It was yours on this machine.",
      note:
        one.was === ""
          ? "It allows nothing, and there is no persona to give it back to."
          : here
            ? `It was an earlier ${one.was}'s. It allows nothing unless you give it back, with Give back to ${one.was}.`
            : `It was an earlier ${one.was}'s, and ${one.was} is not a persona of this project now. It allows nothing.`,
      workspace: workspaceOf(),
      offers: [
        {
          yes: "Remove",
          about: `the grant set aside for ${pair}`,
          says: "Remove this grant for good? It allows nothing now, so nothing changes for any chat.",
          done: "Removed.",
          run: async () =>
            ran(await commands.removeDormantDispatch(plane, one.asking, one.target, one.any)),
        },
      ],
      dormant: true,
    };
  };

  /** The one acknowledgement for a persona that has the name of one that was gone. */
  const giveBack = (name: string): Offer => {
    const aside = standing.dormant.filter((one) => one.was === name).length;
    const held =
      aside === 0
        ? "Its grants were never moved, and count again."
        : `${aside} ${aside === 1 ? "grant" : "grants"} set aside for it ${aside === 1 ? "comes" : "come"} back where the other persona exists, with what this machine had accepted of the project's.`;
    return {
      yes: `Give back to ${name}`,
      about: "what an earlier persona of this name was allowed",
      says: `Let the ${name} this project has now have what an earlier persona named ${name} was allowed? ${held} It may then dispatch, and be dispatched to, as the earlier one could, without asking you.`,
      done: `Given back to ${name}.${unread !== null ? " It does not count until the list of nevers reads." : ""}`,
      run: async () => ran(await commands.giveBackDispatch(plane, name)),
    };
  };

  /** The groups, built by adding lines under each target's name. */
  const build = () => {
    const groups = new Map<string, Group>();
    const group = (asking: string | null): Group => {
      const key = asking === null ? "\u001fnone" : `p:${asking}`;
      let one = groups.get(key);
      if (one === undefined) {
        one = {
          key,
          persona: asking ?? undefined,
          heading: asking ?? "Chats on no persona",
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
        one = { name, lines: [] };
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

  /** The buttons of one line, each asking first. */
  const buttons = (line: string, offers: readonly Offer[]) => (
    <div className="dispatch-actions">
      {offers.map((offer) => {
        const key = `${line}\u001e${offer.yes}`;
        return (
          <button
            key={key}
            type="button"
            className="ui-setting-reset"
            tabIndex={0}
            disabled={busy}
            data-ask={key}
            data-line={line}
            aria-label={named(offer)}
            aria-expanded={asking?.key === key}
            onClick={() => {
              setDone(undefined);
              setAsking({ ...offer, key, line });
            }}
          >
            {offer.yes}
          </button>
        );
      })}
    </div>
  );

  /** The question of `line`, where it is the one asked: a row of its own, the table's whole
   *  width, under the row it is about. It never widens a column or re-flows the rows above. */
  const question = (line: string) =>
    asking?.line === line && (
      <tr className="dispatch-confirm-row">
        <td colSpan={4}>
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
        </td>
      </tr>
    );

  /** One target's rows: its name once, down the side, and a row for each line about it. */
  const rows = (name: ReactNode, lines: readonly Line[]) => {
    // The question's row is under the name too, so the name spans it.
    const span = lines.length + (lines.some((line) => asking?.line === line.key) ? 1 : 0);
    return lines.map((line, at) => (
      <Fragment key={line.key}>
        <tr className={line.dormant ? "dispatch-dormant" : undefined}>
          {at === 0 && (
            <th scope="row" rowSpan={span}>
              {name}
            </th>
          )}
          <td>
            <span>{line.says}</span>
            {line.note !== undefined && <span className="granted-note"> {line.note}</span>}
          </td>
          <td>{line.workspace}</td>
          <td>
            {line.offers.length === 0 ? (
              <span className="granted-note">{line.fixed ?? ""}</span>
            ) : (
              buttons(line.key, line.offers)
            )}
          </td>
        </tr>
        {question(line.key)}
      </Fragment>
    ));
  };

  const groups = held === undefined || unknown !== undefined ? [] : build();

  return (
    <div
      id={ids.id}
      className="dispatch-grants"
      aria-labelledby={ids.labelledBy}
      ref={whole}
      tabIndex={-1}
    >
      {unknown !== undefined && (
        <Notice
          cause="dispatch-standing-unread"
          at="pane"
          tone="trouble"
          fixes={[{ label: "Read again", onPress: () => void read() }]}
        >
          purlis could not read what stands of dispatch here: the pairs you said never to, any
          persona, and which personas this project has ({unknown}). The table is not drawn, since it
          could not say which grants count. Nothing was changed.
        </Notice>
      )}
      {unknown === undefined && unread !== null && (
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
      {unknown !== undefined ? null : held === undefined ? (
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
                const persona = group.persona;
                const here = persona !== undefined && isPersona(persona);
                const back = persona !== undefined && here && standing.back.includes(persona);
                const anys = persona === undefined ? [] : anyLines(persona);
                const heading = `heading:${group.key}`;
                return (
                  <tbody
                    key={group.key}
                    className={persona !== undefined && !here ? "dispatch-dormant" : undefined}
                  >
                    <tr>
                      <th colSpan={4} scope="rowgroup" className="dispatch-persona">
                        <span>{group.heading}</span>
                        {persona !== undefined && !here && (
                          <span className="granted-note">
                            {" "}
                            Not a persona of this project now, so nothing here allows anything.
                          </span>
                        )}
                        {back && persona !== undefined && (
                          <>
                            <span className="granted-note">
                              {" "}
                              An earlier persona had this name. What it was allowed does not count
                              for this one until you give it back.
                            </span>
                            {buttons(heading, [giveBack(persona)])}
                          </>
                        )}
                        {here && wantsOf()}
                      </th>
                    </tr>
                    {question(heading)}
                    {group.targets.length === 0 && here && (
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
                    {anys.length > 0 && rows("Any persona", anys)}
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
                        Grants an earlier persona of a name had. They allow nothing.
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
