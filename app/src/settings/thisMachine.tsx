import { useSyncExternalStore, type ReactNode } from "react";
import { commands, type MachineProject, type ThisMachine } from "../bindings";
import { Choice } from "./components";
import type { LiveSetting, SettingsGroup } from "./groups";
import { channelMoved, readChannel, useUpdateChannel } from "../updateChannel";

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
 * **Undo only where it puts back exactly what was there** (D-ST2-3): an unpin, or forgetting a
 * dormant pin (one whose workspace is gone, V91c as amended), which is pinned again in its
 * place. Forgetting a project and Revoke take an approval away, and an approval is only ever
 * made by the operator answering the ask at the next open — never by an Undo here.
 *
 * **One action at a time** (D-ST2-5): each waits for the last one's read, so a place an Undo
 * puts a pin back at is the place it had then, not the place a stale render showed.
 */
export function thisMachineGroup(): SettingsGroup {
  return {
    id: "you.machine",
    label: "This machine",
    help: "What purlis remembers on this machine: your recent projects, your pins, the projects you approved, and where updates come from.",
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

/**
 * **A recent settled here** (#1291): forgotten, or located at a new path. The window listens, so
 * its own "project gone" line for that path goes too, as it does when the line itself settles
 * it; otherwise Locate… on that line would ask about a path the store no longer remembers.
 */
const settledListeners = new Set<(path: string) => void>();

/** Calls `listener` with the old path of every recent forgotten or located here; returns the
 *  way to stop. */
export function whenRecentSettled(listener: (path: string) => void): () => void {
  settledListeners.add(listener);
  return () => {
    settledListeners.delete(listener);
  };
}

/** `done`, having told the listeners `path` was settled when it was done. */
function settling<T extends Done>(path: string, done: T): T {
  if (done.status === "ok") for (const one of settledListeners) one(path);
  return done;
}

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
          : { state: "trouble", trouble: "purlis did not say what this machine remembers." },
  }));
}

/**
 * **The store changed elsewhere** — a pin on the project strip, another window, the CLI — while
 * the group is on screen (#1240): it is read again when the window comes back into focus or into
 * view, the cheap cover, since the core says nothing when its store changes. The channel too.
 */
function cameBack() {
  if (document.visibilityState === "hidden") return;
  void reread();
  readChannel();
}

/** Something in this window changed the store outside the group: read it again if it is up. */
export function machineChanged(): void {
  if (listeners.size > 0) void reread();
}

function subscribe(listener: () => void) {
  // The first row on screen starts afresh: the store may have changed while none was.
  if (listeners.size === 0) {
    held = START;
    void reread();
    window.addEventListener("focus", cameBack);
    document.addEventListener("visibilitychange", cameBack);
  }
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
    if (listeners.size > 0) return;
    window.removeEventListener("focus", cameBack);
    document.removeEventListener("visibilitychange", cameBack);
  };
}

/** What a command answered: `undefined` once done, or why not. */
type Done = { status: "ok" } | { status: "error"; error: string };
function refusal(done: Done | undefined): string | undefined {
  return done?.status === "error" ? done.error : undefined;
}

/** Every action, one at a time: each runs against the store as the last one's read left it. */
let queue: Promise<void> = Promise.resolve();

/**
 * Does `act` for the row `setting`, then reads the store again. `undo`, when given, is asked at
 * the action's turn — against the latest read, never the render that offered the button — for
 * what puts it back, which becomes the one Undo on offer; any other action ends the last one's.
 */
function acting(
  setting: string,
  act: () => Promise<Done>,
  undo?: (machine: ThisMachine | undefined) => () => Promise<Done>,
) {
  const work = async () => {
    const back = undo?.(held.read.state === "read" ? held.read.machine : undefined);
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
          why === undefined && back
            ? {
                setting,
                run: () =>
                  back()
                    .then(refusal)
                    .catch((err: unknown) => String(err)),
              }
            : undefined,
      };
    });
    await reread();
  };
  queue = queue.then(work, work);
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

/** An entry's button: what it says, what a screen reader calls it, and what it does. */
type EntryAction = { action: string; label: string; onAction: () => void };

function EntryButton({ action, label, onAction }: EntryAction) {
  return (
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
  );
}

