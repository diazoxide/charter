import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PlaneId, WhatChanged } from "../bindings";
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
