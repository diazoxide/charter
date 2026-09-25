import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SavingView } from "./SavingView";
import type { PlaneSaving, RepoSaving, SaveEntry } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function standing(over: Partial<PlaneSaving> = {}): PlaneSaving {
  return {
    stage: "saved",
    changed: [],
    ahead: 0,
    pr: null,
    blocked: null,
    branch: "main",
    pushes: true,
    behind: 0,
    pushFailed: null,
    live: [],
    conflicts: [],
    notice: null,
    mode: "push",
    modeFrom: "charter.toml",
    journal: [],
    ...over,
  };
}

function entry(over: Partial<SaveEntry> = {}): SaveEntry {
  return {
    at: 1_790_000_000,
    target: "plane",
    trigger: "manual",
    mode: "push",
    files: 1,
    commit: "1f488c66767d3706b46bcef51ce4921fa981a2e4",
    pr: null,
    outcome: "saved",
    detail: "",
    ...over,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** The modes the view wrote, in order. */
const chosen: string[] = [];

/** The core: `plane_saving` answers each of `reads` in turn (the last one again after that),
 *  and `save_plane` answers `saved` — lines, or an `Error` whose message is the refusal. */
function core(reads: PlaneSaving[], saved: string[] | Error = ["✓ Committed"]): Asked[] {
  const asked: Asked[] = [];
  let read = 0;
  chosen.length = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    if (cmd === "plane_saving") return reads[Math.min(read++, reads.length - 1)];
    if (cmd === "choose_plane_mode") {
      chosen.push((args as { mode: string }).mode);
      return standing({ mode: (args as { mode: string }).mode });
    }
    if (cmd === "save_plane") {
      if (saved instanceof Error) throw saved.message;
      return saved;
    }
    return null;
  });
  return asked;
}

