import { afterEach, describe, expect, test } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { WorktreeMark } from "./Worktree";
import type { ChatWorktree } from "./bindings";

// This project does not run vitest with `globals`, so nothing unmounts a render on its own:
// every test file here registers this, and without it a later test sees the earlier one's
// DOM still on the page.
afterEach(cleanup);

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
    // Since M1.x this state has a way out that does not involve another binary: starting a
    // chat here writes the layer, or refuses with a sentence. A label that only names the
    // hole leaves the operator with nowhere to go.
    expect(label.title).toMatch(/starting a chat/i);
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
