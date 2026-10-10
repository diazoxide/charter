import { afterEach, describe, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { AskFirst } from "./ExtensionAction";
import { ApproveExtension } from "./ApproveExtension";
import { ApprovePlane } from "./ApprovePlane";
import { AskPersona } from "./AskPersona";
import { BriefPanel } from "./Brief";
import { ClosingProject } from "./ClosingProject";
import { DeleteWorkspace } from "./DeleteWorkspace";
import { NewWorkspace } from "./NewWorkspace";
import { PersonaProfile } from "./PersonaProfile";
import { RelaunchAsk } from "./RelaunchAsk";
import type { Ending } from "./QuitWarning";

afterEach(() => {
  cleanup();
  clearMocks();
});

const none = () => undefined;

/**
 * **DS-8's second pass** (#630, #1719): the dialogs the first walk left, each checked against
 * the answer bar's rule, the copy guide and the reading and working states
 * (`docs/design-system.md`, `docs/ui-copy.md`). One case per finding fixed here.
 */

const CHAT: Ending = {
  key: "/ide.1",
  name: "ide.1",
  harness: "claude",
  cwd: null,
  workspace: "ide",
  state: "waiting",
};

describe("Reopen your sessions", () => {
  it("writes each answer's type, and marks Start fresh as the answer that loses the record", () => {
    render(
      <RelaunchAsk
        question={{ projects: [{ plane: "/p", chats: 2, views: 0 }], after_update: false }}
        nameOf={(plane) => plane}
        onAnswer={none}
      />,
    );

    const question = screen.getByRole("alertdialog");
    const answers = within(question).getAllByRole("button");
    for (const answer of answers) expect(answer).toHaveAttribute("type", "button");
    expect(within(question).getByRole("button", { name: "Start fresh" })).toHaveClass("ends-it");
  });
});

describe("closing a project", () => {
  it("names the project and the act alike when it has not heard of any chat yet", () => {
    render(<ClosingProject name="one" chats={[]} heard={false} onClose={none} onCancel={none} />);

    expect(screen.getByRole("alertdialog", { name: "Close project one?" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close project" })).toBeInTheDocument();
  });

  it("lists the chats with the quit warning's own rows", () => {
    render(<ClosingProject name="one" chats={[CHAT]} heard onClose={none} onCancel={none} />);

    const list = within(screen.getByRole("alertdialog")).getByRole("list");
    expect(list).toHaveClass("ending");
    expect(within(list).getByText("ide.1")).toBeInTheDocument();
  });
});

describe("an extension's action that asks first", () => {
  const action = { id: "a", title: "Careful", asks_first: true, deletes: false };

  for (const [deletes, doing] of [
    [false, "Running…"],
    [true, "Deleting…"],
  ] as const) {
    it(`says ${doing} while the core runs it`, async () => {
      render(
        <AskFirst
          extension="probe"
          action={{ ...action, deletes }}
          onRun={() => new Promise(() => undefined)}
          onCancel={none}
        />,
      );

      await userEvent.click(screen.getByRole("button", { name: deletes ? "Delete" : "Run" }));

      expect(screen.getByRole("button", { name: doing })).toBeDisabled();
    });
  }
});

describe("a persona's profile", () => {
  const read = { named: null, offered: ["work"], inherited_from: null };

  it("says it is reading the definition until it has", () => {
    render(<PersonaProfile persona="ops" saving={false} onSave={none} onCancel={none} />);

    expect(screen.getByText("Reading the profile ops names…")).toBeInTheDocument();
  });

  it("says nothing of reading once the read failed, and shows why", () => {
    render(
      <PersonaProfile
        persona="ops"
        unreadable="purlis could not read personas/ops/persona.md"
        saving={false}
        onSave={none}
        onCancel={none}
      />,
    );

    expect(screen.queryByText(/^Reading the profile/)).toBeNull();
    expect(screen.getByRole("alert")).toHaveTextContent("could not read");
  });

  it("writes its labels in sentence case, and Save names what it saves", () => {
    render(
      <PersonaProfile persona="ops" read={read} saving={false} onSave={none} onCancel={none} />,
    );

    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("radio", { name: "None" })).toBeInTheDocument();
    expect(within(dialog).getByText(/^Written as/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save profile" })).toBeInTheDocument();
  });
});

describe("asking a persona", () => {
  it("starts its lines with a capital, the hint as well", () => {
    render(
      <AskPersona
        persona="devops"
        chat="steward 3"
        workspaces={[]}
        asking={false}
        onAsk={none}
        onCancel={none}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "Ask devops" });
    expect(dialog.querySelector(".where")?.textContent).toBe("From steward 3");
    expect(within(dialog).getByLabelText("Task name")).toHaveAttribute(
      "placeholder",
      "Check the queue",
    );
  });
});

describe("a task's brief", () => {
  it("says it is reading the brief, with a capital, until the core answers", () => {
    mockIPC(() => new Promise(() => {}));
    render(<BriefPanel plane="/p" of={{ name: "devops 6", chat: 6 }} onClose={none} />);

    expect(screen.getByText("Reading the brief…")).toBeInTheDocument();
  });
});

describe("the two trust questions", () => {
  it("describe an extension by what purlis cannot stop it doing", () => {
    render(
      <ApproveExtension
        ask={{
          id: "x",
          name: "probe",
          path: "/ext/probe",
          declares: ["a theme"],
          fingerprint: "f",
          first: true,
          runs_as_you: "It runs as you.",
          fingerprint_note: "purlis read every file.",
          state_note: null,
        }}
        onApprove={none}
        onCancel={none}
      />,
    );

    expect(screen.getByRole("dialog")).toHaveAccessibleDescription("It runs as you.");
  });

  it("describe a project by what purlis cannot judge in it", () => {
    render(
      <ApprovePlane
        ask={{
          path: "/p",
          contributes: { plugins: [], env: [], starts: [], profiles: [], grants: [] },
          changes: [],
          first: true,
        }}
        onApprove={none}
        onCancel={none}
      />,
    );

    expect(screen.getByRole("dialog")).toHaveAccessibleDescription(
      /^purlis can only list what it can read\./,
    );
  });
});

describe("deleting a workspace", () => {
  it("says what goes in the reader's words, not the folder underneath (#1719)", () => {
    render(
      <DeleteWorkspace
        workspace="alpha"
        atRisk={[]}
        deleting={false}
        onDelete={none}
        onCancel={none}
      />,
    );

    const said = screen.getByRole("alertdialog").textContent ?? "";
    expect(said).toContain("This deletes workspace alpha and everything in it:");
    expect(said).not.toContain("workspaces/alpha/");
  });
});

describe("a new workspace's Live row", () => {
  it("says one thing per sentence (#1719)", () => {
    mockIPC(() => ({ repos: [], trouble: [] }));
    render(
      <NewWorkspace plane="ops" planeId="ops" making={false} onCreate={none} onCancel={none} />,
    );

    expect(screen.getByRole("checkbox", { name: "Live" })).toHaveAccessibleDescription(
      "Its charter, memory and todos are committed with the project and published by every " +
        "save. Ticked, the project is saved as soon as it is made, as Saving says. Left " +
        "unticked, they stay on this machine.",
    );
  });
});
