import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  configure,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { GoneProject } from "./bindings";

/**
 * **A project that is gone offers Locate… and Forget** (NO-5, #1237): in the window's Notice
 * for a project the last quit had open, and on the opener for a recent. Never done by charter
 * on its own, because a disk that is unplugged may come back.
 *
 * Forget is the core's (ST-2), so a gone project stops coming back at every launch. Locate…
 * asks for a folder, the core checks there is a project there and re-points the entry, and the
 * window opens what it found, through the trust gate like any open.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);
// A whole window comes up through several answers; under a loaded machine that takes more
// than the default second.
configure({ asyncUtilTimeout: 5_000 });

afterEach(() => {
  cleanup();
  clearMocks();
});

const TWO = "/home/dev/two";
const FOUND = "/media/dev/moved";
const GONE: GoneProject = { path: "/home/dev/gone", said: "~/dev/gone is no longer there" };

function sidebarOf(root: string) {
  return { root, workspaces: [], personas: [], persona: null, unfiled: [] };
}

/**
 * A core with one gone project, either among the last quit's tabs (`restore`) or among the
 * recents (`recent`), with every call kept. `picked` is what the folder picker answers, and
 * `locate` what the core answers to the re-point.
 */
function core(over: {
  where: "restore" | "recent" | "both";
  picked?: string | null;
  locate?: { ok: string } | { refused: string };
}) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  let remembered = true;
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plane_at_launch") return { plane: null, from: null, why: null };
    if (cmd === "planes_to_restore")
      return {
        // "both": nothing came back, so the window draws the opener, which lists it too.
        windows: over.where === "restore" ? [{ planes: [TWO], active: 0 }] : [],
        dropped: [],
        gone: over.where !== "recent" && remembered ? [GONE] : [],
      };
    if (cmd === "recent_planes")
      return {
        planes: [{ path: TWO, name: "two", approved: true }],
        dropped: [],
        gone: over.where !== "restore" && remembered ? [GONE] : [],
        forgetful: null,
      };
    if (cmd === "pick_project") return over.picked === undefined ? FOUND : over.picked;
    if (cmd === "locate_project") {
      const answer = over.locate ?? { ok: FOUND };
      if ("refused" in answer) throw answer.refused;
      remembered = false;
      return answer.ok;
    }
    if (cmd === "forget_project") {
      remembered = false;
      return null;
    }
    if (cmd === "open_plane") return { plane: given.path, ask: null };
    if (cmd === "plane_sidebar") return sidebarOf((given.plane as string) ?? "");
    if (cmd === "opened_chats") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "extensions_on") return [];
    return null;
  });
  return {
    asked,
    sent: (name: string) => asked.filter((one) => one.cmd === name).map((one) => one.args),
  };
}

const projectTabs = () =>
  within(screen.getByRole("tablist", { name: "Projects" }))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".project-name")?.textContent);

/** The Notice about the gone project, wherever it stands. */
const gone = async () =>
  (await screen.findByText(GONE.said)).closest<HTMLElement>("[data-cause]") as HTMLElement;

