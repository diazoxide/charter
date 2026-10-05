import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { forgetRepoClones } from "./repoClones";
import { forgetThisLaunch } from "./regions";

/**
 * **A repo the workspace names and this machine has not cloned, cloned from the window**
 * (#1215).
 *
 * Against the whole app and a fake core, because the claim is about the path from a row the
 * operator can see — the explorer's, the bottom bar's, the palette's — to `clone_repo`, and
 * back to the row moving among the clones once the core says the clone is there. The clone
 * itself is the core's (`clone_repo`, the same call Settings › Repos makes); a failure is
 * drawn in its words.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

/**
 * The core: alpha has `svc` cloned and names `missing` without a clone. A clone that works
 * moves the repo from one list to the other, which is what the next `workspace_panels` reads.
 * `fails` answers that repo's next N clones with the core's sentence.
 */
function core(
  over: {
    missing?: string[];
    fails?: Record<string, { said: string; times: number }>;
    held?: Promise<void>;
    /** Holds only that repo's clone until the promise settles. */
    holds?: Record<string, Promise<void>>;
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const cloned = ["svc"];
  const missing = [...(over.missing ?? ["charter", "web"])];
  const fails = { ...over.fails };
  let inFlight = 0;
  let peak = 0;
  mockIPC(async (cmd, args) => {
    const a = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: a });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "plane_pins") return { project: false, workspaces: ["alpha"], missing: [] };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [] }],
      };
    if (cmd === "workspace_panels")
      return {
        workspace: a.workspace,
        repos: [...cloned],
        paths: Object.fromEntries(cloned.map((repo) => [repo, `${ALPHA}/${repo}`])),
        absent: [...missing],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
        sessions: [],
        contributed: [],
      };
    if (cmd === "workspace_repos")
      return { workspace: a.workspace, repos: [], cache_refused: null };
    if (cmd === "worktree_list") return [];
    if (cmd === "take_repos") return null;
    if (cmd === "clone_repo") {
      const repo = String(a.repo);
      inFlight += 1;
      peak = Math.max(peak, inFlight);
      try {
        if (over.held) await over.held;
        await over.holds?.[repo];
      } finally {
        inFlight -= 1;
      }
      const failing = fails[repo];
      if (failing && failing.times > 0) {
        failing.times -= 1;
        throw failing.said;
      }
      missing.splice(missing.indexOf(repo), 1);
      cloned.push(repo);
      return [`✓ ${repo} → workspaces/alpha/${repo}`];
    }
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    return null;
  });
  return {
    asked,
    clones: () => asked.filter((one) => one.cmd === "clone_repo"),
    /** The most `clone_repo` calls the core was running at once. */
    peak: () => peak,
  };
}

beforeEach(() => {
  globalThis.localStorage.clear();
  forgetThisLaunch();
});
afterEach(() => {
  cleanup();
  clearMocks();
  forgetRepoClones();
});

const absent = async () => await screen.findByTestId("absent");
const absentRow = (repo: string) => screen.getByTestId(`absent-${repo}`);

