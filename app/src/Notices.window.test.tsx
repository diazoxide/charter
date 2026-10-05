import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render as renderBare, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { countsInStatusBar, IMPORTANCE } from "./Notice";
import { forgetThisLaunch } from "./regions";
import { GLOBAL } from "./windowprefs";

/**
 * **Notices under the strip, against the whole window** (NO-2 #1229, rulings V91i, V91j).
 *
 * - A Dismiss lasts until the Notice's cause changes: it is kept on this machine, in the layout
 *   file beside the window's other preferences, so a relaunch keeps it; and it clears itself
 *   once the core answers without the cause, so a return shows the Notice again.
 * - At most two Notices stand under the strip, the most important first; the rest are behind
 *   "+N more".
 *
 * A relaunch is simulated the way the app does one: what the window last wrote to the layout
 * file is handed to the next window as it is created (`windowprefs.ts`).
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";

type Asked = { cmd: string; args: unknown };

/** One chat as the core reports it. */
function chat(
  session: number,
  name: string,
  said: { resumed?: string; fresh?: string; guessed?: string } = {},
) {
  return {
    session,
    name,
    cwd: `${PLANE}/workspaces/alpha`,
    harness: said.fresh ? "claude" : null,
    in_front: session === 1,
    resumed: said.resumed ?? null,
    fresh: said.fresh ?? null,
    profile: null,
    persona: null,
    unreported: null,
    guessed: said.guessed ?? null,
    pinned: false,
  };
}

/**
 * The core: a project with `alpha`, the chats it puts back, the pins whose workspace is gone,
 * and the chats that would not start. `pins` makes the store's read never answer, or refuse.
 */
function core(
  open: ReturnType<typeof chat>[] = [chat(1, "one")],
  world: { gone: string[]; wouldNotStart?: [string, string][]; pins?: "hangs" | "refuses" } = {
    gone: [],
  },
): { asked: Asked[] } {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        workspaces: [
          {
            name: "alpha",
            path: `${PLANE}/workspaces/alpha`,
            vision: "",
            todos: [],
            chats: open,
          },
        ],
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
      };
    if (cmd === "opened_chats") return open;
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return world.wouldNotStart ?? [];
    if (cmd === "running_sessions") return [];
    if (cmd === "plane_pins") {
      if (world.pins === "hangs") return new Promise(() => undefined);
      if (world.pins === "refuses") throw "the machine store could not be read";
      return { project: false, workspaces: [], missing: [...world.gone], order: [...world.gone] };
    }
    return null;
  });
  return { asked };
}

/** The layout file, as the last launch left it: a launch that wrote nothing leaves it as is. */
let onDisk: string | undefined;

/** Quits, and launches again on what the window last wrote to the layout file, handed to the
 *  next window as it is created. */
function relaunchFrom(asks: Asked[]) {
  const written = asks.filter((one) => one.cmd === "write_layout").at(-1);
  onDisk = (written?.args as { text?: string } | undefined)?.text ?? onDisk;
  cleanup();
  clearMocks();
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: {
      path: "layout.json",
      found: onDisk !== undefined,
      document: onDisk === undefined ? null : JSON.parse(onDisk),
      trouble: null,
    },
    theme: { path: "theme.json", found: false, document: null, trouble: null },
  };
}

beforeEach(() => {
  onDisk = undefined;
  forgetThisLaunch();
});

afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

/** The band's Notices, top to bottom, by cause. */
const band = () =>
  [...document.querySelectorAll(".notice-band-shown [data-cause]")].map((one) =>
    one.getAttribute("data-cause"),
  );
const dormant = (name: string) => screen.findByText(new RegExp(`^${name} is gone, kept dormant`));
const noticeOf = (text: HTMLElement) => text.closest("[data-cause]") as HTMLElement;
const dismiss = async (text: HTMLElement) =>
  userEvent.click(within(noticeOf(text)).getByRole("button", { name: "Dismiss" }));
/** Lets every answer in flight land. */
const settle = () => new Promise((done) => setTimeout(done, 50));

