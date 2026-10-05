import { useSyncExternalStore, type ReactNode } from "react";
import { commands, type MachineProject, type ThisMachine } from "../bindings";
import { Choice } from "./components";
import type { LiveSetting, SettingsGroup } from "./groups";

/**
 * **You › This machine** (ST-2, #1226; V91r): what this machine's store holds — its recent
 * projects, its pins (a dangling one included), its approvals — and the update channel, each
 * taken back or changed from here. Nothing in it is a project's: it is the machine store,
 * beside the layout file, and it reaches no clone.
 *
 * **One read, shared by the group's rows**, and read again after every action, so what is
 * listed is what the store holds. Each row's last refusal is said in that row, in the core's
 * words.
 *
 * **Undo only where it puts back exactly what was there** (D-ST2-3): an unpin, which is pinned
 * again in its place. Forget and Revoke take an approval away, and an approval is only ever
 * made by the operator answering the ask at the next open — never by an Undo here.
 */
export function thisMachineGroup(): SettingsGroup {
  return {
    id: "you.machine",
    label: "This machine",
    help: "What charter remembers on this machine: your recent projects, your pins, the projects you approved, and where updates come from.",
    settings: [recents, pins, approvals, channel],
  };
}

type Read =
  | { state: "reading" }
  | { state: "trouble"; trouble: string }
  | { state: "read"; machine: ThisMachine };

/** The last action that can be undone: the row it was made in, and what puts it back. */
type Undo = { setting: string; run: () => Promise<string | undefined> };

type Held = {
  read: Read;
  /** Why a row's last action was refused, by setting. */
  refused: Readonly<Record<string, readonly string[]>>;
  undo: Undo | undefined;
};

const START: Held = { read: { state: "reading" }, refused: {}, undo: undefined };
let held: Held = START;
const listeners = new Set<() => void>();
/** The newest read out: an answer to an older one is dropped. */
let reading = 0;

function set(change: (was: Held) => Held) {
  held = change(held);
  for (const one of listeners) one();
}

async function reread() {
  const mine = ++reading;
  const said = await commands
    .thisMachine()
    .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
  if (mine !== reading) return;
  set((was) => ({
    ...was,
    // A core that answered nothing (an older one, or none at all) is said as trouble, never
    // drawn as an empty machine.
    read:
      said.status === "error"
        ? { state: "trouble", trouble: said.error }
        : typeof said.data === "object" && said.data !== null
          ? { state: "read", machine: said.data }
          : { state: "trouble", trouble: "charter did not say what this machine remembers." },
  }));
}