describe("a repo that is not cloned here", () => {
  it("has a Clone button on its row, which clones it where the workspace says and moves it to the clones", async () => {
    const { clones } = core();
    render(<App />);

    await userEvent.click(within(await absent()).getByRole("button", { name: "Clone charter" }));

    expect(await screen.findByTestId("clone-charter")).toBeInTheDocument();
    expect(within(await absent()).queryByText("charter")).toBeNull();
    expect(clones().map((one) => one.args)).toEqual([
      { plane: PLANE, workspace: "alpha", repo: "charter" },
    ]);
  });

  it("says the clone is under way while it runs", async () => {
    let release = () => {};
    const held = new Promise<void>((done) => (release = done));
    core({ held });
    render(<App />);

    await userEvent.click(within(await absent()).getByRole("button", { name: "Clone charter" }));

    expect(await within(absentRow("charter")).findByText("Cloning charter…")).toBeInTheDocument();
    expect(
      within(absentRow("charter")).queryByRole("button", { name: "Clone charter" }),
    ).toBeNull();
    release();
    expect(await screen.findByTestId("clone-charter")).toBeInTheDocument();
  });

  it("shows the core's reason when it fails, and Retry clones it again", async () => {
    const said = "charter: clone failed — no access, network, or gh isn't authed.";
    const { clones } = core({ fails: { charter: { said, times: 1 } } });
    render(<App />);

    await userEvent.click(within(await absent()).getByRole("button", { name: "Clone charter" }));

    expect(await within(absentRow("charter")).findByRole("alert")).toHaveTextContent(said);
    await userEvent.click(
      within(absentRow("charter")).getByRole("button", { name: "Retry charter" }),
    );

    expect(await screen.findByTestId("clone-charter")).toBeInTheDocument();
    expect(clones().map((one) => one.args.repo)).toEqual(["charter", "charter"]);
  });

  it("is offered on the row's own menu", async () => {
    const { clones } = core();
    render(<App />);
    await absent();

    fireEvent.contextMenu(within(absentRow("web")).getByText("web"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Clone web" }));

    expect(await screen.findByTestId("clone-web")).toBeInTheDocument();
    expect(clones().map((one) => one.args.repo)).toEqual(["web"]);
  });

  it("is offered on the bottom bar's row, as a menu, so the bar stays unpressable", async () => {
    const { clones } = core();
    render(<App />);
    const bar = await screen.findByTestId("bottom-bar");
    const row = await within(bar).findByTestId("repo-web");

    fireEvent.contextMenu(within(row).getByText("web"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Clone web" }));

    await waitFor(() => expect(clones().map((one) => one.args.repo)).toEqual(["web"]));
    expect(bar.querySelectorAll("button, input, textarea, select, a[href]")).toHaveLength(0);
  });
});

describe("every repo that is not cloned here", () => {
  it("has Clone all on the heading, which clones each in turn", async () => {
    const { clones } = core();
    render(<App />);

    await userEvent.click(within(await absent()).getByRole("button", { name: "Clone all" }));

    expect(await screen.findByTestId("clone-web")).toBeInTheDocument();
    expect(await screen.findByTestId("clone-charter")).toBeInTheDocument();
    expect(clones().map((one) => one.args.repo)).toEqual(["charter", "web"]);
    await waitFor(() => expect(screen.queryByTestId("absent")).toBeNull());
  });

  it("moves each repo to the clones as it lands, while the rest are still cloning", async () => {
    let release = () => {};
    const web = new Promise<void>((done) => (release = done));
    const { clones } = core({ holds: { web } });
    render(<App />);

    await userEvent.click(within(await absent()).getByRole("button", { name: "Clone all" }));

    expect(await screen.findByTestId("clone-charter")).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByTestId("absent-charter")).toBeNull());
    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Clone");
    const titles = screen
      .getAllByRole("option")
      .map((row) => row.querySelector(".palette-title")?.textContent);
    expect(titles).not.toContain("Clone charter");
    await userEvent.keyboard("{Escape}");

    release();
    expect(await screen.findByTestId("clone-web")).toBeInTheDocument();
    expect(clones().map((one) => one.args.repo)).toEqual(["charter", "web"]);
  });

  it("runs two rows' clones one after the other, never at once", async () => {
    let release = () => {};
    const charter = new Promise<void>((done) => (release = done));
    const { clones, peak } = core({ holds: { charter } });
    render(<App />);

    await userEvent.click(within(await absent()).getByRole("button", { name: "Clone charter" }));
    await within(absentRow("charter")).findByText("Cloning charter…");
    await userEvent.click(within(absentRow("web")).getByRole("button", { name: "Clone web" }));

    expect(
      await within(absentRow("web")).findByText("web is waiting to be cloned."),
    ).toBeInTheDocument();
    expect(clones().map((one) => one.args.repo)).toEqual(["charter"]);
    release();

    expect(await screen.findByTestId("clone-web")).toBeInTheDocument();
    expect(await screen.findByTestId("clone-charter")).toBeInTheDocument();
    expect(clones().map((one) => one.args.repo)).toEqual(["charter", "web"]);
    expect(peak()).toBe(1);
  });

  it("offers no Clone all when only one repo is missing", async () => {
    core({ missing: ["charter"] });
    render(<App />);

    expect(within(await absent()).queryByRole("button", { name: "Clone all" })).toBeNull();
    expect(
      within(await absent()).getByRole("button", { name: "Clone charter" }),
    ).toBeInTheDocument();
  });

  it("is two rows of the palette: one repo, and all of them", async () => {
    const { clones } = core();
    render(<App />);
    await absent();

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Clone");
    const titles = screen
      .getAllByRole("option")
      .map((row) => row.querySelector(".palette-title")?.textContent);
    expect(titles).toEqual(
      expect.arrayContaining(["Clone charter", "Clone web", "Clone all missing repos"]),
    );

    const all = screen
      .getAllByRole("option")
      .find(
        (row) => row.querySelector(".palette-title")?.textContent === "Clone all missing repos",
      );
    if (all === undefined) throw new Error("no Clone all row");
    await userEvent.click(all);
    await waitFor(() => expect(clones().map((one) => one.args.repo)).toEqual(["charter", "web"]));
  });
});
