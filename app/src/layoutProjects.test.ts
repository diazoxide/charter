import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  DEFAULT_ARRANGEMENT,
  forgetThisLaunch,
  MOST_PROJECTS,
  PROJECTS_MOST_BYTES,
  remembered,
  settleLayout,
  useArrangement,
} from "./regions";
import { keepFacet } from "./projectViews";
import { closedSections } from "./explorerSections";
import { aboutThisMachine, GLOBAL, sayAboutThisMachine } from "./windowprefs";

/**
 * **Each project's entry in `layout.json` v2 holds what its views keep** (B-11, #1686, #1696):
 * beside its arrangement, the facets `projectViews.ts` keeps, written by `regions.ts`.
 */

const PATH = "/home/op/.config/purlis/layout.json";
const put = (document: unknown) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: true, document, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};

let sent: { cmd: string; args: unknown }[] = [];
const writes = () => sent.filter((one) => one.cmd === "write_layout");
const lastWritten = () => {
  const all = writes();
  return JSON.parse((all[all.length - 1].args as { text: string }).text);
};

beforeEach(() => {
  globalThis.localStorage.clear();
  Reflect.deleteProperty(globalThis, GLOBAL);
  forgetThisLaunch();
  sayAboutThisMachine("layout", undefined);
  sent = [];
  mockIPC((cmd, args) => {
    sent.push({ cmd, args });
    return null;
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

/**
 * A version 2 file as #1673, #1677 and #1678 wrote it, before any view kept anything per
 * project: the machine's arrangement, two projects' own, and Explorer's folded sections for the
 * machine.
 */
const WRITTEN_BEFORE = {
  version: 2,
  regions: [
    { id: "navigation", side: "left", order: 0, collapsed: false, size: 18, view: "explorer" },
    { id: "aside", side: "right", order: 0, collapsed: false, view: "todos" },
  ],
  projects: {
    "/home/me/one": {
      regions: [
        { id: "navigation", side: "left", order: 0, collapsed: true, view: "chats" },
        { id: "aside", side: "right", order: 0, collapsed: false },
      ],
    },
    "/home/me/two": { regions: [{ id: "aside", side: "right", order: 0, collapsed: true }] },
  },
  text: { window: 15, terminal: 14 },
  chats: { grouped: true, tabbed: false, away: true },
  explorer: { closed: ["workspaces"] },
  dismissed: { "/home/me/one": ["pin-dormant:ide"] },
};

describe("a version 2 file written before the views kept anything per project", () => {
  it("loads as it did: every arrangement, and the machine's folded sections in every project", async () => {
    put(WRITTEN_BEFORE);

    expect(remembered("/home/me/one")).toEqual([
      { id: "navigation", side: "left", order: 0, collapsed: true, view: "chats" },
      { id: "aside", side: "right", order: 0, collapsed: false },
    ]);
    expect(remembered("/home/me/three")).toEqual([
      { id: "navigation", side: "left", order: 0, collapsed: false, size: 18, view: "explorer" },
      { id: "aside", side: "right", order: 0, collapsed: false, view: "todos" },
    ]);
    expect(closedSections("/home/me/one")).toEqual(new Set(["workspaces"]));
    expect(closedSections("/home/me/three")).toEqual(new Set(["workspaces"]));
    await settleLayout();
    expect(aboutThisMachine()).toEqual([]);
    expect(writes()).toEqual([]);
  });

  it("is moved forward at the first change, with nothing it held lost", async () => {
    put(WRITTEN_BEFORE);

    act(() => keepFacet("/home/me/two", "chats", { scope: "all" }));

    await vi.waitFor(() => expect(writes()).toHaveLength(1));
    const written = lastWritten();
    expect(written.version).toBe(2);
    expect(written.regions).toEqual(WRITTEN_BEFORE.regions);
    expect(written.explorer).toEqual({ closed: ["workspaces"] });
    // Only the project this window changed is sent; the core keeps `/home/me/one` from the file.
    expect(written.projects).toEqual({
      "/home/me/two": {
        regions: [
          { id: "navigation", side: "left", order: 0, collapsed: false },
          { id: "aside", side: "right", order: 0, collapsed: true },
        ],
        chats: { scope: "all" },
      },
    });
  });
});

describe("a version 1 file", () => {
  it("takes what a view keeps for a project, and goes forward to version 2 with its arrangement", async () => {
    // Version 1's id for the navigation region.
    const v1 = [{ id: "explorer", side: "right", order: 0, collapsed: true }];
    put({ version: 1, regions: v1, explorer: { closed: ["workspaces"] } });

    act(() => keepFacet("/p", "explorer", { closed: ["files"] }));

    await vi.waitFor(() => expect(writes()).toHaveLength(1));
    const written = lastWritten();
    expect(written.version).toBe(2);
    expect(written.regions).toContainEqual(
      expect.objectContaining({ id: "navigation", side: "right", collapsed: true }),
    );
    expect(written.projects).toEqual({ "/p": { explorer: { closed: ["files"] } } });
    expect(closedSections("/p")).toEqual(new Set(["files"]));
    expect(closedSections("/q")).toEqual(new Set(["workspaces"]));
    expect(aboutThisMachine()).toEqual([]);
  });
});

describe("a project's entry", () => {
  it("is written with what its views keep, and no arrangement while it has none of its own", async () => {
    act(() => keepFacet("/one", "chats", { scope: "tab" }));

    await vi.waitFor(() => expect(writes()).toHaveLength(1));
    expect(lastWritten().projects).toEqual({ "/one": { chats: { scope: "tab" } } });
  });

  it("with no arrangement starts from the machine's, and says nothing about it", async () => {
    put({
      version: 2,
      regions: [{ id: "navigation", side: "left", order: 0, collapsed: true }],
      projects: { "/one": { chats: { scope: "tab" } } },
    });

    expect(remembered("/one")[0]).toMatchObject({ id: "navigation", collapsed: true });
    await settleLayout();
    expect(aboutThisMachine()).toEqual([]);
  });

  it("keeps what its views kept when its arrangement changes", async () => {
    put({
      version: 2,
      regions: DEFAULT_ARRANGEMENT,
      projects: { "/one": { regions: DEFAULT_ARRANGEMENT, explorer: { folded: ["a/svc"] } } },
    });
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.toggle("navigation"));

    await vi.waitFor(() => expect(writes()).toHaveLength(1));
    const entry = lastWritten().projects["/one"];
    expect(entry.explorer).toEqual({ folded: ["a/svc"] });
    expect(entry.regions[0]).toMatchObject({ id: "navigation", collapsed: true });
  });

  it("still says a regions field that is not a list", async () => {
    put({ version: 2, regions: [], projects: { "/one": { regions: "left" } } });

    await settleLayout();

    expect(aboutThisMachine()[0].detail).toContain('"/one"');
  });
});

describe("the projects' budget in the file", () => {
  it("holds what is sent to its bytes, letting go of the projects changed longest ago first", async () => {
    // Each project keeps a little under 2 KiB, so 40 of them are well past the budget.
    const big = Array.from({ length: 40 }, (_, at) => `ws/repo-${String(at).padStart(30, "0")}`);
    for (let at = 0; at < 40; at += 1) keepFacet(`/p/${at}`, "explorer", { folded: big });

    await vi.waitFor(() => expect(writes()).toHaveLength(40));
    const projects = lastWritten().projects as Record<string, unknown>;
    const bytes = new TextEncoder().encode(JSON.stringify(projects, null, 2)).length;
    expect(bytes).toBeLessThanOrEqual(PROJECTS_MOST_BYTES);
    const names = Object.keys(projects);
    expect(names).toContain("/p/39");
    expect(names).not.toContain("/p/0");
    // Oldest first, as the core reads its order.
    expect(names[names.length - 1]).toBe("/p/39");
  });
});

describe("the projects' bounds", () => {
  it("are the ones the core holds the file to", () => {
    // The core never lets go of a project the window sent: a window sending more than the core
    // keeps would grow the file past what the next launch reads.
    const core = readFileSync(
      join(process.cwd(), "..", "crates", "purlis-core", "src", "windowprefs.rs"),
      "utf8",
    );
    expect(/pub const MOST_PROJECTS: usize = (\d+);/.exec(core)?.[1]).toBe(String(MOST_PROJECTS));
    expect(/pub const MAX_BYTES: u64 = 64 \* 1024;/.test(core)).toBe(true);
    expect(/pub const PROJECTS_MOST_BYTES: u64 = MAX_BYTES \* 3 \/ 8;/.test(core)).toBe(true);
    expect(PROJECTS_MOST_BYTES).toBe((64 * 1024 * 3) / 8);
  });
});