describe("a dismissed Notice", () => {
  it("stays dismissed after a relaunch on this machine", async () => {
    const { asked } = core([chat(1, "one")], { gone: ["delta"] });
    render(<App />);
    await dismiss(await dormant("delta"));
    expect(screen.queryByText(/^delta is gone/)).toBeNull();
    await waitFor(() => expect(asked.some((one) => one.cmd === "write_layout")).toBe(true));

    relaunchFrom(asked);
    core([chat(1, "one")], { gone: ["delta"] });
    render(<App />);
    await screen.findByRole("tablist", { name: "Workspaces" });
    await settle();

    expect(screen.queryByText(/^delta is gone/)).toBeNull();
  });

  it("shows again once its cause has gone and come back", async () => {
    // Launch one: dismissed. Launch two: the workspace is back, so the store answers without the
    // pin, and the dismissal is let go. Launch three: gone again, and the Notice is drawn.
    let { asked } = core([chat(1, "one")], { gone: ["delta"] });
    render(<App />);
    await dismiss(await dormant("delta"));
    await waitFor(() => expect(asked.some((one) => one.cmd === "write_layout")).toBe(true));

    relaunchFrom(asked);
    ({ asked } = core([chat(1, "one")], { gone: [] }));
    render(<App />);
    await screen.findByRole("tablist", { name: "Workspaces" });
    await waitFor(() => expect(asked.some((one) => one.cmd === "write_layout")).toBe(true));
    const written = asked.filter((one) => one.cmd === "write_layout").at(-1);
    expect(JSON.parse((written?.args as { text: string }).text).dismissed).toBeUndefined();

    relaunchFrom(asked);
    core([chat(1, "one")], { gone: ["delta"] });
    render(<App />);
    expect(await dormant("delta")).toBeInTheDocument();
  });

  it("is not let go by a launch whose store did not answer", async () => {
    // Before the store answers, and when it cannot, the window holds no pins at all: that is
    // the window not knowing, not the pin having gone, so nothing is let go.
    let { asked } = core([chat(1, "one")], { gone: ["delta"] });
    render(<App />);
    await dismiss(await dormant("delta"));
    await waitFor(() => expect(asked.some((one) => one.cmd === "write_layout")).toBe(true));

    for (const pins of ["hangs", "refuses"] as const) {
      relaunchFrom(asked);
      ({ asked } = core([chat(1, "one")], { gone: ["delta"], pins }));
      render(<App />);
      await screen.findByRole("tablist", { name: "Workspaces" });
      await settle();
    }

    relaunchFrom(asked);
    core([chat(1, "one")], { gone: ["delta"] });
    render(<App />);
    await screen.findByRole("tablist", { name: "Workspaces" });
    await settle();
    expect(screen.queryByText(/^delta is gone/)).toBeNull();
  });

  it("about what a relaunch did to a chat stays dismissed while it keeps doing it", async () => {
    const fresh = [chat(1, "one", { fresh: "its conversation was not found" })];
    const { asked } = core(fresh, { gone: [] });
    render(<App />);
    const note = await screen.findByText(/came back as a new chat/);
    expect(noticeOf(note).getAttribute("data-cause")).toBe("chat-fresh:1");
    await dismiss(note);
    await waitFor(() => expect(asked.some((one) => one.cmd === "write_layout")).toBe(true));

    relaunchFrom(asked);
    core(fresh, { gone: [] });
    render(<App />);
    await screen.findByRole("tablist", { name: "Workspaces" });
    await settle();
    expect(screen.queryByText(/came back as a new chat/)).toBeNull();
  });

  it("about an event is not kept: the next occurrence shows", async () => {
    // A chat that did not start is an event of this launch; its Dismiss ends that occurrence.
    const { asked } = core([chat(1, "one")], { gone: [], wouldNotStart: [["two", "no profile"]] });
    render(<App />);
    await dismiss(await screen.findByText(/did not start/));
    await settle();

    relaunchFrom(asked);
    core([chat(1, "one")], { gone: [], wouldNotStart: [["two", "no profile"]] });
    render(<App />);
    expect(await screen.findByText(/did not start/)).toBeInTheDocument();
  });
});

describe("the Notices under the strip", () => {
  /** The Notices in the open "+N more" list, top to bottom, by cause. */
  const more = () =>
    [...document.querySelectorAll(".notice-more-list [data-cause]")].map((one) =>
      one.getAttribute("data-cause"),
    );

  /** Four Notices: trouble (a chat that did not start), two dormant pins, and news about a chat
   *  that came back new — drawn by the window in another order than their importance. */
  const four = () => {
    core([chat(1, "one", { fresh: "its conversation was not found" })], {
      gone: ["able", "baker"],
      wouldNotStart: [["two", "no profile"]],
    });
    render(<App />);
  };

  it("are at most two, the most important first, and the rest are behind +N more", async () => {
    four();
    await screen.findByRole("button", { name: "+2 more" });
    expect(band()).toEqual(["chat-did-not-start:two", "pin-dormant:able"]);
    expect(more()).toEqual([]);

    await userEvent.click(screen.getByRole("button", { name: "+2 more" }));

    expect(more()).toEqual(["pin-dormant:baker", "chat-fresh:1"]);
    // Each is a whole Notice, with its ways out.
    const listed = document.querySelector('.notice-more-list [data-cause="pin-dormant:baker"]');
    expect(within(listed as HTMLElement).getByRole("button", { name: "Forget" })).toBeTruthy();
  });

  it("move up as one is dismissed, and +N more goes once nothing is behind it", async () => {
    four();
    await screen.findByRole("button", { name: "+2 more" });

    await dismiss(await dormant("able"));
    expect(band()).toEqual(["chat-did-not-start:two", "pin-dormant:baker"]);
    expect(screen.getByRole("button", { name: "+1 more" })).toBeInTheDocument();

    await dismiss(await screen.findByText(/did not start/));
    expect(band()).toEqual(["pin-dormant:baker", "chat-fresh:1"]);
    expect(screen.queryByRole("button", { name: /more$/ })).toBeNull();
  });
});

describe("the status bar", () => {
  it("counts no Notice the doctor did not also find", async () => {
    // V91i: a Notice counts there only if it also comes from the doctor, whose button counts
    // it. No family of Notice comes from the doctor yet, so the band adds nothing to the bar.
    expect(IMPORTANCE.filter(countsInStatusBar)).toEqual([]);
    core([chat(1, "one")], { gone: [] });
    render(<App />);
    const alerts = await screen.findByRole("button", { name: /^Alerts/ });
    const before = alerts.getAttribute("aria-label");

    cleanup();
    clearMocks();
    forgetThisLaunch();
    core([chat(1, "one", { fresh: "its conversation was not found" })], {
      gone: ["able", "baker"],
      wouldNotStart: [["two", "no profile"]],
    });
    render(<App />);
    await screen.findByRole("button", { name: "+2 more" });
    expect(
      (await screen.findByRole("button", { name: /^Alerts/ })).getAttribute("aria-label"),
    ).toBe(before);
  });
});
