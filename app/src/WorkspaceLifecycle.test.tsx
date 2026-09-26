import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
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

/**
 * Making a workspace and deleting one, from the window.
 *
 * **The delete is what these tests are for.** `charter workspace remove` refuses over work
 * that removing the workspace would discard (`wscmd::work_at_risk`), and that guard runs
 * inside the command. So what has to be true of the window is narrow and checkable:
 *
 * - the only thing it ever calls is `workspace_remove`;
 * - the first press passes `force: false`, always;
 * - a refusal is shown in the core's own words, and **nothing is deleted**;
 * - forcing is a second press on a button that did not exist before the refusal, and it names
 *   what it is about to discard.
 *
 * Against the whole app rather than the dialog alone, because the claim is about the path from
 * a right-click on a strip to an IPC call, and a test of the dialog in isolation would leave
 * every step of that path unasserted.
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
  forgetRepoClones();
});

const PLANE = "/home/dev/plane";

/** One thing the guard found, as `wscmd::AtRisk` crosses the wire. */
type Risk = { what: string; said: string };

/** `wscmd::remove`'s refusal, built from a list exactly as the core builds it. */
const refusalOver = (risky: readonly Risk[]) =>
  `Refusing to remove 'alpha' — this would discard work: ${risky
    .map((risk) => risk.said)
    .join("; ")}. Push/commit first, or pass --force.`;

/** The refusal the core gives when a clone in the workspace has uncommitted work. */
const REFUSED = refusalOver([{ what: "svc", said: "svc: uncommitted changes" }]);

/**
 * The core, with a plane of two workspaces and a workspace verb that answers as charter does.
 *
 * `remove` refuses unless it is forced, exactly as `wscmd::remove` refuses: the point of these
 * tests is what the WINDOW does with that refusal, so the mock has to give one.
 *
 * **It refuses with a `Refused`, which is the sentence AND the list it refused on**
 * (charter-app#182). `refusesOver` is what the guard finds at the moment of the delete, and it
 * defaults to the preview's list because usually nothing moved — the test that matters is the
 * one where it did.
 */
function core(
  over: {
    atRisk?: Risk[];
    refuses?: boolean;
    refusesOver?: Risk[];
    /** The plane's workspaces; none is a plane nobody has made one in yet. */
    workspaces?: string[];
    /** A forge the operator is not logged in to, as CI's machines are. */
    loggedOut?: boolean;
    /** Repos whose clone fails, with the core's sentence. */
    cloneFails?: Record<string, string>;
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const atRisk = over.atRisk ?? [{ what: "svc", said: "svc: uncommitted changes" }];
  const refuses = over.refuses ?? true;
  const refusesOver = over.refusesOver ?? atRisk;
  const gone: string[] = [];
  const made: string[] = [];
  mockIPC((cmd, args) => {
    const got = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: got });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    // beta is pinned, so it is on the strip beside alpha, the workspace you are in: the strip
    // draws what is pinned and the one you are in (ADR 0054). alpha is left unpinned so its
    // own menu offers to pin it.
    if (cmd === "plane_pins")
      return {
        project: false,
        workspaces: ["beta"].filter((name) => !gone.includes(name)),
        missing: [],
      };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
        workspaces: [...(over.workspaces ?? ["alpha", "beta"]), ...made]
          .filter((name) => !gone.includes(name))
          .map((name) => ({
            name,
            path: `${PLANE}/workspaces/${name}`,
            vision: "",
            todos: [],
            chats: [],
          })),
      };
    if (cmd === "workspace_at_risk") return atRisk;
    if (cmd === "project_extensions") return { extensions: [], local_left_out: null };
    if (cmd === "extensions_on") return [];
    if (cmd === "workspace_settings")
      return {
        workspace: got.workspace,
        file: `workspaces/${String(got.workspace)}/workspace.json`,
        exists: true,
        text: "{}\n",
        refusals: [],
        parsed: true,
        fields: [],
        live: false,
      };
    if (cmd === "workspace_remove") {
      if (refuses && got.force !== true)
        throw { said: refusalOver(refusesOver), at_risk: refusesOver };
      gone.push(String(got.workspace));
      return [`✓ Removed workspace '${String(got.workspace)}' and its clones.`];
    }
    if (cmd === "reachable_repos")
      return {
        repos: [
          { name: "api", path: "acme/api", description: "" },
          { name: "web", path: "acme/web", description: "" },
        ],
        trouble: over.loggedOut
          ? ["gh is not authenticated for github.com. Run: gh auth login"]
          : [],
      };
    if (cmd === "take_repos") return null;
    if (cmd === "workspace_repos")
      return { workspace: got.workspace, repos: [{ name: "api" }], cache_refused: null };
    if (cmd === "drop_repo")
      throw {
        said: "Refusing to remove 'api' — this would discard work: api: uncommitted changes. Push or commit first.",
        at_risk: [{ what: "api", said: "api: uncommitted changes" }],
      };
    if (cmd === "clone_repo") {
      const fails = over.cloneFails?.[String(got.repo)];
      if (fails !== undefined) throw fails;
      return [`✓ ${String(got.repo)} cloned`];
    }
    if (cmd === "workspace_rename") {
      const name = String(got.name);
      if (name === "beta")
        throw "workspace 'beta' already exists — pick another name or remove it first.";
      gone.push(String(got.workspace));
      made.push(name);
      return [`✓ Renamed workspace '${String(got.workspace)}' to '${name}'.`];
    }
    if (cmd === "workspace_create") {
      const name = String(got.name);
      if (name.includes("/"))
        throw `invalid workspace name '${name}' (use letters, digits, '.', '_', '-'; must not start with a dot)`;
      return [`✓ Workspace '${name}' ready (LOCAL) → workspaces/${name}/`];
    }
    return null;
  });
  return { asked, calls: (cmd: string) => asked.filter((one) => one.cmd === cmd) };
}

