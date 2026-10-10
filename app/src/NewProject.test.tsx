import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import { stripNamed } from "./test-strips";

/**
 * Making a new project, from the window.
 *
 * **Two claims, and the second is the one that matters.** That the dialog is reachable and
 * sends what was typed; and that a plane charter has just created is still opened **through
 * the trust gate** (ADR 0035) — so the ordinary end of this flow is the approval
 * dialog, exactly as it would be for a project that came from a recents row.
 *
 * What `create_project` writes is `scaffold::init`'s and is tested in
 * `app/src-tauri/src/opener.rs`, against real directories and a real git repository. Nothing
 * here re-asserts it: this is about the window.
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
const MADE = "/home/dev/new-thing";
const REPO = "/home/dev/widget";

/** The four-line refusal `init` gives inside a git repository (ADR 0035, decision 27). */
const NOT_INTO_A_REPO = [
  "✗ this is the git repo 'svc', and `charter init` does not make a repository into a control plane unless you ask it to. Nothing was written.",
  "• A plane is a directory of its own, and this repo is the first clone in it:",
  "• To make THIS repo the plane instead — ask for it by name: purlis init --plane-is-this-repo",
].join("\n");

/** The core, with a plane open and a `create_project` that answers as charter's does. */
function core(
  over: {
    answer?: unknown;
    refuses?: string;
    repoRefuses?: string;
    asksForge?: boolean;
    /** Refuses once the forge is answered: a refusal that comes after the question. */
    refusesWithForge?: string;
    pickFails?: string;
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const got = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: got });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "opened_chats") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "plane_sidebar")
      return { root: PLANE, personas: [], persona: null, unfiled: [], workspaces: [] };
    if (cmd === "pick_project") {
      if (over.pickFails !== undefined) throw over.pickFails;
      return null;
    }
    if (cmd === "open_repo") {
      if (over.repoRefuses !== undefined) throw over.repoRefuses;
      return {
        opened: { opened: { plane: PLANE, ask: null }, workspace: "widget", cwd: `${PLANE}/w` },
        asks_forge: null,
      };
    }
    if (cmd === "create_project") {
      if (over.refuses !== undefined) throw over.refuses;
      if (over.refusesWithForge !== undefined && got.forge !== null) throw over.refusesWithForge;
      if (over.asksForge && got.forge === null)
        return { opened: null, asks_forge: "no repo was named to read it from" };
      return {
        opened: over.answer ?? {
          plane: null,
          ask: {
            path: MADE,
            contributes: { plugins: [], env: [], starts: [], profiles: [], grants: [] },
            changes: [],
            first: true,
          },
        },
        asks_forge: null,
      };
    }
    return null;
  });
  return { asked, calls: (cmd: string) => asked.filter((one) => one.cmd === cmd) };
}

/** Opens the dialog the way an operator does: right-click the project tab, then the row. */
async function askForOne() {
  const tab = await screen.findByRole("tab", { name: /plane/ });
  fireEvent.contextMenu(tab);
  await screen.findByRole("menu");
  await userEvent.click(screen.getByRole("menuitem", { name: "New project…" }));
  return await screen.findByRole("dialog", { name: "New project" });
}