describe("SavingView", () => {
  it("says how much is unsaved and lists what the next save takes", async () => {
    core([standing({ stage: "changed", changed: ["notes/a.md", "todos/b.json"] })]);
    render(<SavingView plane={PLANE} />);

    expect(await screen.findByText("2 changed")).toBeTruthy();
    const files = screen.getByRole("list", { name: "What the next save takes" });
    expect(
      within(files)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["notes/a.md", "todos/b.json"]);
    expect(screen.getByText("Mode: push (charter.toml) — a save pushes to main")).toBeTruthy();
  });

  it("saves with the message typed, then reads the plane again", async () => {
    const asked = core([
      standing({ stage: "changed", changed: ["a.md"] }),
      standing({ journal: [entry()] }),
    ]);
    const onSaved = vi.fn();
    render(<SavingView plane={PLANE} onSaved={onSaved} />);
    await screen.findByText("1 changed");

    await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "by hand");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Saved")).toBeTruthy();
    expect(asked.find((a) => a.cmd === "save_plane")?.args).toEqual({
      plane: PLANE,
      message: "by hand",
    });
    expect(onSaved).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status").textContent).toContain("✓ Committed");
  });

  it("asks for the generated message when none is typed", async () => {
    const asked = core([standing({ stage: "changed", changed: ["a.md"] })]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("1 changed");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(asked.some((a) => a.cmd === "save_plane")).toBe(true));
    expect(asked.find((a) => a.cmd === "save_plane")?.args.message).toBeNull();
  });

  it("shows a refusal in the core's words", async () => {
    core(
      [standing({ stage: "changed", changed: ["a.md"] })],
      new Error("Refusing to save — a secret"),
    );
    render(<SavingView plane={PLANE} />);
    await screen.findByText("1 changed");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect((await screen.findByRole("alert")).textContent).toContain("Refusing to save — a secret");
  });

  it("names why a blocked plane is blocked, and a plane with nothing to save offers no save", async () => {
    core([standing({ stage: "blocked", blocked: "CONFLICT in a.md" })]);
    render(<SavingView plane={PLANE} />);
    expect(await screen.findByText("Blocked: CONFLICT in a.md")).toBeTruthy();
    cleanup();

    core([standing()]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("Saved");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("says what a plane with no mode will do, and where a save that cannot push stops", async () => {
    core([standing({ mode: null, modeFrom: "default" })]);
    render(<SavingView plane={PLANE} />);
    expect(await screen.findByText("Mode: not set — a save pushes to main")).toBeTruthy();
    cleanup();

    core([standing({ mode: "commit", pushes: false })]);
    render(<SavingView plane={PLANE} />);
    expect(
      await screen.findByText("Mode: commit (charter.toml) — a save commits and goes no further"),
    ).toBeTruthy();
  });

  it("says a PR mode pushes to its save branch and keeps a pull request open, and links it", async () => {
    core([
      standing({
        mode: "pr",
        stage: "pr-open",
        branch: "release",
        pr: "https://github.com/acme/plane/pull/12",
      }),
    ]);
    render(<SavingView plane={PLANE} />);
    expect(await screen.findByText("Pushed — waiting on its pull request")).toBeTruthy();
    expect(
      screen.getByText(
        "Mode: pr (charter.toml) — a save pushes to this machine's save branch and keeps a pull request open into release",
      ),
    ).toBeTruthy();
    expect(
      screen
        .getByRole("link", { name: "https://github.com/acme/plane/pull/12" })
        .getAttribute("href"),
    ).toBe("https://github.com/acme/plane/pull/12");
    cleanup();

    core([standing({ mode: "pr-merge", branch: "release" })]);
    render(<SavingView plane={PLANE} />);
    expect(
      await screen.findByText(
        "Mode: pr-merge (charter.toml) — a save pushes to this machine's save branch and keeps a pull request open into release, set to merge when its checks pass",
      ),
    ).toBeTruthy();
  });

  it("offers a save for commits only when a save would push them", async () => {
    core([standing({ stage: "committed", ahead: 2 })]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("2 committed, not pushed");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(
      false,
    );
    cleanup();

    core([standing({ stage: "committed", ahead: 2, pushes: false })]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("2 committed, not pushed");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("lists the last saves, newest first, each with how it ended", async () => {
    core([
      standing({
        journal: [
          entry({ outcome: "blocked", detail: "a secret-shaped value", files: 2, commit: null }),
          entry({ outcome: "saved", trigger: "cli" }),
        ],
      }),
    ]);
    render(<SavingView plane={PLANE} />);

    const rows = within(await screen.findByRole("list", { name: "Recent saves" })).getAllByRole(
      "listitem",
    );
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("blocked");
    expect(rows[0].textContent).toContain("a secret-shaped value");
    expect(rows[1].textContent).toContain("saved");
    expect(rows[1].textContent).toContain("cli");
    expect(rows[1].textContent).toContain("1f488c6");
  });

  it("asks a project nobody has chosen a mode for how it is saved, once, and writes the answer", async () => {
    core([standing({ mode: null, modeFrom: "default" }), standing({ mode: "commit" })]);
    render(<SavingView plane={PLANE} />);

    const question = await screen.findByRole("group", {
      name: "How should this project be saved?",
    });
    await userEvent.click(within(question).getByRole("button", { name: /^Commit only/ }));

    await waitFor(() => expect(chosen).toEqual(["commit"]));
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "How should this project be saved?" })).toBeNull(),
    );
  });

  it("says how many commits came in and were not pulled", async () => {
    core([standing({ behind: 2 })]);
    render(<SavingView plane={PLANE} />);
    expect(await screen.findByText("Saved · 2 incoming")).toBeTruthy();
  });

  it("says why the last push did not land, without calling the plane blocked", async () => {
    core([
      standing({ stage: "committed", ahead: 1, pushFailed: "Could not resolve host: github.com" }),
    ]);
    render(<SavingView plane={PLANE} />);
    expect(
      await screen.findByText("The last push did not land: Could not resolve host: github.com"),
    ).toBeTruthy();
    expect(screen.getByText("1 committed, not pushed")).toBeTruthy();
  });

  it("offers a blocked save's two ways out, and names where it conflicts", async () => {
    core([
      standing({
        stage: "blocked",
        blocked: "the remote changed the same lines in: notes.md",
        conflicts: ["notes.md"],
      }),
    ]);
    const asked: { plane: string; way: string }[] = [];
    const listen = (event: Event) => asked.push((event as CustomEvent).detail);
    window.addEventListener("charter-saving-way-out", listen);
    render(<SavingView plane={PLANE} />);

    const ways = await screen.findByRole("group", { name: "Ways out" });
    expect(
      within(within(ways).getByRole("list", { name: "Where it conflicts" }))
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["notes.md"]);
    await userEvent.click(within(ways).getByRole("button", { name: "Resolve in a chat" }));
    await userEvent.click(within(ways).getByRole("button", { name: "Open terminal here" }));
    window.removeEventListener("charter-saving-way-out", listen);

    expect(asked).toEqual([
      { plane: PLANE, way: "chat" },
      { plane: PLANE, way: "terminal" },
    ]);
  });
});

function repo(over: Partial<RepoSaving> = {}): RepoSaving {
  return {
    name: "widget",
    mode: "pr",
    modeFrom: "default",
    autosave: false,
    stage: "saved",
    branch: "main",
    changed: 0,
    ahead: 0,
    pr: null,
    blocked: null,
    pushes: true,
    ...over,
  };
}

/** The core with a workspace `alpha`: `workspace_saving` answers each of `reads` in turn, and
 *  `save_repo` answers from `saved` by repo name — lines, or an `Error` for a refusal. */
function coreWithRepos(
  plane: PlaneSaving,
  reads: RepoSaving[][],
  saved: Record<string, string[] | Error> = {},
): Asked[] {
  const asked: Asked[] = [];
  let read = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    if (cmd === "plane_saving") return plane;
    if (cmd === "workspace_saving") return reads[Math.min(read++, reads.length - 1)];
    if (cmd === "save_plane") return ["✓ Committed the plane"];
    if (cmd === "save_repo") {
      const said = saved[(args as { name: string }).name] ?? ["✓ Committed"];
      if (said instanceof Error) throw said.message;
      return said;
    }
    return null;
  });
  return asked;
}

