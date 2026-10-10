import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { SearchTab } from "./SearchTab";
import type { FilesSearched, PlaneId, SearchedFile } from "./bindings";
import { searchFromFocus, searchView } from "./contentSearch";
import { forgetProjectThemes } from "./projectTheme";
import { ReferenceChats, type ChatsForReferences } from "./references";
import { settleJump, useJumpAsks, type Pending } from "./fileJump";

/**
 * **The Search view's render states** (FM-8, #1676): what the real-app scenario
 * (`workspace-explorer.e2e.ts`) does not reach on its fixture — a query the core refuses, a
 * branch it could not search, nothing matching, and a page cut short with "Show more". The core
 * is mocked; what it finds on real branches is `content_is_searched_across_branches.rs`'s.
 */

afterEach(() => {
  cleanup();
  clearMocks();
  forgetProjectThemes();
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
function core(
  answer: (query: string) => number | string,
  icons: { picks?: Record<string, string>; themes?: unknown[] } = {},
) {
  const asked: Asked[] = [];
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args: args as Record<string, unknown> });
      if (cmd === "search_files") {
        const said = answer((args as { query: string }).query);
        if (typeof said === "string") throw said;
        return said;
      }
      if (cmd === "project_icons_drawn")
        return icons.picks?.[String((args as { plane: string }).plane)] ?? null;
      if (cmd === "extension_icon_themes") return icons.themes ?? [];
      if (cmd === "reference_into_chat") {
        const { path, lines } = args as { path: string; lines: { first: number } };
        if (path === "locked.rs") throw "locked.rs is not a file this branch offers";
        return { kind: "typed", text: `@${path}#L${lines.first}` };
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

describe("the Search view", () => {
  it("says what is never searched before anything is typed, and asks nothing", () => {
    const asked = core(() => 1);
    draw();

    expect(screen.getByTestId("search-empty")).toHaveTextContent(/never searched/);
    expect(box()).toHaveFocus();
    expect(asked.filter((one) => one.cmd === "search_files")).toEqual([]);
  });

  it("says a search it cannot read is unreadable, and the way out", () => {
    core(() => 1);
    const view = searchView(searchFromFocus(BRANCH, BRANCH.workspace));
    render(<SearchTab plane={PLANE} view={{ ...view, key: "not a search" }} />);

    // The side view is the one place a search is drawn (#1701): there is no tab to close.
    expect(screen.getByText("This search could not be read")).toBeInTheDocument();
    expect(screen.getByText("Show Search again to start a new one.")).toBeInTheDocument();
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

  it("draws each hit's file in its own project's icon theme (#1145)", async () => {
    // beta picks an extension's icon theme that draws Markdown as its own `leaf`.
    const LEAVES = JSON.stringify({
      symbols: {
        leaf: { viewBox: "0 0 16 16", paths: [{ d: "M0 0h16v16H0z", tone: "icon.green" }] },
      },
      extensions: { md: "leaf" },
    });
    const asked = core(() => 2, {
      picks: { [OTHER as unknown as string]: "solarized/Leaves" },
      themes: [{ extension: "solarized", name: "Leaves", text: LEAVES }],
    });
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));
    await told(
      {
        files: [hit("src/a.rs", [3]), hit("docs/b.md", [1]), hit("docs/c.md", [1], OTHER)],
        refused: [],
        ended: "done",
      },
      asked,
    );

    const groups = within(screen.getByRole("listbox", { name: "Search results" })).getAllByRole(
      "group",
    );
    const icon = (group: HTMLElement) =>
      group.querySelector("svg.file-icon")?.getAttribute("data-icon");
    await vi.waitFor(() => expect(groups.map(icon)).toEqual(["rust", "markdown", "leaf"]));
    expect(groups[0].querySelector("svg.file-icon")).toHaveAttribute("aria-hidden", "true");
    const drawn = asked.filter((one) => one.cmd === "project_icons_drawn").map((one) => one.args);
    expect(drawn).toContainEqual({ plane: PLANE, workspace: "web" });
    expect(drawn).toContainEqual({ plane: OTHER, workspace: "web" });
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

  it("says what to try when a finished search found nothing, in place of an empty list", async () => {
    const asked = core(() => 1);
    draw();
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));
    // Still searching: no verdict yet, so nothing says "nothing".
    expect(screen.queryByTestId("search-none")).toBeNull();

    await told({ files: [], refused: [], ended: "done" }, asked);

    // FR-19 (#614): the empty view says what goes there and what to do about it.
    const none = screen.getByTestId("search-none");
    expect(none).toHaveTextContent("Nothing matches “needle”");
    expect(none).toHaveTextContent(/wider scope/);
    expect(screen.queryByRole("listbox", { name: "Search results" })).toBeNull();
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

describe("a search hit into a chat, from the keyboard (#1151)", () => {
  const CHATS: ChatsForReferences["chats"] = [
    { session: 3, name: "steward 3" },
    { session: 5, name: "web 5" },
  ];

  /** The tab, in a window that lends it `chats`, with `files` found for "needle". */
  async function found(files: SearchedFile[], chats = CHATS) {
    const asked = core(() => 1);
    const hand = vi.fn();
    const view = searchView(searchFromFocus(BRANCH, BRANCH.workspace));
    render(
      <ReferenceChats.Provider value={{ plane: PLANE, chats, hand }}>
        <SearchTab plane={PLANE} view={view} />
      </ReferenceChats.Provider>,
    );
    await userEvent.type(box(), "needle");
    await vi.waitFor(() => expect(asked.some((one) => one.cmd === "search_files")).toBe(true));
    await told({ files, refused: [], ended: "done" }, asked);
    const hits = screen.getByRole("listbox", { name: "Search results" });
    hits.focus();
    return { asked, hand, hits };
  }

  it("says its key on the hits", async () => {
    const { hits } = await found([hit("src/a.rs", [3])]);

    expect(hits).toHaveAttribute("aria-keyshortcuts", "Shift+Enter");
  });

  it("opens the chat picker for the hit stepped to with Shift+Enter, and types its line", async () => {
    const { asked, hits } = await found([hit("src/a.rs", [3, 9])]);
    await userEvent.keyboard("{ArrowDown}{Shift>}{Enter}{/Shift}");

    const menu = screen.getByRole("menu", { name: "Add src/a.rs:9 to a chat's context" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((one) => one.textContent)).toEqual(["steward 3", "web 5"]);
    expect(items[0]).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}{Enter}");

    await vi.waitFor(() =>
      expect(status()).toHaveTextContent("Typed @src/a.rs#L9 into web 5. Nothing was sent."),
    );
    expect(asked.filter((one) => one.cmd === "reference_into_chat").map((one) => one.args)).toEqual(
      [
        {
          plane: PLANE,
          ...BRANCH,
          path: "src/a.rs",
          lines: { first: 9, last: 9 },
          session: 5,
        },
      ],
    );
    expect(screen.queryByRole("menu")).toBeNull();
    expect(hits).toHaveFocus();
  });

  it("opens no file on Shift+Enter, which Enter alone does", async () => {
    const jumps: Pending[] = [];
    function Window() {
      useJumpAsks((jump) => {
        jumps.push(jump);
      });
      return null;
    }
    render(<Window />);
    await found([hit("src/a.rs", [3])]);
    await userEvent.keyboard("{Shift>}{Enter}{/Shift}");

    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(jumps).toEqual([]);
    await userEvent.keyboard("{Escape}{Enter}");

    expect(jumps.map((one) => [one.path, one.line])).toEqual([["src/a.rs", 3]]);
    for (const jump of jumps) settleJump(jump.at);
  });

  it("closes the picker on Escape, handing nothing, and gives the keyboard back to the hits", async () => {
    const { asked, hits } = await found([hit("src/a.rs", [3])]);
    await userEvent.keyboard("{Shift>}{Enter}{/Shift}");
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByRole("menu")).toBeNull();
    expect(hits).toHaveFocus();
    expect(asked.some((one) => one.cmd === "reference_into_chat")).toBe(false);
    expect(box()).not.toHaveFocus();
  });

  it("says the core's refusal in the tab's status line", async () => {
    await found([hit("locked.rs", [2])]);
    await userEvent.keyboard("{Shift>}{Enter}{/Shift}{Enter}");

    await vi.waitFor(() =>
      expect(status()).toHaveTextContent("locked.rs is not a file this branch offers"),
    );
  });

  it("refuses a hit from another project without a picker or a question to the core", async () => {
    const { asked, hits } = await found([hit("docs/b.md", [1], OTHER)]);
    await userEvent.keyboard("{Shift>}{Enter}{/Shift}");

    expect(screen.queryByRole("menu")).toBeNull();
    expect(status()).toHaveTextContent(
      "docs/b.md:1 is in another project, so this project's chats cannot take it.",
    );
    expect(hits).toHaveFocus();
    expect(asked.some((one) => one.cmd === "reference_into_chat")).toBe(false);
  });

  it("says no chat is open rather than opening an empty picker", async () => {
    const { hits } = await found([hit("src/a.rs", [3])], []);
    await userEvent.keyboard("{Shift>}{Enter}{/Shift}");

    expect(screen.queryByRole("menu")).toBeNull();
    expect(status()).toHaveTextContent("No chat is open in this project to add src/a.rs:3 to.");
    expect(hits).toHaveFocus();
  });
});
