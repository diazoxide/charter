import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

/**
 * Pinning, against the whole window (charter ADR 0039, stored per ADR 0040).
 *
 * **Three levels and three stores, which is the thing these are here to keep true.** A
 * project's pin and a workspace's go in the machine store; a chat's goes in the plane's own
 * app record. A design that treated "pin" as one feature would pass a test that only ever
 * pinned one kind of thing, so every one of the three is driven here, end to end, through
 * the surface an operator uses — which is the palette, because that is where the rows are.
 *
 * What a pin DOES is draw the thing first on its strip. That is the assertion in each case,
 * and it is what makes a pin more than a marker.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

/** One chat as the core reports it, with only what these tests read worth setting. */
function chat(session: number, name: string, workspace: string, pinned = false) {
  return {
    session,
    name,
    cwd: `${PLANE}/workspaces/${workspace}`,
    harness: "claude",
    in_front: session === 1,
    resumed: null,
    fresh: null,
    profile: null,
    persona: null,
    unreported: null,
    pinned,
  };
}

/** A plane with three workspaces, so an order is something a pin can change. */
function sidebar(open: ReturnType<typeof chat>[]) {
  return {
    root: PLANE,
    workspaces: ["alpha", "beta", "gamma"].map((name) => ({
      name,
      path: `${PLANE}/workspaces/${name}`,
      vision: "",
      todos: [],
      chats: open.filter((one) => one.cwd.endsWith(name)),
    })),
    personas: ["steward"],
    persona: "steward",
    unfiled: [],
  };
}

type Asked = { cmd: string; args: unknown };

/** The core, with the chats it has open and what the machine store says is pinned. */
function core(
  open: ReturnType<typeof chat>[],
  pins: { project: boolean; workspaces: string[]; missing: string[] } = {
    project: false,
    workspaces: [],
    missing: [],
  },
  refuse?: string,
): { asked: Asked[] } {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return sidebar(open);
    if (cmd === "opened_chats") return open;
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "plane_pins") return pins;
    if (cmd === "pin_chat" || cmd === "pin_workspace" || cmd === "pin_project") {
      // A refusal is a THROWN value and not an `Error`, which is what `typedError` turns
      // into `{ status: "error" }` — the same shape the core's own refusals arrive in.
      if (refuse !== undefined) throw refuse;
      return null;
    }
    return null;
  });
  return { asked };
}

/** Opens the palette, types, and runs the row Enter is aimed at. */
async function runFromPalette(typed: string) {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard(typed);
  await userEvent.keyboard("{Enter}");
}

const strip = (name: string) => screen.getByRole("tablist", { name });
const namesIn = (name: string, inside: string) =>
  within(strip(name))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(inside)?.textContent);
const chatNames = () => namesIn("Tabs", ".tab-name");
const workspaceNames = () => namesIn("Workspaces", ".workspace-name");
const pinned = () => screen.queryAllByRole("img", { name: /^pinned / }).map((one) => one.ariaLabel);
const asked = (asks: Asked[], cmd: string) => asks.filter((one) => one.cmd === cmd);

describe("a pinned chat", () => {
  it("comes back pinned, and is drawn first on its strip", async () => {
    // The pin rides the record the chat came back from, so a relaunch keeps the arrangement.
    core([chat(1, "one", "alpha"), chat(2, "two", "alpha"), chat(3, "three", "alpha", true)]);
    render(<App />);

    await vi.waitFor(() => expect(chatNames()).toEqual(["three", "one", "two"]));
    expect(pinned()).toContain("pinned chat");
  });

  it("is marked, and nothing unpinned is", async () => {
    core([chat(1, "one", "alpha"), chat(2, "two", "alpha", true)]);
    render(<App />);

    await vi.waitFor(() => expect(chatNames()).toEqual(["two", "one"]));
    const marks = within(strip("Tabs")).getAllByRole("tab");
    expect(within(marks[0]).getByRole("img", { name: "pinned chat" })).toBeInTheDocument();
    expect(within(marks[1]).queryByRole("img", { name: "pinned chat" })).toBeNull();
  });

  it("is pinned by the palette, and the core is told", async () => {
    const { asked: asks } = core([chat(1, "one", "alpha"), chat(2, "two", "alpha")]);
    render(<App />);
    await vi.waitFor(() => expect(chatNames()).toEqual(["one", "two"]));

    await runFromPalette("Pin chat two");

    await vi.waitFor(() => expect(chatNames()).toEqual(["two", "one"]));
    expect(asked(asks, "pin_chat").map((one) => one.args)).toEqual([
      { plane: PLANE, session: 2, pinned: true },
    ]);
  });

  it("is not moved on screen when the core refused to write the pin", async () => {
    // The mark is what the operator reads as "this is pinned", so it must follow the core's
    // write and never the click. A pin the next launch does not have is worse than none.
    const { asked: asks } = core(
      [chat(1, "one", "alpha"), chat(2, "two", "alpha")],
      undefined,
      "no",
    );
    render(<App />);
    await vi.waitFor(() => expect(chatNames()).toEqual(["one", "two"]));

    await runFromPalette("Pin chat two");

    expect(asked(asks, "pin_chat")).toHaveLength(1);
    // The palette stays up on a refusal, and it is modal — Radix marks everything behind it
    // `aria-hidden`, so the strip is not reachable until it is answered. That is the app
    // behaving correctly (`docs/ui-primitives.md`), so the test takes the route that exists.
    await userEvent.keyboard("{Escape}");
    expect(chatNames()).toEqual(["one", "two"]);
    expect(pinned()).toEqual([]);
  });

  it("offers Unpin once it is pinned, and nothing else", async () => {
    core([chat(1, "one", "alpha", true)]);
    render(<App />);
    await vi.waitFor(() => expect(chatNames()).toEqual(["one"]));

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("pin chat");

    const rows = screen.getAllByRole("option").map((row) => row.textContent ?? "");
    expect(rows.some((row) => row.includes("Unpin chat one"))).toBe(true);
    expect(rows.some((row) => row.includes("Pin chat one"))).toBe(false);
  });
});

