import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { RepoInstructionsTab } from "./RepoInstructionsTab";
import type { InstructionFile } from "./bindings";

/**
 * **A workspace whose repos hold no agent instructions says so** (DS-3 #626, FR-19 #614): the
 * tab's empty state names the files it looks for and reads again on a press, instead of an
 * empty list under a sentence that says the repo has some.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const AGENTS: InstructionFile = {
  repo: "svc",
  file: "AGENTS.md",
  text: "Run make check.\n",
  standing: { kind: "offered", caution: null },
};

/** The core: `repo_instructions` answers each of `reads` in turn, the last one again after. */
function core(reads: InstructionFile[][]) {
  let read = 0;
  const asked = vi.fn();
  mockIPC((cmd) => {
    if (cmd !== "repo_instructions") return null;
    asked();
    return reads[Math.min(read++, reads.length - 1)];
  });
  return asked;
}

describe("a workspace whose repos hold no agent instructions", () => {
  it("says what the tab looks for, and offers nothing to add", async () => {
    core([[]]);
    render(<RepoInstructionsTab plane={PLANE} workspace="alpha" onClose={vi.fn()} />);

    const empty = await screen.findByTestId("repo-instructions-empty");
    expect(
      within(empty).getByText("No instructions for AI agents in this workspace's repos"),
    ).toBeInTheDocument();
    expect(empty).toHaveTextContent(/CLAUDE\.md/);
    expect(empty).toHaveTextContent(/AGENTS\.md/);
    expect(screen.queryByRole("button", { name: "Add to memory" })).toBeNull();
    expect(screen.queryByText(/This repo has instructions for AI agents/)).toBeNull();
  });

  it("reads the repos again on Read again, and offers a file added since", async () => {
    const asked = core([[], [AGENTS]]);
    render(<RepoInstructionsTab plane={PLANE} workspace="alpha" onClose={vi.fn()} />);
    const empty = await screen.findByTestId("repo-instructions-empty");
    const before = asked.mock.calls.length;

    await userEvent.click(within(empty).getByRole("button", { name: "Read again" }));

    expect(await screen.findByText("svc/AGENTS.md")).toBeInTheDocument();
    expect(asked.mock.calls.length).toBeGreaterThan(before);
    expect(screen.queryByTestId("repo-instructions-empty")).toBeNull();
  });
});
