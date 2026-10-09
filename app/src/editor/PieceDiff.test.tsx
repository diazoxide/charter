import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { BranchChanged, PlaneId, WhatChanged } from "../bindings";
import { PieceDiffTab } from "./PieceDiff";

const PLANE = "/plane" as unknown as PlaneId;
const CUT = { workspace: "alpha", repo: "svc", piece: "fix-it" };

/** The core, answering `what_changed` with `answer`, or refusing with it when it is a string. */
function core(answer: WhatChanged | string) {
  const asked: string[] = [];
  mockIPC((cmd, args) => {
    if (cmd !== "what_changed") throw new Error(`unexpected ${cmd}`);
    const a = args as Record<string, string>;
    asked.push(`${a.workspace}/${a.repo}/${a.piece}:${a.path}`);
    if (typeof answer === "string") throw answer;
    return answer;
  });
  return asked;
}

const changed = (diff: WhatChanged["diff"]): WhatChanged => ({
  mark: "changed",
  from: null,
  uncommitted: true,
  base: "main",
  diff,
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("what changed in one file (FM-11)", () => {
  it("draws the file against the branch's base, with git's hunks marked", async () => {
    const asked = core(
      changed({
        kind: "text",
        base: "one\ntwo\n",
        head: "one\nTWO\n",
        hunks: [{ oldStart: 2, oldLines: 1, newStart: 2, newLines: 1 }],
      }),
    );
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/lib.rs" />);

    await waitFor(() => expect(document.querySelector(".cm-mergeView")).not.toBeNull());
    const changedLines = [...document.querySelectorAll(".cm-changedLine")].map(
      (line) => line.textContent,
    );
    expect(changedLines).toEqual(["two", "TWO"]);
    expect(screen.getByText("against main")).toBeInTheDocument();
    expect(asked).toEqual(["alpha/svc/fix-it:src/lib.rs"]);
  });

  it("says the core's refusal as a sentence, never as an empty diff", async () => {
    core("'src/same.rs' is not a file this branch changed against its base");
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/same.rs" />);

    expect(await screen.findByTestId("piece-diff-trouble")).toHaveTextContent(
      "'src/same.rs' is not a file this branch changed against its base",
    );
    expect(screen.queryByTestId("merge-viewer")).toBeNull();
  });

  it("says a binary file and a large one in a sentence", async () => {
    core(changed({ kind: "binary" }));
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="logo.png" />);

    expect(await screen.findByTestId("piece-diff-trouble")).toHaveTextContent(
      "logo.png is a binary file, so its lines are not compared",
    );
    cleanup();

    core(changed({ kind: "too-large", bytes: 3 * 1024 * 1024 }));
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="dump.sql" />);

    expect(await screen.findByTestId("piece-diff-trouble")).toHaveTextContent(
      "dump.sql is 3 MiB, past what the comparison draws (2 MiB)",
    );
    expect(screen.queryByTestId("merge-viewer")).toBeNull();
  });

  it("says a change with no line in it rather than drawing two equal sides", async () => {
    core(changed({ kind: "text", base: "same\n", head: "same\n", hunks: [] }));
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="run.sh" />);

    expect(await screen.findByTestId("piece-diff-trouble")).toHaveTextContent(
      "No line of run.sh differs from main",
    );
    expect(screen.queryByTestId("merge-viewer")).toBeNull();
  });

  it("says a pure rename by the name it moved from", async () => {
    core({
      ...changed({ kind: "text", base: "same\n", head: "same\n", hunks: [] }),
      mark: "renamed",
      from: "src/old.rs",
    });
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/new.rs" />);

    expect(await screen.findByTestId("piece-diff-trouble")).toHaveTextContent(
      "Only its name changed: moved from src/old.rs",
    );
    expect(screen.getByTestId("piece-diff-trouble")).not.toHaveTextContent("its mode");
    expect(screen.queryByTestId("merge-viewer")).toBeNull();
  });

  it("says a new empty file is added and empty", async () => {
    core({
      ...changed({ kind: "text", base: "", head: "", hunks: [] }),
      mark: "added",
    });
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/empty.rs" />);

    expect(await screen.findByTestId("piece-diff-trouble")).toHaveTextContent(
      "empty.rs was added, and it is empty",
    );
    expect(screen.getByTestId("piece-diff-trouble")).not.toHaveTextContent("its mode");
  });

  it("offers your editor beside a file it does not draw, as its sentence says", async () => {
    core(changed({ kind: "binary" }));
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="logo.png" />);

    expect(await screen.findByTestId("piece-diff-trouble")).toHaveTextContent(
      "Open in your editor, at the top of this tab",
    );
    expect(
      screen.getByRole("button", { name: "Open in your editor at line 1" }),
    ).toBeInTheDocument();
  });

  it("names where a renamed file came from, and compares again on request", async () => {
    const asked = core({
      ...changed({
        kind: "text",
        base: "a\n",
        head: "b\n",
        hunks: [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1 }],
      }),
      mark: "renamed",
      from: "src/old.rs",
    });
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/new.rs" />);

    expect(await screen.findByText("against main, moved from src/old.rs")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Compare again" }));

    await waitFor(() => expect(asked).toHaveLength(2));
  });
});