describe("a pinned workspace", () => {
  it("is drawn first on the workspace strip, in the plane's own order", async () => {
    core([chat(1, "one", "alpha")], { project: false, workspaces: ["gamma", "beta"], missing: [] });
    render(<App />);

    // `beta` before `gamma` although the pins arrived the other way round: a pin says WHICH
    // workspaces come first, never in what order they do.
    await vi.waitFor(() => expect(workspaceNames()).toEqual(["beta", "gamma", "alpha"]));
    expect(pinned().filter((one) => one === "pinned workspace")).toHaveLength(2);
  });

  it("is pinned by the palette, and the machine store is asked again", async () => {
    const { asked: asks } = core([chat(1, "one", "alpha")]);
    render(<App />);
    await vi.waitFor(() => expect(workspaceNames()).toEqual(["alpha", "beta", "gamma"]));
    const before = asked(asks, "plane_pins").length;

    await runFromPalette("Pin workspace beta");

    expect(asked(asks, "pin_workspace").map((one) => one.args)).toEqual([
      { plane: PLANE, workspace: "beta", pinned: true },
    ]);
    // The store is what says what is pinned, so the window asks it rather than assuming its
    // own write landed as it expected.
    await vi.waitFor(() => expect(asked(asks, "plane_pins").length).toBeGreaterThan(before));
  });

  it("is never offered for the chats outside every workspace", async () => {
    // That strip is not a workspace on the plane, so there is nothing on disk for a pin to
    // name — and a row that wrote one would put a name in the store that resolves to nothing.
    core([chat(1, "one", "elsewhere")]);
    render(<App />);
    await vi.waitFor(() => expect(chatNames()).toEqual(["one"]));

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Pin workspace");

    const rows = screen.getAllByRole("option").map((row) => row.textContent ?? "");
    expect(rows.some((row) => row.includes("outside"))).toBe(false);
  });

  it("says so when a pin no longer names a workspace, and does not draw it", async () => {
    // The hazard ADR 0034 names for a trust entry keyed on a path, one scope down: a
    // reference that no longer resolves must not become something charter offers.
    core([chat(1, "one", "alpha")], { project: false, workspaces: [], missing: ["was-here"] });
    render(<App />);

    expect(await screen.findByText(/was-here is not on this plane any more/)).toBeInTheDocument();
    await vi.waitFor(() => expect(workspaceNames()).toEqual(["alpha", "beta", "gamma"]));
  });
});

describe("a pinned project", () => {
  it("is drawn first on the project strip, and marked", async () => {
    // **Two projects, because one cannot show an order.** A cold launch that restores both
    // is the shortest route to a window holding two, and it is the state the pin is for.
    const other = "/home/dev/other";
    mockIPC((cmd, args) => {
      if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
      if (cmd === "planes_to_restore") return { planes: [PLANE, other], active: 0, dropped: [] };
      if (cmd === "open_plane") return { plane: (args as { path: string }).path, ask: null };
      if (cmd === "plane_sidebar") return sidebar([]);
      if (cmd === "opened_chats") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      // Only the SECOND project is pinned, so a strip that drew them in the order they were
      // opened would fail this and a strip that simply reversed them would pass it.
      if (cmd === "plane_pins")
        return {
          project: (args as { plane: string }).plane === other,
          workspaces: [],
          missing: [],
        };
      return null;
    });
    render(<App />);

    await vi.waitFor(() => expect(pinned()).toContain("pinned project"));
    expect(namesIn("Projects", ".project-name")).toEqual(["other", "plane"]);
  });

  it("is pinned by the palette, and the core is told which project", async () => {
    const { asked: asks } = core([chat(1, "one", "alpha")]);
    render(<App />);
    await vi.waitFor(() => expect(chatNames()).toEqual(["one"]));

    await runFromPalette("Pin project plane");

    expect(asked(asks, "pin_project").map((one) => one.args)).toEqual([
      { plane: PLANE, pinned: true },
    ]);
    await vi.waitFor(() => expect(pinned()).toContain("pinned project"));
  });

  it("shows the core's refusal in the core's own words", async () => {
    // The store is bounded, and "charter pins at most 32 projects. Unpin one first." is a
    // sentence the operator can act on. A pin that silently did not happen is a control that
    // does not work.
    core([chat(1, "one", "alpha")], undefined, "charter pins at most 32 projects.");
    render(<App />);
    await vi.waitFor(() => expect(chatNames()).toEqual(["one"]));

    await runFromPalette("Pin project plane");

    expect(await screen.findByText("charter pins at most 32 projects.")).toBeInTheDocument();
  });
});