describe("SavingView, with the workspace's repos (charter-app#299)", () => {
  it("draws one row per repo, each with its stage, branch and pull request", async () => {
    coreWithRepos(standing(), [
      [
        repo({ name: "api", stage: "changed", changed: 3, branch: "feature/x" }),
        repo({
          name: "web",
          stage: "pr-open",
          branch: "main",
          pr: "https://github.com/acme/web/pull/7",
        }),
        repo({ name: "docs", stage: "off", mode: "off", modeFrom: "charter.toml", changed: 2 }),
      ],
    ]);
    render(<SavingView plane={PLANE} workspace="alpha" />);

    const table = await screen.findByRole("table", { name: "Repos in alpha" });
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows.map((row) => row.getAttribute("data-repo"))).toEqual(["api", "web", "docs"]);
    expect(rows[0].textContent).toContain("feature/x");
    expect(rows[0].textContent).toContain("3 changed");
    expect(
      within(rows[1]).getByRole("link", { name: "https://github.com/acme/web/pull/7" }),
    ).toBeTruthy();
    expect(rows[1].textContent).toContain("waiting on its pull request");
    expect(rows[2].textContent).toContain("Off — charter does not save this repo");
    expect(
      (within(rows[2]).getByRole("button", { name: "Save docs" }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it("saves one repo from its row, and says in the core's words whom a held-back save waits for", async () => {
    const asked = coreWithRepos(standing(), [[repo({ stage: "changed", changed: 1 })]], {
      widget: new Error("Not saved: alpha.2 is mid-turn in alpha."),
    });
    render(<SavingView plane={PLANE} workspace="alpha" />);

    await userEvent.click(await screen.findByRole("button", { name: "Save widget" }));

    expect((await screen.findByRole("alert")).textContent).toContain(
      "Not saved: alpha.2 is mid-turn in alpha.",
    );
    expect(asked.find((a) => a.cmd === "save_repo")?.args).toEqual({
      plane: PLANE,
      workspace: "alpha",
      name: "widget",
      message: null,
    });
  });

  it("saves the plane and every repo with something to save, from Save all", async () => {
    const asked = coreWithRepos(standing({ stage: "changed", changed: ["a.md"] }), [
      [
        repo({ name: "api", stage: "changed", changed: 1 }),
        repo({ name: "web", stage: "saved" }),
        repo({ name: "cli", stage: "committed", ahead: 1 }),
      ],
    ]);
    render(<SavingView plane={PLANE} workspace="alpha" />);

    await userEvent.click(await screen.findByRole("button", { name: "Save all" }));

    await waitFor(() =>
      expect(
        asked
          .filter((a) => a.cmd === "save_plane" || a.cmd === "save_repo")
          .map((a) => (a.cmd === "save_plane" ? "plane" : a.args.name)),
      ).toEqual(["plane", "api", "cli"]),
    );
    expect((await screen.findByRole("status")).textContent).toContain("✓ Committed the plane");
  });
});