function subscribe(listener: () => void) {
  // The first row on screen starts afresh: the store may have changed while none was.
  if (listeners.size === 0) {
    held = START;
    void reread();
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** What a command answered: `undefined` once done, or why not. */
type Done = { status: "ok" } | { status: "error"; error: string };
function refusal(done: Done | undefined): string | undefined {
  return done?.status === "error" ? done.error : undefined;
}

/**
 * Does `act` for the row `setting`, then reads the store again. `undo`, when given, becomes the
 * one Undo on offer; any other action ends the last one's.
 */
function acting(setting: string, act: () => Promise<Done>, undo?: () => Promise<Done>) {
  void (async () => {
    const why = await act()
      .then(refusal)
      .catch((err: unknown) => String(err));
    set((was) => {
      const refused = { ...was.refused };
      if (why === undefined) Reflect.deleteProperty(refused, setting);
      else refused[setting] = [why];
      return {
        ...was,
        refused,
        undo:
          why === undefined && undo
            ? {
                setting,
                run: () =>
                  undo()
                    .then(refusal)
                    .catch((err: unknown) => String(err)),
              }
            : undefined,
      };
    });
    await reread();
  })();
}

function useMachine(setting: string) {
  const now = useSyncExternalStore(subscribe, () => held);
  const undo = now.undo;
  return {
    read: now.read,
    error: now.refused[setting],
    undo:
      undo?.setting === setting
        ? () => {
            set((was) => ({ ...was, undo: undefined }));
            acting(setting, async () => {
              const why = await undo.run();
              return why === undefined ? { status: "ok" } : { status: "error", error: why };
            });
          }
        : undefined,
  };
}

/** What stands in for a list until the store has been read, or why it could not be. */
function waiting(read: Read) {
  if (read.state === "reading") return <p className="ui-setting-help">Reading…</p>;
  if (read.state === "trouble")
    return (
      <p className="ui-setting-help" role="alert">
        {read.trouble}
      </p>
    );
  if (read.machine.forgetful !== null)
    return <p className="ui-setting-help">{read.machine.forgetful}</p>;
  return undefined;
}

/** One entry of a list: what it is, what is said about it, and its one action. */
function Entry({
  name,
  path,
  note,
  action,
  label,
  onAction,
}: {
  name: string;
  path?: string;
  note?: string;
  action: string;
  /** What the action's button is called to a screen reader: the action and the entry. */
  label: string;
  onAction: () => void;
}) {
  return (
    <li className="ui-machine-entry">
      <span className="ui-machine-name">{name}</span>
      {path !== undefined && <span className="ui-machine-path">{path}</span>}
      {note !== undefined && <span className="ui-setting-badge">{note}</span>}
      <button
        type="button"
        className="ui-setting-reset"
        // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
        tabIndex={0}
        aria-label={label}
        onClick={onAction}
      >
        {action}
      </button>
    </li>
  );
}

/** A list setting: its entries, or what it says when there are none. */
function list(
  id: string,
  label: string,
  help: string,
  draw: (machine: ThisMachine) => { empty: string; entries: ReactNode[] },
): LiveSetting {
  return {
    id,
    label,
    help,
    useControl: function useList() {
      const { read, error, undo } = useMachine(id);
      return {
        grouped: true,
        error,
        undo,
        control: (ids) => {
          const instead = waiting(read);
          if (instead !== undefined || read.state !== "read") return instead;
          const { empty, entries } = draw(read.machine);
          return (
            <div role="group" aria-labelledby={ids.labelledBy} aria-describedby={ids.describedBy}>
              {entries.length === 0 ? (
                <p className="ui-setting-help">{empty}</p>
              ) : (
                <ul className="ui-machine-list">{entries}</ul>
              )}
            </div>
          );
        },
      };
    },
  };
}

const recents = list(
  "you.machine.recents",
  "Recent projects",
  "The projects charter offers when it opens. Forget takes one off this machine, with its pins and approval; it is asked about again the next time it is opened. One that has moved or gone can be forgotten here too.",
  (machine) => ({
    empty: "No recent projects.",
    entries: machine.projects.map((one) => (
      <Entry
        key={one.path}
        name={one.name}
        path={one.path}
        note={one.gone === null ? undefined : "gone"}
        action="Forget"
        label={`Forget ${one.name}`}
        onAction={() => acting("you.machine.recents", () => commands.forgetProject(one.path))}
      />
    )),
  }),
);

/** Unpins `workspace` in `project`, or the project itself, with the Undo that pins it back in
 *  its place. */
function unpin(project: MachineProject, workspace?: string) {
  const at =
    workspace === undefined
      ? null
      : project.workspace_pins.findIndex((pin) => pin.name === workspace);
  acting(
    "you.machine.pins",
    () => commands.pinOnThisMachine(project.path, workspace ?? null, false, null),
    () => commands.pinOnThisMachine(project.path, workspace ?? null, true, at),
  );
}

const pins = list(
  "you.machine.pins",
  "Pins",
  "The projects and workspaces you pinned, first in the opener and on the workspace strip. A pin to a workspace that is gone is listed here, so it can be unpinned.",
  (machine) => ({
    empty: "Nothing is pinned.",
    entries: machine.projects.flatMap((one) => [
      ...(one.pinned
        ? [
            <Entry
              key={`${one.path}\u0000`}
              name={one.name}
              path={one.path}
              note={one.gone === null ? undefined : "gone"}
              action="Unpin"
              label={`Unpin ${one.name}`}
              onAction={() => unpin(one)}
            />,
          ]
        : []),
      ...one.workspace_pins.map((pin) => (
        <Entry
          key={`${one.path}\u0000${pin.name}`}
          name={`${pin.name} in ${one.name}`}
          note={pin.gone ? "gone" : undefined}
          action="Unpin"
          label={`Unpin ${pin.name} in ${one.name}`}
          onAction={() => unpin(one, pin.name)}
        />
      )),
    ]),
  }),
);

const approvals = list(
  "you.machine.approvals",
  "Approved projects",
  "The projects you approved opening on this machine, with what they start. Revoke takes the approval back: the project stays in your recents, and opening it asks again.",
  (machine) => ({
    empty: "No project is approved on this machine.",
    entries: machine.projects
      .filter((one) => one.approved)
      .map((one) => (
        <Entry
          key={one.path}
          name={one.name}
          path={one.path}
          action="Revoke"
          label={`Revoke ${one.name}`}
          onAction={() => acting("you.machine.approvals", () => commands.revokeApproval(one.path))}
        />
      )),
  }),
);

/** The channels a machine can be on (`charter_core::updates::Channel`). */
const CHANNELS = [
  { value: "stable", label: "stable", says: "Releases cut on the maintainers' word." },
  { value: "dev", label: "dev", says: "Every green build of main, for testing." },
];

const channel: LiveSetting = {
  id: "you.machine.channel",
  label: "Update channel",
  help: "Which stream this machine takes charter from. It applies to the next check for an update.",
  useControl: function useChannel() {
    const id = "you.machine.channel";
    const { error } = useMachine(id);
    const now = useSyncExternalStore(subscribeChannel, () => chosen);
    return {
      grouped: true,
      error,
      control: (ids) => (
        <Choice
          kind="radio"
          ids={ids}
          options={CHANNELS}
          value={now}
          onValueChange={(to) =>
            acting(id, async () => {
              const done = await commands.setUpdateChannel(to);
              if (done.status === "ok") setChosen(to);
              return done;
            })
          }
        />
      ),
    };
  },
};

/** The channel this machine is on, once asked. */
let chosen: string | undefined;
const channelListeners = new Set<() => void>();
function setChosen(to: string | undefined) {
  chosen = to;
  for (const one of channelListeners) one();
}
function subscribeChannel(listener: () => void) {
  if (channelListeners.size === 0) {
    chosen = undefined;
    void commands
      .updateChannel()
      .then((now) => setChosen(typeof now === "string" ? now : undefined))
      .catch(() => undefined);
  }
  channelListeners.add(listener);
  return () => {
    channelListeners.delete(listener);
  };
}
