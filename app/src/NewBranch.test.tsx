import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { NewBranch } from "./NewBranch";

afterEach(cleanup);

const TAKEN =
  "branch 'spike' already exists in svc. Pick another name, or delete that branch if nothing on it is needed.";

function show(over: { trouble?: string; making?: boolean } = {}) {
  const onCut = vi.fn();
  const onCancel = vi.fn();
  render(
    <NewBranch
      repo="svc"
      trouble={over.trouble}
      making={over.making ?? false}
      onCut={onCut}
      onCancel={onCancel}
    />,
  );
  return { onCut, onCancel, user: userEvent.setup() };
}

describe("cutting a new branch from the window (GL-1)", () => {
  it("names the repo the branch will be cut in", () => {
    show();

    expect(screen.getByRole("dialog", { name: "New branch" })).toHaveTextContent("in svc");
  });

  it("cuts a branch under the name typed", async () => {
    const { onCut, user } = show();

    await user.type(screen.getByRole("textbox", { name: /^Name/ }), "spike{Enter}");

    expect(onCut).toHaveBeenCalledWith("spike");
  });

  it("leaves the name to charter when none is typed", async () => {
    const { onCut, user } = show();

    await user.click(screen.getByRole("button", { name: "Create branch" }));

    expect(onCut).toHaveBeenCalledWith(null);
  });

  it("cuts nothing on Escape", async () => {
    const { onCut, onCancel, user } = show();

    await user.keyboard("{Escape}");

    expect(onCancel).toHaveBeenCalled();
    expect(onCut).not.toHaveBeenCalled();
  });

  it("shows the core's refusal in the dialog, and cannot be answered twice", () => {
    // The sentence `worktree_add` answers for a taken name — `Refusal::in_window`, held to
    // these words by `worktrees::tests` — so what this dialog draws is what the core says.
    show({ trouble: TAKEN, making: true });

    expect(screen.getByRole("alert")).toHaveTextContent(TAKEN);
    expect(screen.getByRole("dialog").textContent ?? "").not.toMatch(/worktree|piece/i);
    // While it runs the act says so (#630).
    expect(screen.getByRole("button", { name: "Creating…" })).toBeDisabled();
  });

  it("says branch, and none of the words the first hour keeps out (ADR 0072 §3)", () => {
    show();

    expect(screen.getByRole("dialog").textContent ?? "").not.toMatch(/worktree|piece/i);
  });
});