describe("a project the last quit had open that is gone", () => {
  it("offers Locate… and Forget, and opens the rest", async () => {
    core({ where: "restore" });
    render(<App />);

    const notice = await gone();
    expect(within(notice).getByRole("button", { name: "Locate…" })).toBeInTheDocument();
    expect(within(notice).getByRole("button", { name: "Forget" })).toBeInTheDocument();
    await waitFor(() => expect(projectTabs()).toEqual(["two"]));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("forgets it on this machine, so it no longer comes back at launch", async () => {
    const { sent } = core({ where: "restore" });
    render(<App />);

    await userEvent.click(within(await gone()).getByRole("button", { name: "Forget" }));

    await waitFor(() => expect(screen.queryByText(GONE.said)).not.toBeInTheDocument());
    expect(sent("forget_project")).toEqual([{ path: GONE.path }]);
  });

  it("re-points it at a picked folder once the core has checked it, and opens it", async () => {
    const { sent } = core({ where: "restore" });
    render(<App />);
    await waitFor(() => expect(projectTabs()).toEqual(["two"]));

    await userEvent.click(within(await gone()).getByRole("button", { name: "Locate…" }));

    await waitFor(() => expect(projectTabs()).toEqual(["two", "moved"]));
    expect(sent("locate_project")).toEqual([{ gone: GONE.path, picked: FOUND }]);
    // Through `open_plane`, the trust gate: the approval did not travel with the path.
    expect(sent("open_plane").map((one) => one.path)).toContain(FOUND);
    expect(screen.queryByText(GONE.said)).not.toBeInTheDocument();
  });

  it("says why the core refused a folder, and keeps the Notice", async () => {
    const refused = "~/Downloads is not a project: charter found none there or above it";
    const { sent } = core({ where: "restore", picked: "/home/dev/Downloads", locate: { refused } });
    render(<App />);

    await userEvent.click(within(await gone()).getByRole("button", { name: "Locate…" }));

    expect(await screen.findByText(refused)).toBeInTheDocument();
    expect(within(await gone()).getByRole("button", { name: "Locate…" })).toBeInTheDocument();
    expect(sent("open_plane").map((one) => one.path)).not.toContain("/home/dev/Downloads");
  });

  it("does nothing when the picker is cancelled", async () => {
    const { sent } = core({ where: "restore", picked: null });
    render(<App />);

    await userEvent.click(within(await gone()).getByRole("button", { name: "Locate…" }));

    await waitFor(() => expect(sent("pick_project")).toHaveLength(1));
    expect(sent("locate_project")).toEqual([]);
    expect(await gone()).toBeInTheDocument();
  });
});

describe("a recent that is gone, on the opener", () => {
  it("offers Locate… and Forget, and Forget takes it off the list", async () => {
    const { sent } = core({ where: "recent" });
    render(<App />);

    const notice = await gone();
    expect(within(notice).getByRole("button", { name: "Locate…" })).toBeInTheDocument();
    await userEvent.click(within(notice).getByRole("button", { name: "Forget" }));

    await waitFor(() => expect(screen.queryByText(GONE.said)).not.toBeInTheDocument());
    expect(sent("forget_project")).toEqual([{ path: GONE.path }]);
    expect(screen.getByRole("button", { name: /two/ })).toBeInTheDocument();
  });

  it("re-points it at a picked folder and opens the project found there", async () => {
    const { sent } = core({ where: "recent" });
    render(<App />);

    await userEvent.click(within(await gone()).getByRole("button", { name: "Locate…" }));

    await waitFor(() => expect(sent("open_plane").map((one) => one.path)).toEqual([FOUND]));
    expect(sent("locate_project")).toEqual([{ gone: GONE.path, picked: FOUND }]);
  });
});

describe("a gone project both the restore and the opener name", () => {
  // Nothing came back, so the window draws the opener, and both lists hold the same project.
  // The opener owns it: one Notice, and settling it leaves no copy behind anywhere.
  const copies = () => screen.queryAllByText(GONE.said);

  it("is drawn once", async () => {
    core({ where: "both" });
    render(<App />);

    await screen.findByRole("heading", { level: 1 });
    await waitFor(() => expect(copies()).toHaveLength(1));
    expect(within(await gone()).getByRole("button", { name: "Locate…" })).toBeInTheDocument();
  });

  it("leaves nothing stale once it is forgotten", async () => {
    const { sent } = core({ where: "both" });
    render(<App />);
    await waitFor(() => expect(copies()).toHaveLength(1));

    await userEvent.click(within(await gone()).getByRole("button", { name: "Forget" }));

    await waitFor(() => expect(sent("recent_planes").length).toBeGreaterThan(1));
    await waitFor(() => expect(copies()).toHaveLength(0));
    expect(sent("forget_project")).toEqual([{ path: GONE.path }]);
  });

  it("leaves nothing stale once it is located and open", async () => {
    core({ where: "both" });
    render(<App />);
    await waitFor(() => expect(copies()).toHaveLength(1));

    await userEvent.click(within(await gone()).getByRole("button", { name: "Locate…" }));

    await waitFor(() => expect(projectTabs()).toEqual(["moved"]));
    expect(copies()).toHaveLength(0);
  });
});