describe("what changed, to a screen reader (#1189)", () => {
  it("says the comparison is being read as a status", () => {
    mockIPC(() => new Promise(() => {}));
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/lib.rs" />);

    expect(screen.getByRole("status")).toHaveTextContent("Comparing src/lib.rs…");
  });

  it("says a refusal and a change with no line in it as a status", async () => {
    core("'src/same.rs' is not a file this branch changed against its base");
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/same.rs" />);

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "'src/same.rs' is not a file this branch changed against its base",
      ),
    );
    cleanup();

    core(changed({ kind: "text", base: "same\n", head: "same\n", hunks: [] }));
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="run.sh" />);

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("No line of run.sh differs from main"),
    );
  });

  it("names the merge view after the file and what it is compared against", async () => {
    core(
      changed({
        kind: "text",
        base: "one\n",
        head: "ONE\n",
        hunks: [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1 }],
      }),
    );
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/lib.rs" />);

    expect(
      await screen.findByRole("group", { name: "What changed in src/lib.rs against main" }),
    ).toHaveAttribute("data-testid", "merge-viewer");
    expect(screen.queryByRole("status")).toBeNull();
  });
});

describe("what changed, read again when the branch moves (#1189)", () => {
  const TEXT = changed({
    kind: "text",
    base: "one\n",
    head: "ONE\n",
    hunks: [{ oldStart: 1, oldLines: 1, newStart: 1, newLines: 1 }],
  });

  /** The core, its `what_changed` answered by hand: each ask waits for `answer()`. */
  function slowCore() {
    const waiting: (() => void)[] = [];
    let asks = 0;
    mockIPC(
      (cmd) => {
        if (cmd !== "what_changed") throw new Error(`unexpected ${cmd}`);
        asks += 1;
        return new Promise((done) => waiting.push(() => done(TEXT)));
      },
      { shouldMockEvents: true },
    );
    return {
      asks: () => asks,
      answer: () => act(async () => waiting.splice(0).forEach((done) => done())),
    };
  }

  const moved = (piece: string | null, workspace = "alpha") =>
    act(() =>
      emit("branch-changed", {
        branches: [{ plane: PLANE, workspace, repo: "svc", piece }],
      } satisfies BranchChanged),
    );

  it("reads again when its own branch moved, and keeps the comparison drawn meanwhile", async () => {
    const core = slowCore();
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/lib.rs" />);
    await waitFor(() => expect(core.asks()).toBe(1));
    await core.answer();
    await screen.findByTestId("merge-viewer");

    await moved("fix-it");

    await waitFor(() => expect(core.asks()).toBe(2));
    expect(screen.getByTestId("merge-viewer")).toBeInTheDocument();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("does not read again for another branch", async () => {
    const core = slowCore();
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/lib.rs" />);
    await waitFor(() => expect(core.asks()).toBe(1));
    await core.answer();

    await moved("other");
    await moved(null);
    await moved("fix-it", "beta");
    await act(async () => {
      await new Promise((done) => setTimeout(done, 20));
    });

    expect(core.asks()).toBe(1);
  });

  it("reads once more after a read in flight, however often the branch moved meanwhile", async () => {
    const core = slowCore();
    render(<PieceDiffTab plane={PLANE} cut={CUT} path="src/lib.rs" />);
    await waitFor(() => expect(core.asks()).toBe(1));

    await moved("fix-it");
    await moved("fix-it");
    await moved("fix-it");
    expect(core.asks()).toBe(1);
    await core.answer();

    await waitFor(() => expect(core.asks()).toBe(2));
    await core.answer();
    await act(async () => {
      await new Promise((done) => setTimeout(done, 20));
    });
    expect(core.asks()).toBe(2);
  });
});