/** One entry of a list: what it is, what is said about it, and its action (or two). */
function Entry({
  name,
  path,
  note,
  before,
  ...main
}: {
  name: string;
  path?: string;
  note?: string;
  /** An action offered before the entry's own, as Locate… is before Forget. */
  before?: EntryAction;
} & EntryAction) {
  return (
    <li className="ui-machine-entry">
      <span className="ui-machine-name">{name}</span>
      {path !== undefined && <span className="ui-machine-path">{path}</span>}
      {note !== undefined && <span className="ui-setting-badge">{note}</span>}
      {before !== undefined && <EntryButton {...before} />}
      <EntryButton {...main} />
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
  "The projects purlis offers when it opens. Forget takes one off this machine, with its pins and approval; it is asked about again the next time it is opened. One that has moved or gone can be located where it is now, or forgotten.",
  (machine) => ({
    empty: "No recent projects.",
    entries: machine.projects.map((one) => (
      <Entry
        key={one.path}
        name={one.name}
        path={one.path}
        note={one.gone === null ? undefined : "gone"}
        before={
          one.gone === null
            ? undefined
            : { action: "Locate…", label: `Locate ${one.name}`, onAction: () => void locate(one) }
        }
        action="Forget"
        label={`Forget ${one.name}`}
        onAction={() =>
          acting("you.machine.recents", () =>
            commands.forgetProject(one.path).then((done) => settling(one.path, done)),
          )
        }
      />
    )),
  }),
);

/**
 * **Locate…** for a recent that has moved or gone (#1291), as `GoneProjectNotice` does it (NO-5):
 * a folder is asked for, and the core re-points the entry once it has checked a project is
 * there (`locate_project`); the approval does not travel with the path. A cancelled dialog does
 * nothing, not even end another row's Undo. A refusal is said in the row, and so is a dialog
 * that could not open, which is not a cancel. Nothing is opened from Settings: the entry now
 * lists where the project is, and opening it, through the trust gate, is the opener's.
 */
async function locate(project: MachineProject) {
  const picked = await commands
    .pickProject()
    .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
  if (picked.status === "error") {
    acting("you.machine.recents", () => Promise.resolve(picked));
    return;
  }
  if (!picked.data) return;
  const folder = picked.data;
  acting("you.machine.recents", async () => {
    const located = await commands.locateProject(project.path, folder);
    return settling(project.path, located.status === "ok" ? { status: "ok" } : located);
  });
}

/**
 * Unpins `workspace` in `project`, or the project itself — for a pin whose workspace is gone,
 * forgets it — with the Undo that pins it back in its place. The place is the pin's in the
 * latest read at the action's turn: an earlier unpin still settling has moved it (F1).
 */
function unpin(project: MachineProject, workspace?: string) {
  acting(
    "you.machine.pins",
    () => commands.pinOnThisMachine(project.path, workspace ?? null, false, null),
    (machine) => {
      const now = machine?.projects.find((one) => one.path === project.path) ?? project;
      const at =
        workspace === undefined
          ? -1
          : now.workspace_pins.findIndex((pin) => pin.name === workspace);
      return () =>
        commands.pinOnThisMachine(project.path, workspace ?? null, true, at < 0 ? null : at);
    },
  );
}

const pins = list(
  "you.machine.pins",
  "Pins",
  "The projects and workspaces you pinned, first in the opener and on the workspace strip. A pin to a workspace that is gone is kept dormant: off the strip, and back in its place if the workspace returns. Forget removes it for good.",
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
      ...one.workspace_pins.map((pin) => {
        const action = pin.gone ? "Forget" : "Unpin";
        return (
          <Entry
            key={`${one.path}\u0000${pin.name}`}
            name={`${pin.name} in ${one.name}`}
            note={pin.gone ? "gone, kept dormant" : undefined}
            action={action}
            label={`${action} ${pin.name} in ${one.name}`}
            onAction={() => unpin(one, pin.name)}
          />
        );
      }),
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

/** The channels a machine can be on (`purlis_core::updates::Channel`). */
const CHANNELS = [
  { value: "stable", label: "stable", says: "Releases cut on the maintainers' word." },
  { value: "dev", label: "dev", says: "Every green build of main, for testing." },
];

const channel: LiveSetting = {
  id: "you.machine.channel",
  label: "Update channel",
  help: "Which stream this machine takes purlis from. It applies to the next check for an update.",
  useControl: function useChannel() {
    const id = "you.machine.channel";
    const { error } = useMachine(id);
    // One value with the title bar's updater (#1240).
    const now = useUpdateChannel();
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
              if (done.status === "ok") channelMoved(to);
              return done;
            })
          }
        />
      ),
    };
  },
};