/** The workspaces, as the strip lists them. */
const strip = () =>
  within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .map((tab) => tab.querySelector(".workspace-name")?.textContent);

/** Right-clicks a workspace tab and waits for charter's own menu. */
async function menuOn(workspace: string) {
  const tab = within(screen.getByRole("tablist", { name: "Workspaces" }))
    .getAllByRole("tab")
    .find((one) => one.querySelector(".workspace-name")?.textContent === workspace);
  if (!tab) throw new Error(`no ${workspace} on the strip; it lists ${strip().join(", ")}`);
  fireEvent.contextMenu(tab);
  return await screen.findByRole("menu");
}

/** Opens the delete dialog for one workspace, through the menu an operator would use. */
async function askToDelete(workspace: string) {
  await menuOn(workspace);
  await userEvent.click(screen.getByRole("menuitem", { name: `Delete workspace ${workspace}` }));
  return await screen.findByRole("alertdialog");
}

async function settled() {
  // beta pinned, then alpha, the workspace you are in (ADR 0054).
  await waitFor(() => expect(strip()).toEqual(["beta", "alpha"]));
}

describe("deleting a workspace", () => {
  it("is reached from the workspace tab's own menu, which the operator asked for", async () => {
    core();
    render(<App />);
    await settled();

    const menu = await menuOn("alpha");

    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((row) => row.textContent?.trim()),
    ).toEqual([
      "Focus workspace alpha",
      expect.stringContaining("Pin workspace alpha"),
      expect.stringContaining("Workspace settings…"),
      expect.stringContaining("Make alpha live…"),
      expect.stringContaining("Rename workspace alpha…"),
      "New workspace…",
      expect.stringContaining("Delete workspace alpha"),
    ]);
  });

  it("shows what charter would discard before anything is pressed", async () => {
    // The preview is `workspace_at_risk`, which is the core's own guard read for drawing. The
    // sentences are charter's, not the window's.
    const { calls } = core();
    render(<App />);
    await settled();

    await askToDelete("alpha");

    expect(calls("workspace_at_risk")[0].args).toMatchObject({
      plane: PLANE,
      workspace: "alpha",
    });
    expect(await screen.findByText("svc: uncommitted changes")).toBeInTheDocument();
  });

  it("never forces on the first press, and offers no way to", async () => {
    const { calls } = core();
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");
    await screen.findByText("svc: uncommitted changes");

    // The only answer on screen is the one that does not force.
    expect(
      within(dialog)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Delete workspace", "Cancel"]);

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete workspace" }));

    expect(calls("workspace_remove").map((one) => one.args)).toEqual([
      { plane: PLANE, workspace: "alpha", force: false },
    ]);
  });

  it("shows the core's refusal word for word, and deletes nothing", async () => {
    const { calls } = core();
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");
    await screen.findByText("svc: uncommitted changes");

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete workspace" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(REFUSED);
    // One call, and it was the one that asked the core to refuse.
    expect(calls("workspace_remove")).toHaveLength(1);
    // The strip is only reachable once the dialog is answered: a Radix alert dialog marks
    // the rest of the window `aria-hidden`, which is the surface behaving correctly
    // (`docs/ui-primitives.md`). Cancelled, the workspace is still there.
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(strip()).toEqual(["beta", "alpha"]);
  });

  it("only then offers to force, and the button names what it discards", async () => {
    const { calls } = core();
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");
    await screen.findByText("svc: uncommitted changes");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete workspace" }));
    await within(dialog).findByRole("alert");

    const force = within(dialog).getByRole("button", {
      name: "Delete it anyway, discarding the work in svc",
    });
    // And the answer that does not force has gone: a dialog offering both is offering to
    // force to somebody who has read nothing.
    expect(within(dialog).queryByRole("button", { name: "Delete workspace" })).toBeNull();

    await userEvent.click(force);

    expect(calls("workspace_remove").map((one) => one.args.force)).toEqual([false, true]);
    await vi.waitFor(() => expect(strip()).toEqual(["beta"]));
  });

  /**
   * **charter-app#182: the force button's words and the refusal's words are one reading.**
   *
   * The workspace moves while the dialog is up — `lib` goes dirty after the preview and before
   * the press — so the core refuses over two clones where the preview saw one. Before this, the
   * button was drawn from the preview: it offered to discard *the work in svc* directly beneath
   * a sentence saying the delete would discard svc **and** lib. Two descriptions of one act
   * that do not agree, at the moment somebody is deciding whether to throw work away.
   *
   * Nothing was ever wrongly deleted, and this test does not claim otherwise: the core's own
   * read decides and always did. What is asserted is that the surface asking for consent
   * describes the state the refusal was made against.
   */
  it("names what the core refused on, not what the preview saw, when they differ", async () => {
    const dirtied = [
      { what: "svc", said: "svc: uncommitted changes" },
      { what: "lib", said: "lib: 2 unpushed commit(s)" },
    ];
    const { calls } = core({
      atRisk: [{ what: "svc", said: "svc: uncommitted changes" }],
      refusesOver: dirtied,
    });
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");
    // The preview, which is the older reading and is drawn as one.
    await screen.findByText("svc: uncommitted changes");
    expect(within(dialog).queryByText("lib: 2 unpushed commit(s)")).toBeNull();

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete workspace" }));

    // The refusal, verbatim, and the list under "What charter would discard" is now ITS list.
    const refusal = await within(dialog).findByRole("alert");
    expect(refusal).toHaveTextContent(refusalOver(dirtied));
    expect(
      within(within(dialog).getByTestId("at-risk"))
        .getAllByRole("listitem")
        .map((row) => row.textContent),
    ).toEqual(["svc: uncommitted changes", "lib: 2 unpushed commit(s)"]);
    // And the button names the same two, in the same order, from the same list.
    within(dialog).getByRole("button", {
      name: "Delete it anyway, discarding the work in 2: svc, lib",
    });
    // The preview's own line about the earlier reading is gone with it: a dialog cannot say
    // "charter found no uncommitted work" above a refusal listing two dirty clones.
    expect(calls("workspace_at_risk")).toHaveLength(1);
  });

  /**
   * **A refusal `--force` cannot get past is not offered a force button.**
   *
   * The guard refuses only when it found something, so a refusal carrying nothing is one of
   * the core's other exits — a name that is not a workspace, a `workspaces/<ws>` that links out
   * of the plane, a `remove_dir_all` that failed. `--force` reaches none of them, and a button
   * saying "Delete it anyway" beside one would be a promise the next press cannot keep. This
   * shape only became tellable when the refusal started carrying its list (#182).
   */
  it("offers no way to force past a refusal that forcing cannot reach", async () => {
    core({ refusesOver: [] });
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");
    await screen.findByText("svc: uncommitted changes");

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete workspace" }));
    await within(dialog).findByRole("alert");

    expect(
      within(dialog)
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["Delete workspace", "Cancel"]);
  });

  it("deletes nothing when the dialog is cancelled", async () => {
    const { calls } = core();
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");

    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(calls("workspace_remove")).toEqual([]);
    expect(strip()).toEqual(["beta", "alpha"]);
  });

  it("goes through workspace_remove and never through anything else", async () => {
    // The guard is INSIDE `wscmd::remove`. A window that reached a lower-level call would be
    // past it, which is the worst defect available here — so the whole IPC transcript of a
    // delete is asserted, not just the call that was expected.
    const { asked } = core({ refuses: false });
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");
    const before = asked.length;
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete workspace" }));
    await vi.waitFor(() => expect(strip()).toEqual(["beta"]));

    // Everything else the delete sets off is the window reading the plane again: the sidebar,
    // this operator's pins, and the two the three regions share for whatever workspace the
    // window lands on afterwards — and what that workspace has on (charter-app#280). None of
    // them writes anything.
    const READS = [
      "extensions_on",
      // The workspace in front changed, and its theme is a layer of what the window draws
      // (charter-app#281).
      "project_theme_drawn",
      // And so are the badges and repo columns its extensions show (charter-app#340).
      "extension_facts",
      "plane_sidebar",
      "plane_pins",
      "workspace_panels",
      "workspace_repos",
      // The title bar's save indicator counts the workspace in front's repos in
      // (charter-app#299): a read of git and the save journal.
      "workspace_saving",
      "alerts_everywhere",
      "window_holds_planes",
    ];
    const during = asked.slice(before).map((one) => one.cmd);
    expect(during.filter((cmd) => !READS.includes(cmd))).toEqual(["workspace_remove"]);
  });

  it("says what the core said when it worked", async () => {
    core({ refuses: false });
    render(<App />);
    await settled();
    const dialog = await askToDelete("alpha");

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete workspace" }));

    expect(
      await screen.findByText("✓ Removed workspace 'alpha' and its clones."),
    ).toBeInTheDocument();
  });
});

