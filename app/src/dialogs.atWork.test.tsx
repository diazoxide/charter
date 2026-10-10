import type { ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { DeleteVault } from "./DeleteVault";
import { DeleteWorkspace } from "./DeleteWorkspace";
import { NewBranch } from "./NewBranch";
import { NewPersona } from "./NewPersona";
import { NewProject } from "./NewProject";
import { NewVault } from "./NewVault";
import { NewWorkspace } from "./NewWorkspace";
import { RemovePersona } from "./RemovePersona";
import { RemoveFromWorkspace } from "./RemoveFromWorkspace";
import { userEvent } from "@testing-library/user-event";

afterEach(() => {
  cleanup();
  clearMocks();
});

const none = () => undefined;

/**
 * **A dialog at work says what it is doing** (#630, DS-8; `docs/ui-copy.md`, *Work in
 * progress*): its act, pressed, reads as a present participle with an ellipsis until the core
 * answers, as Rename workspace's `Renaming…` and Start's `Starting…` already do. A button that
 * only greys out says nothing about whether the press was taken.
 */
describe("a dialog at work", () => {
  const cases: [string, ReactElement, string][] = [
    ["New branch", <NewBranch repo="svc" making onCut={none} onCancel={none} />, "Creating…"],
    ["New persona", <NewPersona plane="ops" making onCreate={none} onCancel={none} />, "Creating…"],
    [
      "New vault",
      <NewVault plane="ops" making onCreate={none} onMade={none} onCancel={none} />,
      "Creating…",
    ],
    [
      "New workspace",
      <NewWorkspace plane="ops" planeId="ops" making onCreate={none} onCancel={none} />,
      "Creating…",
    ],
    [
      "Delete workspace",
      <DeleteWorkspace workspace="alpha" atRisk={[]} deleting onDelete={none} onCancel={none} />,
      "Deleting…",
    ],
    [
      "Delete vault",
      <DeleteVault vault="db" deleting onDelete={none} onCancel={none} />,
      "Deleting…",
    ],
    [
      "Delete persona",
      <RemovePersona persona="ops" deleting onDelete={none} onCancel={none} />,
      "Deleting…",
    ],
  ];

  for (const [name, dialog, doing] of cases) {
    it(`${name} says ${doing} on its act while it runs`, () => {
      mockIPC(() => new Promise(() => undefined));
      render(dialog);
      expect(screen.getByRole("button", { name: doing })).toBeDisabled();
    });
  }
});

describe("Remove from workspace (#630)", () => {
  it("says what it does in the reader's words, not the file underneath", () => {
    render(
      <RemoveFromWorkspace plane="ops" workspace="alpha" repo="svc" onClose={none} onDone={none} />,
    );
    expect(screen.getByText(/alpha stops naming svc\. Nothing is deleted/)).toBeInTheDocument();
    expect(screen.queryByText(/workspace\.json/)).not.toBeInTheDocument();
  });

  it("says Removing… while it runs, and Escape does not close it under the answer", async () => {
    mockIPC(() => new Promise(() => undefined));
    const onClose = vi.fn();
    render(
      <RemoveFromWorkspace
        plane="ops"
        workspace="alpha"
        repo="svc"
        onClose={onClose}
        onDone={none}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Remove from workspace" }));
    expect(await screen.findByRole("button", { name: "Removing…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("New project at work (#630)", () => {
  it("says it is copying the repo while one opens, in the first run's words", () => {
    render(<NewProject making={false} opening onCreate={none} onOpenRepo={none} onCancel={none} />);
    expect(screen.getByText("Copying your repo into its workspace…")).toBeInTheDocument();
  });

  it("says Creating… on Create project while a project of its own is made", () => {
    render(<NewProject making opening={false} onCreate={none} onOpenRepo={none} onCancel={none} />);
    expect(screen.getByRole("button", { name: "Creating…", hidden: true })).toBeDisabled();
  });

  it("calls a code repository a repo, everywhere it is named", () => {
    render(
      <NewProject
        making={false}
        opening={false}
        onCreate={none}
        onOpenRepo={none}
        onCancel={none}
      />,
    );
    expect(document.body.textContent).not.toMatch(/repository/i);
    expect(screen.getByLabelText("Repo to adopt")).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Browse for the repo to adopt", hidden: true }),
    ).toBeTruthy();
  });
});
