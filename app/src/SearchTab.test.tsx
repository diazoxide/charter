import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { SearchTab } from "./SearchTab";
import type { FilesSearched, PlaneId, SearchedFile } from "./bindings";
import { searchFromFocus, searchView } from "./contentSearch";

/**
 * **The Search tab's render states** (FM-8): what the real-app scenario
 * (`workspace-explorer.e2e.ts`) does not reach on its fixture — a query the core refuses, a
 * branch it could not search, nothing matching, and a page cut short with "Show more". The core
 * is mocked; what it finds on real branches is `content_is_searched_across_branches.rs`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/projects/alpha" as unknown as PlaneId;
const OTHER = "/projects/beta" as unknown as PlaneId;
const BRANCH = { workspace: "web", repo: "svc", piece: "fix-login" };

const hit = (path: string, numbers: number[], plane: PlaneId = PLANE): SearchedFile => ({
  plane,
  ...BRANCH,
  path,
  count: numbers.length,
  lines: numbers.map((number) => ({
    number,
    clipped: false,
    parts: [
      { text: "let ", hit: false },
      { text: "needle", hit: true },
      { text: " = 1;", hit: false },
    ],
  })),
});

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: `search_files` answers `answer(query)` — a branch count, or a refusal — and every
 *  command is recorded. */
function core(answer: (query: string) => number | string) {
  const asked: Asked[] = [];
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args: args as Record<string, unknown> });
      if (cmd === "search_files") {
        const said = answer((args as { query: string }).query);
        if (typeof said === "string") throw said;
        return said;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  return asked;
}

/** The run the tab asked for last. */
function lastRun(asked: Asked[]): { id: number; run: number } {
  const runs = asked.filter((one) => one.cmd === "search_files");
  const { id, run } = runs[runs.length - 1].args as { id: number; run: number };
  return { id, run };
}

async function told(
  batch: Omit<FilesSearched, "id" | "run" | "unsearched"> & { unsearched?: string[] },
  asked: Asked[],
) {
  await act(() => emit("files-searched", { unsearched: [], ...lastRun(asked), ...batch }));
}

function draw() {
  const view = searchView(searchFromFocus(BRANCH, BRANCH.workspace));
  return render(<SearchTab plane={PLANE} view={view} />);
}

const box = () => screen.getByRole("searchbox", { name: "Search the files" });
const status = () => screen.getByRole("status", { name: "Search progress" });

describe("the Search tab", () => {
  it("says what is never searched before anything is typed, and asks nothing", () => {
    const asked = core(() => 1);
    draw();

    expect(screen.getByTestId("search-empty")).toHaveTextContent(/never searched/);
    expect(box()).toHaveFocus();
    expect(asked.filter((one) => one.cmd === "search_files")).toEqual([]);
  });

  it("asks the core once the query is still, over the branch it was opened on", async () => {
    const asked = core(() => 1);
    draw();

    await userEvent.type(box(), "needle");

    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "search_files")).toHaveLength(1),
    );
    const { args } = asked.filter((one) => one.cmd === "search_files")[0];
    expect(args.query).toBe("needle");
    expect(args.scope).toEqual({ kind: "branch", plane: PLANE, ...BRANCH });
    expect(args.options).toEqual({ regex: false, matchCase: false, wholeWord: false });
  });

  it("draws a query the core refuses as its sentence", async () => {
    core(() => "regex parse error: unclosed group");
    draw();

    await userEvent.click(screen.getByRole("button", { name: "Regular expression" }));
    await userEvent.type(box(), "open(");

    expect(await screen.findByTestId("search-trouble")).toHaveTextContent("unclosed group");
  });

  it("groups hits project → branch → file with their lines and counts as they stream in", async () => {
    const asked = core(() => 2);
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));

    await told({ files: [hit("src/a.rs", [3, 9])], refused: [], ended: null }, asked);
    expect(status()).toHaveTextContent("2 matching lines in 1 file so far");
    await told({ files: [hit("docs/b.md", [1], OTHER)], refused: [], ended: "done" }, asked);

    const hits = screen.getByRole("listbox", { name: "Search results" });
    const groups = within(hits).getAllByRole("group");
    expect(groups.map((one) => one.getAttribute("aria-label"))).toEqual([
      "a.rs, src · fix-login · alpha",
      "b.md, docs · fix-login · beta",
    ]);
    expect(groups[0]).toHaveTextContent("alpha");
    expect(groups[0]).toHaveTextContent("fix-login · web");
    const lines = within(groups[0]).getAllByRole("option");
    expect(lines.map((one) => one.textContent)).toEqual(["3let needle = 1;", "9let needle = 1;"]);
    expect(within(lines[0]).getByText("needle").tagName).toBe("MARK");
    expect(status()).toHaveTextContent("3 matching lines in 2 files");
    expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();
  });

  it("offers Show more when a page stops short, and asks for the same run's next page", async () => {
    const asked = core(() => 1);
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));

    await told({ files: [hit("src/a.rs", [1])], refused: [], ended: "capped" }, asked);
    expect(status()).toHaveTextContent("there may be more");
    await userEvent.click(screen.getByRole("button", { name: "Show more" }));

    const more = asked.filter((one) => one.cmd === "search_files_more");
    expect(more.map((one) => one.args)).toEqual([lastRun(asked)]);
  });

  it("says a branch it could not search, and nothing found", async () => {
    const asked = core(() => 2);
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));

    await told(
      {
        files: [],
        refused: ["svc in workspace 'web' has no branch folder called 'gone'"],
        ended: "done",
      },
      asked,
    );

    expect(status()).toHaveTextContent("No matches");
    expect(status()).toHaveTextContent("no branch folder called 'gone'");
  });

  it("asks again without ending the last run first, so the two can never cross", async () => {
    const asked = core(() => 1);
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "search_files")).toHaveLength(1),
    );
    const before = asked.length;

    await userEvent.type(box(), "s");

    await vi.waitFor(() =>
      expect(asked.filter((one) => one.cmd === "search_files")).toHaveLength(2),
    );
    expect(asked.slice(before).map((one) => one.cmd)).toEqual(["search_files"]);
  });

  it("says each file it did not search, and why", async () => {
    const asked = core(() => 1);
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));

    await told(
      {
        files: [hit("src/a.rs", [1])],
        refused: [],
        unsearched: ["bundle.min.js in fix-login: it has a line longer than 256 KiB"],
        ended: "done",
      },
      asked,
    );

    expect(screen.getByTestId("search-unsearched")).toHaveTextContent(
      "1 file not searched: bundle.min.js in fix-login: it has a line longer than 256 KiB",
    );
  });

  it("drops a batch of an earlier run", async () => {
    const asked = core(() => 1);
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));
    const { id, run } = lastRun(asked);

    await act(() =>
      emit("files-searched", {
        id,
        run: run - 1,
        files: [hit("old.rs", [1])],
        refused: [],
        ended: "done",
      }),
    );

    expect(screen.queryByRole("group", { name: /old\.rs/ })).toBeNull();
  });
});
