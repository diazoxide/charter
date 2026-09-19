import { describe, expect, test, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RemovePiece, WorktreeMark } from "./Worktree";
import type { ChatWorktree } from "./bindings";

const piece: ChatWorktree = {
  workspace: "ide",
  repo: "charter-app",
  piece: "fix-login",
  branch: "fix-login",
  wired: false,
  stale: false,
};

describe("what a chat's row says about its worktree", () => {
  test("a chat on a piece shows the branch it is on", () => {
    render(<WorktreeMark worktree={piece} />);

    expect(screen.getByText("fix-login")).toBeInTheDocument();
  });

  test("a worktree with no charter layer says so, because the guards are not on", () => {
    // Not decoration. A chat here runs with none of the plane's ask/deny rules and none of
    // its persona's agents, and a silent row lets the operator believe otherwise.
    render(<WorktreeMark worktree={piece} />);

    const label = screen.getByText("unwired");
    expect(label).toBeInTheDocument();
    expect(label.title).toMatch(/no persona agents/i);
    expect(label.title).toMatch(/ask\/deny/i);
  });

  test("a wired worktree carries no label", () => {
    render(<WorktreeMark worktree={{ ...piece, wired: true }} />);

    expect(screen.queryByText("unwired")).not.toBeInTheDocument();
  });

  test("a registration whose directory is gone reads stale, and not unwired", () => {
    // Both are true of a stale row, and saying both would put a guard warning on a tree that
    // no longer exists.
    render(<WorktreeMark worktree={{ ...piece, stale: true }} />);

    expect(screen.getByText("stale")).toBeInTheDocument();
    expect(screen.queryByText("unwired")).not.toBeInTheDocument();
  });
});

describe("removing a piece", () => {
  test("it says what it keeps before anything is asked", () => {
    render(<RemovePiece worktree={piece} plane="/plane" onRemove={vi.fn()} />);

    expect(screen.getByText(/the branch/i)).toHaveTextContent("fix-login");
    expect(screen.getByText(/the branch/i)).toHaveTextContent(/stays/i);
  });

  test("a refusal is shown in the core's own words, not reworded", async () => {
    // The sentence names the repair. An operator shown a generic failure cannot follow it,
    // and cannot search for it.
    const said =
      "'fix-login' has 2 commit(s) that exist nowhere else — refusing to remove. Push the branch or merge it, or discard with --force";
    const onRemove = vi.fn().mockResolvedValue({ status: "error", error: said });
    render(<RemovePiece worktree={piece} plane="/plane" onRemove={onRemove} />);

    await userEvent.click(screen.getByRole("button", { name: /remove worktree/i }));

    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(said));
  });

  test("the first ask never forces", async () => {
    const onRemove = vi.fn().mockResolvedValue({ status: "ok" });
    render(<RemovePiece worktree={piece} plane="/plane" onRemove={onRemove} />);

    await userEvent.click(screen.getByRole("button", { name: /remove worktree/i }));

    expect(onRemove).toHaveBeenCalledWith(
      expect.objectContaining({ force: false, piece: "fix-login", repo: "charter-app" }),
    );
  });

  test("discarding is a second decision, and only offered after the refusal is on screen", async () => {
    const onRemove = vi
      .fn()
      .mockResolvedValueOnce({ status: "error", error: "refusing to remove" })
      .mockResolvedValueOnce({ status: "ok" });
    render(<RemovePiece worktree={piece} plane="/plane" onRemove={onRemove} />);

    // Nothing to discard with until the operator has been told why.
    expect(screen.queryByRole("button", { name: /discard/i })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /remove worktree/i }));
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: /discard it anyway/i }));

    expect(onRemove).toHaveBeenNthCalledWith(2, expect.objectContaining({ force: true }));
  });

  test("a removal that succeeds says so by clearing the refusal", async () => {
    const onRemove = vi
      .fn()
      .mockResolvedValueOnce({ status: "error", error: "refusing to remove" })
      .mockResolvedValueOnce({ status: "ok" });
    const onRemoved = vi.fn();
    render(
      <RemovePiece worktree={piece} plane="/plane" onRemove={onRemove} onRemoved={onRemoved} />,
    );

    await userEvent.click(screen.getByRole("button", { name: /remove worktree/i }));
    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: /discard it anyway/i }));

    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(onRemoved).toHaveBeenCalled();
  });
});