describe("making a workspace", () => {
  async function askToCreate() {
    await menuOn("alpha");
    await userEvent.click(screen.getByRole("menuitem", { name: "New workspace…" }));
    return await screen.findByRole("dialog");
  }

  it("is reached from the strip's own `+`, in the shape the project strip has", async () => {
    // charter-app#193, the operator: *"also no new workspace button in workspaces tab — it
    // should be like projects tabs buttons"*. `workspace.create` has been a catalogue row
    // since #172, with the dialog behind it — the palette runs it and the tab's menu lists
    // it — and the one strip that is entirely about workspaces had no way to run it.
    const { calls } = core();
    render(<App />);
    await settled();

    // Beside the tabs and not among them: the strip's controls are a sibling of the tablist,
    // so a `role="tab"` query never picks this up and the collapse never hides it.
    const row = screen.getByRole("tablist", { name: "Workspaces" }).parentElement as HTMLElement;
    const plus = within(row).getByRole("button", { name: "New workspace…" });
    // Icon-only, like the project strip's two: the catalogue's words are its `aria-label`,
    // which is what a screen reader reads and what this test just found it by.
    expect(plus.textContent).toBe("");
    expect(plus.querySelector("svg.lucide-plus")).not.toBeNull();

    await userEvent.click(plus);
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "gamma");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

    // The same command the menu's row sends, because it is the same row.
    expect(calls("workspace_create").map((one) => one.args)).toEqual([
      { plane: PLANE, name: "gamma", vision: null, live: false },
    ]);
  });

  it("is reached from the same menu, and asks for the name and the vision", async () => {
    const { calls } = core();
    render(<App />);
    await settled();
    const dialog = await askToCreate();

    await userEvent.type(within(dialog).getByLabelText("Name"), "gamma");
    await userEvent.type(within(dialog).getByLabelText("What it is for (optional)"), "ship it");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

    expect(calls("workspace_create").map((one) => one.args)).toEqual([
      { plane: PLANE, name: "gamma", vision: "ship it", live: false },
    ]);
  });

  it("sends no vision when the box was left empty", async () => {
    // An empty box is no vision, not a vision that is empty — the core would otherwise write
    // an empty `## Vision` and read it back as one that had been recorded.
    const { calls } = core();
    render(<App />);
    await settled();
    const dialog = await askToCreate();

    await userEvent.type(within(dialog).getByLabelText("Name"), "gamma");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

    expect(calls("workspace_create")[0].args.vision).toBeNull();
  });

  it("cannot be answered with nothing", async () => {
    core();
    render(<App />);
    await settled();
    const dialog = await askToCreate();

    expect(within(dialog).getByRole("button", { name: "Create workspace" })).toBeDisabled();
  });

  it("shows the core's refusal about a name, in the dialog, and stays open", async () => {
    // The window validates no name of its own: `workspace_create` runs `wscmd::ensure`, which
    // is where `contain::workspace_name_ok` is. A second alphabet here would drift.
    // Logged out of the forge, as CI is: the picker's note about that is not the refusal.
    core({ loggedOut: true });
    render(<App />);
    await settled();
    const dialog = await askToCreate();

    await userEvent.type(within(dialog).getByLabelText("Name"), "../escape");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "invalid workspace name '../escape'",
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("reads the plane again rather than writing the new workspace into its own copy", async () => {
    const { calls } = core();
    render(<App />);
    await settled();
    const before = calls("plane_sidebar").length;
    const dialog = await askToCreate();

    await userEvent.type(within(dialog).getByLabelText("Name"), "gamma");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

    await vi.waitFor(() => expect(calls("plane_sidebar").length).toBeGreaterThan(before));
  });
});

describe("a plane with no workspace yet", () => {
  it("offers to create one in the middle of the window, and keeps the strip's `+`", async () => {
    // The first workspace could be made only from the palette: the strip and its `+` were
    // drawn only once a workspace existed, and the empty window offered a chat.
    const { calls } = core({ workspaces: [] });
    render(<App />);

    const empty = await screen.findByTestId("empty-plane");
    expect(empty).toHaveTextContent("No workspaces yet");
    const row = screen.getByRole("tablist", { name: "Workspaces" }).parentElement as HTMLElement;
    expect(within(row).getByRole("button", { name: "New workspace…" })).toBeInTheDocument();

    await userEvent.click(within(empty).getByRole("button", { name: "Create a workspace" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "first");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));

    expect(calls("workspace_create").map((one) => one.args.name)).toEqual(["first"]);
  });
});

describe("picking a new workspace's repos (ADR 0055)", () => {
  async function createWithRepos(repos: string[]) {
    await menuOn("alpha");
    await userEvent.click(screen.getByRole("menuitem", { name: "New workspace…" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "gamma");
    for (const repo of repos)
      await userEvent.click(await within(dialog).findByRole("checkbox", { name: repo }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Create workspace" }));
  }

  it("makes the workspace first, then clones each picked repo in turn", async () => {
    const { asked } = core();
    render(<App />);
    await settled();

    await createWithRepos(["web", "api"]);

    await vi.waitFor(() =>
      expect(
        asked
          .filter((one) => ["workspace_create", "take_repos", "clone_repo"].includes(one.cmd))
          .map((one) => [one.cmd, one.args.repos ?? one.args.repo ?? one.args.name]),
      ).toEqual([
        ["workspace_create", "gamma"],
        ["take_repos", ["api", "web"]],
        ["clone_repo", "api"],
        ["clone_repo", "web"],
      ]),
    );
    expect(await screen.findByText("Cloned api, web into gamma.")).toBeInTheDocument();
  });

  it("says which repo did not clone, in the core's words, and clones the rest", async () => {
    const { calls } = core({ cloneFails: { api: "api: clone failed — no access." } });
    render(<App />);
    await settled();

    await createWithRepos(["api", "web"]);

    expect(await screen.findByText(/Could not clone api into gamma/)).toHaveTextContent(
      "api: clone failed — no access.",
    );
    expect(calls("clone_repo").map((one) => one.args.repo)).toEqual(["api", "web"]);
  });

  it("clones nothing when nothing was picked", async () => {
    const { calls } = core();
    render(<App />);
    await settled();

    await createWithRepos([]);

    await vi.waitFor(() => expect(calls("workspace_create")).toHaveLength(1));
    expect(calls("take_repos")).toEqual([]);
    expect(calls("clone_repo")).toEqual([]);
  });
});

describe("a workspace's settings (charter-app#280)", () => {
  it("open from the workspace tab's menu, in a tab on that workspace's strip, about that workspace", async () => {
    const { calls } = core();
    render(<App />);
    await settled();

    await menuOn("beta");
    await userEvent.click(screen.getByRole("menuitem", { name: /Workspace settings/ }));

    expect(
      await screen.findByRole("heading", { name: "Workspace settings · beta" }),
    ).toBeInTheDocument();
    await vi.waitFor(() =>
      expect(calls("workspace_settings").map((one) => one.args)).toContainEqual({
        plane: PLANE,
        workspace: "beta",
      }),
    );
    // Filed on beta's strip, which is now the one in front.
    expect(
      within(screen.getByRole("tablist", { name: "Workspaces" })).getByRole("tab", {
        selected: true,
      }),
    ).toHaveTextContent("beta");
  });

  it("untick a repo to remove it, and show the core's refusal when it holds work", async () => {
    // ADR 0055: the guard is inside `drop_repo`, and the window only draws what it said.
    const { calls } = core();
    render(<App />);
    await settled();
    await menuOn("beta");
    await userEvent.click(screen.getByRole("menuitem", { name: /Workspace settings/ }));
    const repos = await screen.findByRole("group", { name: "Repos" });

    const api = await within(repos).findByRole("checkbox", { name: "api" });
    await waitFor(() => expect(api).toBeChecked());
    await userEvent.click(api);
    await userEvent.click(within(repos).getByRole("button", { name: "Remove 1" }));

    expect(await within(repos).findByRole("alert")).toHaveTextContent("api: uncommitted changes");
    expect(calls("drop_repo").map((one) => one.args)).toEqual([
      { plane: PLANE, workspace: "beta", repo: "api" },
    ]);
  });

  it("filter what the window draws by the focused workspace's answer", async () => {
    const { calls } = core();
    render(<App />);
    await settled();

    await menuOn("beta");
    await userEvent.click(screen.getByRole("menuitem", { name: "Focus workspace beta" }));

    await vi.waitFor(() =>
      expect(calls("extensions_on").map((one) => one.args)).toContainEqual({
        plane: PLANE,
        workspace: "beta",
      }),
    );
  });
});

describe("renaming a workspace (charter#367)", () => {
  it("asks for the name from the workspace's menu, shows the core's refusal, and lands on the new name", async () => {
    const { calls } = core();
    render(<App />);
    await settled();

    await menuOn("alpha");
    await userEvent.click(screen.getByRole("menuitem", { name: /Rename workspace alpha/ }));
    const dialog = await screen.findByRole("dialog", { name: "Rename workspace alpha" });
    const box = within(dialog).getByLabelText("New name");

    // A taken name: refused by the core, in its words, and the dialog stays for another.
    await userEvent.clear(box);
    await userEvent.type(box, "beta");
    await userEvent.click(within(dialog).getByRole("button", { name: "Rename workspace" }));
    expect(
      await within(dialog).findByText(
        "workspace 'beta' already exists — pick another name or remove it first.",
      ),
    ).toBeInTheDocument();

    await userEvent.clear(box);
    await userEvent.type(box, "gamma");
    await userEvent.click(within(dialog).getByRole("button", { name: "Rename workspace" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("workspace_rename").map((one) => one.args)).toEqual([
      { plane: PLANE, workspace: "alpha", name: "beta" },
      { plane: PLANE, workspace: "alpha", name: "gamma" },
    ]);
    // The window was in alpha, so it is in gamma now.
    await waitFor(() => expect(strip()).toEqual(["beta", "gamma"]));
  });

  it("does not offer to rename a workspace to the name it already has", async () => {
    core();
    render(<App />);
    await settled();

    await menuOn("alpha");
    await userEvent.click(screen.getByRole("menuitem", { name: /Rename workspace alpha/ }));
    const dialog = await screen.findByRole("dialog", { name: "Rename workspace alpha" });

    expect(within(dialog).getByRole("button", { name: "Rename workspace" })).toBeDisabled();
  });
});