describe("making a project", () => {
  it("is offered on the project tab's own menu", async () => {
    core();
    render(<App />);

    const tab = await screen.findByRole("tab", { name: /plane/ });
    fireEvent.contextMenu(tab);

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((row) => row.getAttribute("aria-label")),
    ).toEqual([
      "Switch to project plane",
      "Pin project plane",
      "Project settings…",
      "Saving…",
      // Greyed: it is the only project in this window (charter#126).
      "Move project plane to a new window",
      "New project…",
      "Open a project…",
      "Close project plane",
    ]);
  });

  /**
   * **The button, which is all charter-app#178 was still missing.** #172 built the dialog, the
   * command and the catalogue row behind it and left the chrome; this is the chrome, and what
   * it has to be is the same row the menu above already reaches. So the assertion is on the
   * strip's pair: two controls, no words between them, each named by `actions.projectRows`.
   */
  it("is on the project strip too, beside the one that opens a project that exists", async () => {
    core();
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    const controls = within(stripNamed("Projects"))
      .getAllByRole("button")
      .filter((button) => button.classList.contains("bare"));

    // Icon-only, in the operator's own words for this strip — *"without label — just icon"* —
    // so the accessible name is the whole name and the order is open, then create.
    expect(controls.map((button) => button.getAttribute("aria-label"))).toEqual([
      "Open a project…",
      "New project…",
    ]);
    expect(controls.map((button) => button.textContent)).toEqual(["", ""]);

    await userEvent.click(controls[1]);
    expect(await screen.findByRole("dialog", { name: "New project" })).toBeInTheDocument();
  });

  it("opens a repository into the local project by default, asking nothing about where", async () => {
    // FR-4: the default is one directory, the repository. The two-directory form is under
    // Advanced, closed.
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    // The keyboard is in the one box the default asks for, as the dialog opens.
    expect(within(dialog).getByLabelText("Repo")).toHaveFocus();

    const advanced = within(dialog).getByText("Advanced").closest("details");
    expect(advanced).not.toHaveAttribute("open");
    expect(advanced).toContainElement(within(dialog).getByLabelText("Folder"));

    await userEvent.type(within(dialog).getByLabelText("Repo"), REPO);
    await userEvent.click(within(dialog).getByRole("button", { name: "Open repo" }));

    expect(calls("open_repo").map((one) => one.args)).toEqual([
      { path: REPO, template: { kind: "no-template" }, forge: null },
    ]);
    expect(calls("create_project")).toEqual([]);
  });

  it("is drawn from the settings set, each answer tied to its line of help", async () => {
    // DS-3c (#1175): the rows, fields and box are the house set's, not the hand-built
    // `asks` / `picking` / `choice` classes, and each line of help describes its own box.
    core();
    render(<App />);
    const dialog = await askForOne();
    for (const name of ["Repo", "Folder", "Repository to adopt", "Make this repo itself the project"])
      expect(within(dialog).getByLabelText(name).closest(".ui-setting-row")).not.toBeNull();

    expect(within(dialog).getByLabelText("Folder")).toHaveAccessibleDescription(
      "It does not have to exist yet. purlis makes it, and writes the project into it.",
    );
    expect(within(dialog).getByLabelText("Repository to adopt")).toHaveAccessibleDescription(
      /^Optional: the project goes in the folder above/,
    );
    expect(
      within(dialog).getByLabelText("Make this repo itself the project"),
    ).toHaveAccessibleDescription(/^Only for a folder that is the top of a git repository/);
  });

  it("keeps a refused repository's words in the dialog", async () => {
    const refusal = "/home/dev/widget is not the top level of a git repo.";
    core({ repoRefuses: refusal });
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Repo"), REPO);
    await userEvent.click(within(dialog).getByRole("button", { name: "Open repo" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(refusal);
  });

  it("sends the folder that was typed, and does not make the repo the plane", async () => {
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(calls("create_project").map((one) => one.args)).toEqual([
      { path: MADE, planeIsThisRepo: false, adopt: null, forge: null },
    ]);
  });

  it("sends the repository to adopt beside the folder the plane goes in", async () => {
    // ADR 0035's default: the plane in a directory of its own and that repo as its first
    // clone. Two answers, two boxes — neither derived from the other.
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.type(within(dialog).getByLabelText("Repository to adopt"), REPO);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(calls("create_project").map((one) => one.args)).toEqual([
      { path: MADE, planeIsThisRepo: false, adopt: REPO, forge: null },
    ]);
  });

  it("sends no repository when the box that makes this repo the plane is ticked", async () => {
    // The two are answers to one question — which repository this plane starts from — so
    // ticking the box takes the adopt field out of the dialog's answer as well as out of
    // reach, rather than sending both and letting the core rank them.
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Repository to adopt"), REPO);
    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByLabelText("Make this repo itself the project"));

    expect(within(dialog).getByLabelText("Repository to adopt")).toBeDisabled();

    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(calls("create_project")[0].args).toEqual({
      path: MADE,
      planeIsThisRepo: true,
      adopt: null,
      forge: null,
    });
  });

  it("asks for the old shape only when the box is ticked", async () => {
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByLabelText("Make this repo itself the project"));
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(calls("create_project")[0].args.planeIsThisRepo).toBe(true);
  });

  it("asks which forge when there is no remote to read, and makes the project with the answer", async () => {
    const { calls } = core({ asksForge: true });
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));
    const question = await within(dialog).findByRole("group", {
      name: "Which forge are its repos on?",
    });
    expect(question).toHaveTextContent("no repo was named to read it from");
    await userEvent.click(within(question).getByRole("button", { name: "GitHub" }));

    await vi.waitFor(() =>
      expect(calls("create_project").map((one) => one.args)).toEqual([
        { path: MADE, planeIsThisRepo: false, adopt: null, forge: null },
        { path: MADE, planeIsThisRepo: false, adopt: null, forge: "github" },
      ]),
    );
    expect(await screen.findByRole("dialog", { name: "Open this project?" })).toBeInTheDocument();
  });

  it("drops a pending forge question once the form it was about changes", async () => {
    core({ asksForge: true });
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));
    await within(dialog).findByRole("group", { name: "Which forge are its repos on?" });
    await userEvent.type(within(dialog).getByLabelText("Folder"), "-2");

    expect(
      within(dialog).queryByRole("group", { name: "Which forge are its repos on?" }),
    ).toBeNull();
  });

  it("opens what it made through the trust gate, not around it", async () => {
    // **The claim of this file.** `create_project` answers with `open_if_approved`'s own
    // answer, so a plane charter has just scaffolded raises the same question a stranger's
    // would. A flow that opened its own new project without asking would be a second way in.
    core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    const question = await screen.findByRole("dialog", { name: "Open this project?" });
    expect(question).toHaveTextContent(MADE);
  });

  it("takes the project straight into the window when the gate already said yes", async () => {
    core({ answer: { plane: MADE, ask: null } });
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    await vi.waitFor(() =>
      expect(screen.getAllByRole("tab", { name: /new-thing/ })).toHaveLength(1),
    );
  });

  it("keeps the core's refusal in the dialog, in full, with its line breaks", async () => {
    // `init`'s refusal in a repository is four lines naming what to do instead. An operator
    // shown a one-line summary of it can follow none of it.
    core({ refuses: NOT_INTO_A_REPO });
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), "/home/dev/svc");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    const refusal = await within(dialog).findByRole("alert");
    expect(refusal.textContent).toBe(NOT_INTO_A_REPO);
    expect(refusal).toHaveClass("said-in-full");
    expect(screen.getByRole("dialog", { name: "New project" })).toBeInTheDocument();
  });

  it("cannot be answered with no folder", async () => {
    core();
    render(<App />);
    const dialog = await askForOne();

    expect(within(dialog).getByRole("button", { name: "Create project" })).toBeDisabled();
  });

  it("says a folder dialog that could not open in the dialog, which a cancel does not (#1291)", async () => {
    core({ pickFails: "the folder dialog could not be opened" });
    render(<App />);
    const dialog = await askForOne();

    await userEvent.click(within(dialog).getByRole("button", { name: "Browse for the repo" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "the folder dialog could not be opened",
    );
    expect(screen.getByRole("dialog", { name: "New project" })).toBeInTheDocument();
  });

  it("gives a failed dialog's line to a newer refusal (#1291)", async () => {
    // The forge's answer sends the form again without the form's own button, so a refusal can
    // come after the dialog failed. The newer one is said, never the old line in its place.
    core({
      asksForge: true,
      pickFails: "the folder dialog could not be opened",
      refusesWithForge: "✗ /home/dev/new-thing is not empty. Nothing was written.",
    });
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));
    const question = await within(dialog).findByRole("group", {
      name: "Which forge are its repos on?",
    });
    await userEvent.click(within(dialog).getByRole("button", { name: "Browse for the repo" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("could not be opened");

    await userEvent.click(within(question).getByRole("button", { name: "GitHub" }));

    await vi.waitFor(() =>
      expect(within(dialog).getByRole("alert")).toHaveTextContent("is not empty"),
    );
    expect(within(dialog).getByRole("alert")).not.toHaveTextContent("could not be opened");
  });

  it("says nothing when the folder dialog is cancelled", async () => {
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.click(within(dialog).getByRole("button", { name: "Browse for the repo" }));

    await vi.waitFor(() => expect(calls("pick_project")).toHaveLength(1));
    expect(within(dialog).queryByRole("alert")).not.toBeInTheDocument();
  });
});
