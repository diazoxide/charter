import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";

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
  "• To make THIS repo the plane instead — ask for it by name: charter init --plane-is-this-repo",
].join("\n");

/** The core, with a plane open and a `create_project` that answers as charter's does. */
function core(over: { answer?: unknown; refuses?: string } = {}) {
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
    if (cmd === "create_project") {
      if (over.refuses !== undefined) throw over.refuses;
      return (
        over.answer ?? {
          plane: null,
          ask: {
            path: MADE,
            contributes: { plugins: [], env: [], starts: [], profiles: [] },
            changes: [],
            first: true,
          },
        }
      );
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

    const controls = within(screen.getByRole("tablist", { name: "Projects" }))
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

  it("sends the folder that was typed, and does not make the repo the plane", async () => {
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(calls("create_project").map((one) => one.args)).toEqual([
      { path: MADE, planeIsThisRepo: false, adopt: null },
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
      { path: MADE, planeIsThisRepo: false, adopt: REPO },
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
    await userEvent.click(within(dialog).getByLabelText("Make this repo itself the plane"));

    expect(within(dialog).getByLabelText("Repository to adopt")).toBeDisabled();

    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(calls("create_project")[0].args).toEqual({
      path: MADE,
      planeIsThisRepo: true,
      adopt: null,
    });
  });

  it("asks for the old shape only when the box is ticked", async () => {
    const { calls } = core();
    render(<App />);
    const dialog = await askForOne();

    await userEvent.type(within(dialog).getByLabelText("Folder"), MADE);
    await userEvent.click(within(dialog).getByLabelText("Make this repo itself the plane"));
    await userEvent.click(within(dialog).getByRole("button", { name: "Create project" }));

    expect(calls("create_project")[0].args.planeIsThisRepo).toBe(true);
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
});
