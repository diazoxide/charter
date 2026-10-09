import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "../App";
import type { MachineProject, ThisMachine } from "../bindings";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";

/**
 * **Settings › You › This machine** (ST-2, #1226), in the whole window: the machine store's
 * recents (Forget), pins with a dangling one among them (Unpin, and its Undo), approvals
 * (Revoke) and the update channel — each action reaching the core as the command it is, and the
 * list read again after it, so what is shown is what the store holds.
 */

vi.mock("../SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const PLANE = "/home/dev/plane";

/** The machine store, as the fake core holds it. */
let store: MachineProject[];
let channel: string;
/** Every action command the window sent, with its arguments. */
let sent: { cmd: string; args: Record<string, unknown> }[];
/** The refusal the next action is answered with, if any. */
let refuse: string | undefined;
/** What every read of the store waits on: resolved, unless a test holds the reads back. */
let gate: Promise<void>;
/** The folder the next Locate…'s dialog answers with; null is a cancelled dialog. */
let picked: string | null;

function machine(): ThisMachine {
  return { projects: structuredClone(store), dropped: [], forgetful: null };
}

function project(path: string, more: Partial<MachineProject> = {}): MachineProject {
  return {
    path,
    name: path.split("/").pop() ?? path,
    gone: null,
    approved: false,
    pinned: false,
    workspace_pins: [],
    ...more,
  };
}

function act(cmd: string, args: Record<string, unknown>) {
  sent.push({ cmd, args });
  if (refuse !== undefined) throw refuse;
  const path = args.path as string;
  const one = store.find((p) => p.path === path);
  if (cmd === "forget_project") store = store.filter((p) => p.path !== path);
  if (cmd === "revoke_approval" && one) one.approved = false;
  if (cmd === "pin_on_this_machine" && one) {
    const name = args.workspace as string | null;
    if (name === null) one.pinned = args.pinned as boolean;
    else if (!(args.pinned as boolean))
      one.workspace_pins = one.workspace_pins.filter((pin) => pin.name !== name);
    else {
      const at = (args.at as number | null) ?? one.workspace_pins.length;
      // The fake knows which pins are dangling by name, as the core knows by the disk.
      one.workspace_pins.splice(at, 0, { name, gone: name === "renamed" });
    }
  }
  if (cmd === "set_update_channel") channel = args.channel as string;
  if (cmd === "locate_project") {
    // The core re-points the entry at the project it found there, which is no longer gone.
    const was = store.find((p) => p.path === args.gone);
    if (was) Object.assign(was, { path: args.picked, gone: null });
    return args.picked;
  }
  return null;
}

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/op/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
  store = [
    project(PLANE, {
      approved: true,
      pinned: true,
      workspace_pins: [
        { name: "ide", gone: false },
        { name: "renamed", gone: true },
        { name: "docs", gone: false },
      ],
    }),
    project("/mnt/usb/old", { gone: "is not there any more" }),
  ];
  channel = "stable";
  sent = [];
  refuse = undefined;
  gate = Promise.resolve();
  picked = "/mnt/disk/old";
  mockIPC(
    (cmd, args) => {
      if (cmd === "plane_at_launch") return { plane: null, from: null, why: "no plane here" };
      if (cmd === "recent_planes") return { planes: [], dropped: [], forgetful: null };
      if (cmd === "this_machine") return gate.then(machine);
      if (cmd === "update_channel") return channel;
      if (cmd === "pick_project") {
        sent.push({ cmd, args: {} });
        return picked;
      }
      if (
        [
          "forget_project",
          "revoke_approval",
          "pin_on_this_machine",
          "set_update_channel",
          "locate_project",
        ].includes(cmd)
      )
        return act(cmd, (args ?? {}) as Record<string, unknown>);
      return null;
    },
    { shouldMockEvents: true },
  );
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

/** Settings, at You › This machine, from the palette of a window with no project open. */
async function thisMachine() {
  render(<App />);
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard("Settings…");
  await userEvent.keyboard("{Enter}");
  const nav = await screen.findByRole("navigation", { name: "Groups" });
  await userEvent.click(within(nav).getByRole("button", { name: "This machine" }));
  return screen.findByRole("region", { name: "This machine" });
}

/** The list a row of the group draws, by the row's name. */
const listOf = (name: string) => screen.getByRole("group", { name });
const entries = (name: string) =>
  within(listOf(name))
    .queryAllByRole("listitem")
    .map((item) => item.textContent);

describe("You › This machine", () => {
  it("lists the recents, every pin with the dangling one marked, the approvals and the channel", async () => {
    await thisMachine();

    await waitFor(() =>
      expect(entries("Recent projects")).toEqual([
        `plane${PLANE}Forget`,
        "old/mnt/usb/oldgoneLocate…Forget",
      ]),
    );
    expect(entries("Pins")).toEqual([
      `plane${PLANE}Unpin`,
      "ide in planeUnpin",
      "renamed in planegone, kept dormantForget",
      "docs in planeUnpin",
    ]);
    expect(entries("Approved projects")).toEqual([`plane${PLANE}Revoke`]);
    expect(await screen.findByRole("radio", { name: "stable" })).toBeChecked();
  });

  it("forgets a recent that is gone, and draws the store as it is after", async () => {
    await thisMachine();

    await userEvent.click(await screen.findByRole("button", { name: "Forget old" }));

    await waitFor(() => expect(entries("Recent projects")).toEqual([`plane${PLANE}Forget`]));
    expect(sent).toEqual([{ cmd: "forget_project", args: { path: "/mnt/usb/old" } }]);
    expect(
      within(screen.getByRole("region", { name: "This machine" })).queryByRole("button", {
        name: "Undo",
      }),
      "a forgotten approval is not put back by an Undo",
    ).not.toBeInTheDocument();
  });

  it("locates a recent that has moved, as the gone notices do, and lists it where it is now", async () => {
    await thisMachine();

    await userEvent.click(await screen.findByRole("button", { name: "Locate old" }));

    await waitFor(() =>
      expect(entries("Recent projects")).toEqual([`plane${PLANE}Forget`, "old/mnt/disk/oldForget"]),
    );
    expect(sent).toEqual([
      { cmd: "pick_project", args: {} },
      { cmd: "locate_project", args: { gone: "/mnt/usb/old", picked: "/mnt/disk/old" } },
    ]);
    expect(
      screen.queryByRole("button", { name: "Undo" }),
      "Locate… re-points the entry; the old path is not put back by an Undo",
    ).not.toBeInTheDocument();
  });

  it("does nothing when Locate…'s dialog is cancelled", async () => {
    await thisMachine();
    picked = null;

    await userEvent.click(await screen.findByRole("button", { name: "Locate old" }));

    await waitFor(() => expect(sent).toEqual([{ cmd: "pick_project", args: {} }]));
    expect(entries("Recent projects")).toEqual([
      `plane${PLANE}Forget`,
      "old/mnt/usb/oldgoneLocate…Forget",
    ]);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps another row's Undo on offer when Locate…'s dialog is cancelled (D-1291-2)", async () => {
    await thisMachine();
    await userEvent.click(await screen.findByRole("button", { name: "Unpin ide in plane" }));
    await screen.findByRole("button", { name: "Undo" });
    picked = null;

    await userEvent.click(screen.getByRole("button", { name: "Locate old" }));

    await waitFor(() => expect(sent.map(({ cmd }) => cmd)).toContain("pick_project"));
    expect(sent.map(({ cmd }) => cmd)).toEqual(["pin_on_this_machine", "pick_project"]);
    expect(screen.getByRole("button", { name: "Undo" })).toBeInTheDocument();
  });

  it("says why a picked folder was refused in the recents row, and keeps the entry", async () => {
    await thisMachine();
    refuse = "/mnt/disk/old is not a purlis project: it has no workspaces folder";

    await userEvent.click(await screen.findByRole("button", { name: "Locate old" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "/mnt/disk/old is not a purlis project: it has no workspaces folder",
    );
    expect(entries("Recent projects")).toEqual([
      `plane${PLANE}Forget`,
      "old/mnt/usb/oldgoneLocate…Forget",
    ]);
  });

  it("forgets a dormant pin, and its Undo pins it back in its place", async () => {
    await thisMachine();

    await userEvent.click(await screen.findByRole("button", { name: "Forget renamed in plane" }));

    await waitFor(() =>
      expect(entries("Pins")).toEqual([
        `plane${PLANE}Unpin`,
        "ide in planeUnpin",
        "docs in planeUnpin",
      ]),
    );
    expect(sent).toEqual([
      {
        cmd: "pin_on_this_machine",
        args: { path: PLANE, workspace: "renamed", pinned: false, at: null },
      },
    ]);

    await userEvent.click(screen.getByRole("button", { name: "Undo" }));

    await waitFor(() =>
      expect(entries("Pins")).toEqual([
        `plane${PLANE}Unpin`,
        "ide in planeUnpin",
        "renamed in planegone, kept dormantForget",
        "docs in planeUnpin",
      ]),
    );
    expect(sent[1]).toEqual({
      cmd: "pin_on_this_machine",
      args: { path: PLANE, workspace: "renamed", pinned: true, at: 1 },
    });
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("puts a pin back where it was when it was unpinned, after another unpin settled", async () => {
    await thisMachine();
    await screen.findByRole("button", { name: "Unpin ide in plane" });
    // Reads held back, so both buttons are pressed on the list as first drawn: [ide, renamed,
    // docs], where docs is third. By its turn ide is gone, and docs is second.
    let release = () => {};
    gate = new Promise((done) => {
      release = done;
    });

    await userEvent.click(screen.getByRole("button", { name: "Unpin ide in plane" }));
    await userEvent.click(screen.getByRole("button", { name: "Unpin docs in plane" }));
    release();
    gate = Promise.resolve();

    await waitFor(() =>
      expect(entries("Pins")).toEqual([
        `plane${PLANE}Unpin`,
        "renamed in planegone, kept dormantForget",
      ]),
    );
    await userEvent.click(screen.getByRole("button", { name: "Undo" }));

    await waitFor(() =>
      expect(entries("Pins")).toEqual([
        `plane${PLANE}Unpin`,
        "renamed in planegone, kept dormantForget",
        "docs in planeUnpin",
      ]),
    );
    expect(sent.at(-1)).toEqual({
      cmd: "pin_on_this_machine",
      args: { path: PLANE, workspace: "docs", pinned: true, at: 1 },
    });
  });

  it("revokes an approval, keeping the project in the recents, with no Undo", async () => {
    await thisMachine();

    await userEvent.click(await screen.findByRole("button", { name: "Revoke plane" }));

    await waitFor(() =>
      expect(within(listOf("Approved projects")).queryAllByRole("listitem")).toHaveLength(0),
    );
    expect(listOf("Approved projects")).toHaveTextContent(
      "No project is approved on this machine.",
    );
    expect(entries("Recent projects")).toHaveLength(2);
    expect(sent).toEqual([{ cmd: "revoke_approval", args: { path: PLANE } }]);
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
  });

  it("says a refusal in the row it was made in, in the core's words", async () => {
    await thisMachine();
    refuse = "purlis could not forget that project: the store is locked";

    await userEvent.click(await screen.findByRole("button", { name: "Forget plane" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "purlis could not forget that project: the store is locked",
    );
    expect(entries("Recent projects")).toHaveLength(2);
  });

  it("puts this machine on another update channel", async () => {
    await thisMachine();

    await userEvent.click(await screen.findByRole("radio", { name: "dev" }));

    await waitFor(() => expect(screen.getByRole("radio", { name: "dev" })).toBeChecked());
    expect(sent).toEqual([{ cmd: "set_update_channel", args: { channel: "dev" } }]);
  });
});
