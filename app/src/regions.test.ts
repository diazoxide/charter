import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  CATALOGUE,
  DEFAULT_ARRANGEMENT,
  forgetThisLaunch,
  inSlots,
  LEGACY_KEY,
  openView,
  REGION_IDS,
  remembered,
  settleLayout,
  shownIn,
  SIDES,
  SLOTS,
  slotSize,
  useArrangement,
} from "./regions";
import { setYourEditor } from "./yourEditor";
import {
  aboutThisMachine,
  GLOBAL,
  sayAboutThisMachine,
  usingTheDefaultLayout,
  type Reading,
} from "./windowprefs";

const PATH = "/home/op/.config/charter/layout.json";

/** Hands the window a layout file, as the initialization script does before anything runs. */
const handed = (layout: Partial<Reading>) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: false, document: null, trouble: null, ...layout },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};
/** A layout file holding `document`. */
const put = (document: unknown) => handed({ found: true, document });
const placed = (arrangement: ReturnType<typeof remembered>, id: string) =>
  arrangement.find((one) => one.id === id);
/** The navigation region's placement, which every arrangement has. */
const navigationOf = (arrangement: ReturnType<typeof remembered>) => {
  const found = placed(arrangement, "navigation");
  if (found === undefined) throw new Error("no navigation region");
  return found;
};

/** Every command the window sent the core, in order, and what the core answers. */
let sent: { cmd: string; args: unknown }[] = [];
let answer: (cmd: string) => unknown = () => null;

beforeEach(() => {
  globalThis.localStorage.clear();
  Reflect.deleteProperty(globalThis, GLOBAL);
  forgetThisLaunch();
  sayAboutThisMachine("layout", undefined);
  sent = [];
  answer = (cmd) => (cmd === "adopt_layout" ? true : null);
  mockIPC((cmd, args) => {
    sent.push({ cmd, args });
    return answer(cmd);
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

describe("the arrangement a window has never been told about", () => {
  it("is ADR 0038's four regions: navigation left, attention right, state along the bottom", () => {
    expect(remembered()).toEqual([
      { id: "navigation", side: "left", order: 0, collapsed: false },
      { id: "aside", side: "right", order: 0, collapsed: false },
      { id: "bottom", side: "bottom", order: 0, collapsed: false },
    ]);
  });

  it("names every region in the catalogue, so no region can exist with nowhere to be", () => {
    expect(DEFAULT_ARRANGEMENT.map((one) => one.id).sort()).toEqual([...REGION_IDS].sort());
  });

  it("puts every slot inside the bounds the slot itself allows", () => {
    // A default size outside its own slot's bounds is a window that lays out somewhere other
    // than where the arrangement says on its very first frame.
    for (const side of SIDES) {
      const size = slotSize(side, inSlots(DEFAULT_ARRANGEMENT)[side]);
      expect(size).toBeGreaterThanOrEqual(SLOTS[side].least);
      expect(size).toBeLessThanOrEqual(SLOTS[side].most);
    }
  });
});

describe("putting a region away", () => {
  it("puts it away and brings it back", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("navigation"));
    expect(placed(result.current.arrangement, "navigation")?.collapsed).toBe(true);

    act(() => result.current.toggle("navigation"));
    expect(placed(result.current.arrangement, "navigation")?.collapsed).toBe(false);
  });

  it("leaves every other region where it was", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("navigation"));

    expect(result.current.arrangement.filter((one) => one.collapsed).map((one) => one.id)).toEqual([
      "navigation",
    ]);
  });

  it("remembers the answer, so a project switch does not bring a region back", () => {
    // The regions are the WINDOW's layout, and a `PlaneView` that is not in front draws
    // nothing at all — so without this, looking at another project and back would undo it.
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("bottom"));

    expect(remembered()).toEqual(result.current.arrangement);
    expect(placed(remembered(), "bottom")?.collapsed).toBe(true);
  });
});

describe("moving a region", () => {
  it("puts it on the side it was moved to, and remembers", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.move("bottom", "right", 1));

    expect(placed(result.current.arrangement, "bottom")).toMatchObject({
      side: "right",
      order: 1,
    });
    expect(placed(remembered(), "bottom")).toMatchObject({ side: "right", order: 1 });
  });

  it("leaves the side it came from empty rather than holding a place for it", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.move("bottom", "right", 1));

    expect(inSlots(result.current.arrangement).bottom).toEqual([]);
  });
});

describe("how big each slot was left", () => {
  it("is remembered, which is what charter-app#141 deferred", () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.resized({ left: 31 }));

    expect(placed(remembered(), "navigation")?.size).toBe(31);
    expect(slotSize("left", inSlots(remembered()).left)).toBe(31);
  });

  it("is the slot's, so every region drawn in it takes the width it was drawn at", () => {
    const { result } = renderHook(() => useArrangement());
    act(() => result.current.move("bottom", "left", 1));

    act(() => result.current.resized({ left: 33 }));

    expect(placed(result.current.arrangement, "navigation")?.size).toBe(33);
    expect(placed(result.current.arrangement, "bottom")?.size).toBe(33);
  });

  it("is not written to a region that was not drawn", () => {
    // A region that is away was not measured. Giving it the slot's width would be charter
    // deciding how wide it should come back, from a drag it had no part in.
    const { result } = renderHook(() => useArrangement());
    act(() => result.current.move("bottom", "left", 1));
    act(() => result.current.toggle("bottom"));

    act(() => result.current.resized({ left: 33 }));

    expect(placed(result.current.arrangement, "bottom")?.size).toBeUndefined();
  });

  it("ignores a zero, because a collapsed slot measures zero and that is not a width", () => {
    const { result } = renderHook(() => useArrangement());
    act(() => result.current.resized({ left: 28 }));

    act(() => result.current.resized({ left: 0 }));

    expect(placed(result.current.arrangement, "navigation")?.size).toBe(28);
  });

  it("falls back on the catalogue until something has been dragged", () => {
    expect(slotSize("right", inSlots(DEFAULT_ARRANGEMENT).right)).toBe(CATALOGUE.aside.size);
  });

  it("is the first region DRAWN in the slot, not the first one placed there", () => {
    // A slot holding a region that is put away and one that is not takes the width of the one
    // the operator can see. Asking the first placed region would size the slot from something
    // that is not on screen, and the width would jump when it came back.
    const slots = inSlots([
      { id: "navigation", side: "left", order: 0, collapsed: true, size: 40 },
      { id: "bottom", side: "left", order: 1, collapsed: false, size: 12 },
      { id: "aside", side: "right", order: 0, collapsed: false },
    ]);

    expect(slotSize("left", slots.left)).toBe(12);
  });
});

describe("a stored arrangement that is not what this build writes", () => {
  it("draws a region the document does not mention", () => {
    // A document written by an older build, or by hand. Losing a region with no way to notice
    // it went is worse than ignoring half a preference.
    put({ regions: [{ id: "navigation", side: "left", order: 0, collapsed: true }] });

    expect(placed(remembered(), "navigation")?.collapsed).toBe(true);
    expect(placed(remembered(), "aside")).toEqual({
      id: "aside",
      side: "right",
      order: 0,
      collapsed: false,
    });
  });

  it("drops a region this build does not have, and keeps the rest", () => {
    put({
      regions: [
        { id: "minimap", side: "left", order: 0, collapsed: false },
        { id: "bottom", side: "right", order: 2, collapsed: false },
      ],
    });

    expect(remembered().map((one) => one.id)).toEqual(["navigation", "aside", "bottom"]);
    expect(placed(remembered(), "bottom")).toMatchObject({ side: "right", order: 2 });
  });

  it("draws a region whose `collapsed` is not an answer", () => {
    // Only `true` puts a region away. `"yes"` is a document charter cannot read, and the
    // failure of a preference must not be the operator losing a region with no way to notice.
    put({ regions: [{ id: "aside", side: "right", order: 0, collapsed: "yes" }] });

    expect(placed(remembered(), "aside")?.collapsed).toBe(false);
  });

  it("uses the default side for a side that is not a side", () => {
    put({ regions: [{ id: "aside", side: "diagonal", order: 0, collapsed: false }] });

    expect(placed(remembered(), "aside")?.side).toBe("right");
  });

  it("uses the default order for an order that is not a number", () => {
    put({ regions: [{ id: "aside", side: "right", order: "first", collapsed: false }] });

    expect(placed(remembered(), "aside")?.order).toBe(0);
  });

  it("ignores a size that is not a percentage a panel could be given", () => {
    for (const size of [-5, 0, 140, "20%", null, Number.NaN]) {
      globalThis.localStorage.clear();
      put({ regions: [{ id: "navigation", side: "left", order: 0, collapsed: false, size }] });

      expect(placed(remembered(), "navigation")?.size).toBeUndefined();
    }
  });

  it("keeps a size that is one", () => {
    put({ regions: [{ id: "navigation", side: "left", order: 0, collapsed: false, size: 12.5 }] });

    expect(placed(remembered(), "navigation")?.size).toBe(12.5);
  });

  it("draws the default arrangement when the document is not one", () => {
    for (const held of [null, [], "a layout", 7, { regions: "none" }]) {
      put(held);

      expect(remembered()).toEqual(DEFAULT_ARRANGEMENT);
    }
  });

  it("skips an entry that is not an object at all", () => {
    put({ regions: [null, "navigation", ["aside"], { id: "bottom", side: "left", order: 3 }] });

    expect(remembered().map((one) => one.id)).toEqual(["navigation", "aside", "bottom"]);
    expect(placed(remembered(), "bottom")).toMatchObject({ side: "left", order: 3 });
  });
});

describe("the layout file", () => {
  it("is what the first frame is drawn from, with nothing fetched", () => {
    put({
      version: 2,
      regions: [{ id: "navigation", side: "right", order: 1, collapsed: false, size: 22 }],
    });

    expect(placed(remembered(), "navigation")).toEqual({
      id: "navigation",
      side: "right",
      order: 1,
      collapsed: false,
      size: 22,
    });
    expect(sent).toEqual([]);
  });

  it("is where every change goes, in the order the window made them", async () => {
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("navigation"));
    act(() => result.current.move("bottom", "right", 1));

    await vi.waitFor(() => expect(sent).toHaveLength(2));
    expect(sent.map((one) => one.cmd)).toEqual(["write_layout", "write_layout"]);
    const last = JSON.parse((sent[1].args as { text: string }).text);
    expect(last).toEqual({
      version: 2,
      regions: [
        { id: "navigation", side: "left", order: 0, collapsed: true },
        { id: "aside", side: "right", order: 0, collapsed: false },
        { id: "bottom", side: "right", order: 1, collapsed: false },
      ],
      // The two text sizes live in the same file (charter-app#283), and a change to the
      // arrangement writes them as they stand.
      text: { window: 14, terminal: 13 },
    });
  });

  it("keeps the text sizes the file held when the arrangement changes", async () => {
    put({ version: 2, regions: [], text: { window: 18, terminal: 11 } });
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("aside"));

    await vi.waitFor(() => expect(sent).toHaveLength(1));
    const kept = JSON.parse((sent[0].args as { text: string }).text);
    expect(kept.text).toEqual({ window: 18, terminal: 11 });
  });

  it("keeps your editor (RC-20) when the arrangement changes", async () => {
    put({ version: 2, regions: [], editor: "zed" });
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("aside"));

    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(JSON.parse((sent[0].args as { text: string }).text).editor).toBe("zed");
  });

  it("is rewritten with your editor when it is chosen", async () => {
    act(() => setYourEditor("idea"));

    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(sent[0].cmd).toBe("write_layout");
    expect(JSON.parse((sent[0].args as { text: string }).text).editor).toBe("idea");
  });

  it("that could not be read is drawn as the default, and the drawer says why and where", async () => {
    handed({ found: true, trouble: `${PATH} is not JSON: expected value at line 1` });

    expect(remembered()).toEqual(DEFAULT_ARRANGEMENT);
    await settleLayout();

    const [said] = aboutThisMachine();
    expect(said.subject).toBe("layout");
    expect(said.detail).toContain("is not JSON");
    expect(said.detail).toContain("default arrangement");
    expect(said.remedy).toContain(PATH);
  });

  it("naming a region this build does not have is drawn without it, and says so", async () => {
    put({ version: 2, regions: [{ id: "minimap", side: "left", order: 0 }] });

    expect(remembered()).toEqual(DEFAULT_ARRANGEMENT);
    await settleLayout();

    expect(aboutThisMachine()).toHaveLength(1);
    expect(aboutThisMachine()[0].detail).toContain('"minimap" is not a region this purlis has');
  });

  it("says nothing when there is nothing wrong with it", async () => {
    put({ version: 2, regions: DEFAULT_ARRANGEMENT });

    await settleLayout();

    expect(aboutThisMachine()).toEqual([]);
  });

  it("that the core would not write is said, and the window keeps what the operator did", async () => {
    answer = (cmd) => {
      if (cmd === "write_layout") throw "purlis will not overwrite a layout it could not read";
      return null;
    };
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("aside"));

    expect(placed(result.current.arrangement, "aside")?.collapsed).toBe(true);
    await vi.waitFor(() => expect(aboutThisMachine()).toHaveLength(1));
    expect(aboutThisMachine()[0].detail).toContain("will not overwrite");
  });

  it("takes back what the drawer said about it once a change has been kept", async () => {
    handed({ found: true, trouble: `${PATH} is not JSON` });
    await settleLayout();
    expect(aboutThisMachine()).toHaveLength(1);
    const { result } = renderHook(() => useArrangement());

    act(() => result.current.toggle("aside"));

    await vi.waitFor(() => expect(aboutThisMachine()).toEqual([]));
  });
});

describe("the arrangement web storage held before the file", () => {
  const legacy = [{ id: "navigation", side: "left", order: 0, collapsed: true }];

  it("is drawn on the first launch that finds no file, and moved into it", async () => {
    globalThis.localStorage.setItem(LEGACY_KEY, JSON.stringify({ regions: legacy }));
    handed({ found: false });

    expect(placed(remembered(), "navigation")?.collapsed).toBe(true);
    await settleLayout();

    expect(sent.map((one) => one.cmd)).toEqual(["adopt_layout"]);
    expect(JSON.parse((sent[0].args as { text: string }).text)).toMatchObject({
      version: 2,
      regions: [{ id: "navigation", collapsed: true }, { id: "aside" }, { id: "bottom" }],
    });
    // Moved, so never read again.
    expect(globalThis.localStorage.getItem(LEGACY_KEY)).toBeNull();
    // And a project opened after the move still gets it, with the key gone.
    expect(placed(remembered(), "navigation")?.collapsed).toBe(true);
  });

  it("is kept where it is when the move fails, so the next launch tries again", async () => {
    globalThis.localStorage.setItem(LEGACY_KEY, JSON.stringify({ regions: legacy }));
    answer = () => {
      throw "the disk is full";
    };

    await settleLayout();

    expect(globalThis.localStorage.getItem(LEGACY_KEY)).not.toBeNull();
    expect(aboutThisMachine()[0].detail).toContain("the disk is full");
  });

  it("is not read at all once there is a file", async () => {
    globalThis.localStorage.setItem(LEGACY_KEY, JSON.stringify({ regions: legacy }));
    const read = vi.spyOn(globalThis.Storage.prototype, "getItem");
    put({ version: 2, regions: [] });

    expect(placed(remembered(), "navigation")?.collapsed).toBe(false);
    await settleLayout();

    expect(read).not.toHaveBeenCalled();
    expect(sent).toEqual([]);
  });

  it("draws the default when the webview will not read it", () => {
    vi.spyOn(globalThis.Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("storage is blocked");
    });

    expect(remembered()).toEqual(DEFAULT_ARRANGEMENT);
  });
});

describe("the arrangement as the slots it draws", () => {
  it("gives every side a list, including one nothing is placed in", () => {
    expect(Object.keys(inSlots(DEFAULT_ARRANGEMENT)).sort()).toEqual([...SIDES].sort());
    expect(inSlots([{ id: "navigation", side: "left", order: 0, collapsed: false }]).right).toEqual(
      [],
    );
  });

  it("sorts a side by order", () => {
    const slots = inSlots([
      { id: "navigation", side: "left", order: 5, collapsed: false },
      { id: "bottom", side: "left", order: 1, collapsed: false },
      { id: "aside", side: "left", order: 3, collapsed: false },
    ]);

    expect(slots.left.map((one) => one.id)).toEqual(["bottom", "aside", "navigation"]);
  });

  it("breaks a tie the same way every launch", () => {
    // Two regions given the same order is a document a person wrote. Leaving the answer to
    // whatever order the array happened to be in would move the window between launches.
    const slots = inSlots([
      { id: "aside", side: "left", order: 0, collapsed: false },
      { id: "navigation", side: "left", order: 0, collapsed: false },
    ]);

    expect(slots.left.map((one) => one.id)).toEqual(["navigation", "aside"]);
  });

  it("counts what is drawn in a slot and not what is placed there", () => {
    const placedThere = [
      { id: "navigation" as const, side: "left" as const, order: 0, collapsed: true },
      { id: "bottom" as const, side: "left" as const, order: 1, collapsed: false },
    ];

    expect(shownIn(placedThere).map((one) => one.id)).toEqual(["bottom"]);
  });
});

/** What the window wrote last, as the file would hold it. */
const lastWritten = () => {
  const writes = sent.filter((one) => one.cmd === "write_layout");
  return JSON.parse((writes[writes.length - 1].args as { text: string }).text);
};

describe("a version 1 layout file, as every purlis before #1673 wrote it", () => {
  // `docs/design-system.md`'s own example, as it stood for version 1: a person who moved the
  // explorer right, swapped the attention region left and put the bottom bar away.
  const DOCUMENTED = {
    version: 1,
    regions: [
      { id: "explorer", side: "right", order: 0, collapsed: false, size: 22 },
      { id: "aside", side: "left", order: 0, collapsed: false },
      { id: "bottom", side: "bottom", order: 0, collapsed: true },
    ],
    text: { window: 15, terminal: 14 },
    editor: "zed",
    chats: { lines: 1, grouped: true, tabbed: true, away: false },
    dismissed: { "/home/me/project": ["pin-dormant:ide"] },
  };

  it("keeps every region where the person put it, the explorer's now the navigation region's", () => {
    put(DOCUMENTED);

    expect(remembered("/home/me/project")).toEqual([
      { id: "navigation", side: "right", order: 0, collapsed: false, size: 22 },
      { id: "aside", side: "left", order: 0, collapsed: false },
      { id: "bottom", side: "bottom", order: 0, collapsed: true },
    ]);
  });

  it("is moved forward without a word in the alerts drawer", async () => {
    put(DOCUMENTED);

    await settleLayout();

    expect(aboutThisMachine()).toEqual([]);
  });

  it("is where every project starts, since version 1 had one arrangement for all of them", () => {
    put(DOCUMENTED);

    expect(remembered("/one")).toEqual(remembered("/two"));
    expect(placed(remembered("/two"), "navigation")).toMatchObject({ side: "right", size: 22 });
  });

  it("opens the left side on Chats, which sat above the explorer in version 1", () => {
    put(DOCUMENTED);

    expect(openView(navigationOf(remembered("/one")))).toBe("chats");
  });

  it("keeps a region the person had put away put away", () => {
    put({
      version: 1,
      regions: [
        { id: "explorer", side: "left", order: 0, collapsed: true, size: 31.5 },
        { id: "aside", side: "right", order: 0, collapsed: false, size: 24 },
        { id: "bottom", side: "bottom", order: 0, collapsed: false },
      ],
    });

    expect(placed(remembered("/one"), "navigation")).toEqual({
      id: "navigation",
      side: "left",
      order: 0,
      collapsed: true,
      size: 31.5,
    });
    expect(placed(remembered("/one"), "aside")?.size).toBe(24);
  });

  it("is written back as version 2 at the first change, keeping what else the file held", async () => {
    put(DOCUMENTED);
    const { result } = renderHook(() => useArrangement("/home/me/project"));

    act(() => result.current.toggle("aside"));

    await vi.waitFor(() => expect(sent).toHaveLength(1));
    const written = lastWritten();
    expect(written.version).toBe(2);
    expect(written.regions.map((one: { id: string }) => one.id)).toEqual([
      "navigation",
      "aside",
      "bottom",
    ]);
    expect(written.text).toEqual({ window: 15, terminal: 14 });
    expect(written.editor).toBe("zed");
    // The dismissals are the core's to keep (NO-2): a window's write never carries them.
    expect(written.dismissed).toBeUndefined();
  });

  it("is moved forward from web storage too, where the oldest builds kept it", () => {
    globalThis.localStorage.setItem(
      LEGACY_KEY,
      JSON.stringify({ regions: [{ id: "explorer", side: "right", order: 0, collapsed: true }] }),
    );

    expect(placed(remembered(), "navigation")).toMatchObject({ side: "right", collapsed: true });
  });
});

describe("each project's own arrangement", () => {
  it("is drawn from the file, and a project the file does not name starts from the machine's", () => {
    put({
      version: 2,
      regions: DEFAULT_ARRANGEMENT,
      projects: {
        "/one": {
          regions: [
            { id: "navigation", side: "left", order: 0, collapsed: true, view: "explorer" },
          ],
        },
      },
    });

    expect(placed(remembered("/one"), "navigation")).toMatchObject({
      collapsed: true,
      view: "explorer",
    });
    expect(remembered("/two")).toEqual(DEFAULT_ARRANGEMENT);
  });

  it("is written under the project, and only the projects this window arranged are sent", async () => {
    put({
      version: 2,
      regions: DEFAULT_ARRANGEMENT,
      projects: { "/other": { regions: DEFAULT_ARRANGEMENT } },
    });
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.toggle("navigation"));

    await vi.waitFor(() => expect(sent).toHaveLength(1));
    const written = lastWritten();
    // The core keeps `/other` from the file (`purlis_core::windowprefs::write_layout`), so a
    // second window arranging it meanwhile keeps what it did.
    expect(Object.keys(written.projects)).toEqual(["/one"]);
    expect(written.projects["/one"].regions[0]).toMatchObject({
      id: "navigation",
      collapsed: true,
    });
  });

  it("is the project's, so arranging one leaves an open project's alone", () => {
    const one = renderHook(() => useArrangement("/one"));
    const two = renderHook(() => useArrangement("/two"));

    act(() => one.result.current.toggle("navigation"));

    expect(placed(two.result.current.arrangement, "navigation")?.collapsed).toBe(false);
    expect(placed(remembered("/one"), "navigation")?.collapsed).toBe(true);
  });

  it("skips a project's entry that is not an arrangement, and says so", async () => {
    put({ version: 2, regions: DEFAULT_ARRANGEMENT, projects: { "/one": "left", "/two": null } });

    expect(remembered("/one")).toEqual(DEFAULT_ARRANGEMENT);
    await settleLayout();

    expect(aboutThisMachine()[0].detail).toContain('"/one"');
  });

  it("cannot reach a prototype through a project's name", () => {
    put(JSON.parse('{"version":2,"regions":[],"projects":{"__proto__":{"regions":[]}}}'));

    expect(remembered("__proto__")).toEqual(DEFAULT_ARRANGEMENT);
    expect(({} as Record<string, unknown>).regions).toBeUndefined();
  });
});

describe("the open view of a side (#1673)", () => {
  const navigation = navigationOf;

  it("is Chats until the person picks another", () => {
    const { result } = renderHook(() => useArrangement("/one"));

    expect(openView(navigation(result.current.arrangement))).toBe("chats");
  });

  it("switches on a pick of another view, and the side stays open", () => {
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.pick("explorer"));

    expect(navigation(result.current.arrangement)).toMatchObject({
      view: "explorer",
      collapsed: false,
    });
  });

  it("puts the side away on a pick of the view already open, and brings it back on the next", () => {
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.pick("chats"));
    expect(navigation(result.current.arrangement).collapsed).toBe(true);

    act(() => result.current.pick("chats"));
    expect(navigation(result.current.arrangement)).toMatchObject({ collapsed: false });
    expect(openView(navigation(result.current.arrangement))).toBe("chats");
  });

  it("brings a side that is away back on the view picked", () => {
    const { result } = renderHook(() => useArrangement("/one"));
    act(() => result.current.toggle("navigation"));

    act(() => result.current.pick("explorer"));

    expect(navigation(result.current.arrangement)).toMatchObject({
      view: "explorer",
      collapsed: false,
    });
  });

  it("is shown, never put away, when a key or the palette asks for it", () => {
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.show("chats"));
    expect(navigation(result.current.arrangement).collapsed).toBe(false);

    act(() => result.current.toggle("navigation"));
    act(() => result.current.show("explorer"));
    expect(navigation(result.current.arrangement)).toMatchObject({
      view: "explorer",
      collapsed: false,
    });
  });

  it("is remembered in the file", async () => {
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.pick("explorer"));

    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(lastWritten().projects["/one"].regions[0].view).toBe("explorer");
  });

  it("falls back on Chats for a view the side does not have, and says so", async () => {
    put({
      version: 2,
      regions: [{ id: "navigation", side: "left", order: 0, collapsed: false, view: "minimap" }],
    });

    expect(openView(navigation(remembered()))).toBe("chats");
    await settleLayout();
    expect(aboutThisMachine()[0].detail).toContain('"minimap"');
  });
});

describe("the open view of the right side (#1678)", () => {
  const aside = (arrangement: ReturnType<typeof remembered>) => {
    const found = placed(arrangement, "aside");
    if (found === undefined) throw new Error("no attention region");
    return found;
  };

  it("is Memory until the person picks another, though Todos is first on the bar", () => {
    const { result } = renderHook(() => useArrangement("/one"));

    expect(CATALOGUE.aside.views).toEqual(["todos", "memory", "personas", "sessions", "vaults"]);
    expect(openView(aside(result.current.arrangement))).toBe("memory");
  });

  it("puts the side away on a pick of Memory while it is open, and opens Todos on a pick of it", () => {
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.pick("memory"));
    expect(aside(result.current.arrangement).collapsed).toBe(true);

    act(() => result.current.pick("todos"));
    expect(aside(result.current.arrangement)).toMatchObject({ view: "todos", collapsed: false });
    // The left side is not touched by a pick on the right.
    expect(navigationOf(result.current.arrangement)).toEqual(DEFAULT_ARRANGEMENT[0]);
  });

  it("opens an extension's panel as a view of its own, and remembers it in the file", async () => {
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.pick("panel:ext/stats/burn", ["panel:ext/stats/burn"]));

    expect(openView(aside(result.current.arrangement), ["panel:ext/stats/burn"])).toBe(
      "panel:ext/stats/burn",
    );
    await vi.waitFor(() => expect(sent).toHaveLength(1));
    expect(lastWritten().projects["/one"].regions[1].view).toBe("panel:ext/stats/burn");
  });

  it("is read back from the file as an extension's panel, and opens on Memory while it is not there", () => {
    put({
      version: 2,
      regions: [{ id: "aside", side: "right", view: "panel:ext/stats/burn" }],
    });

    const remembers = aside(remembered());
    expect(remembers.view).toBe("panel:ext/stats/burn");
    expect(openView(remembers, ["panel:ext/stats/burn"])).toBe("panel:ext/stats/burn");
    // Its extension is not approved in this window: the side opens on its default.
    expect(openView(remembers, [])).toBe("memory");
  });

  it("puts the side away on a pick of the view drawn open, when the remembered one is gone", () => {
    put({
      version: 2,
      regions: [{ id: "aside", side: "right", view: "panel:ext/gone/x" }],
    });
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.pick("memory", []));

    expect(aside(result.current.arrangement).collapsed).toBe(true);
  });

  it("keeps the side's width and whether it is away beside its view, under the project", async () => {
    const { result } = renderHook(() => useArrangement("/one"));

    act(() => result.current.pick("todos"));
    act(() => result.current.resized({ right: 28 }));
    act(() => result.current.pick("todos"));

    await vi.waitFor(() => expect(sent).toHaveLength(3));
    expect(lastWritten().projects["/one"].regions[1]).toEqual({
      id: "aside",
      side: "right",
      order: 0,
      collapsed: true,
      size: 28,
      view: "todos",
    });
    // And it comes back so for the project, read from the file the next launch is handed.
    put(lastWritten());
    expect(aside(remembered("/one"))).toMatchObject({ view: "todos", size: 28, collapsed: true });
  });

  it("refuses a view that is neither the side's nor an extension panel's, and says so", async () => {
    put({ version: 2, regions: [{ id: "aside", side: "right", view: "chats" }] });

    expect(openView(aside(remembered()))).toBe("memory");
    await settleLayout();
    expect(aboutThisMachine()[0].detail).toContain('"chats" is not a view of aside');
  });

  it("is not an extension's panel on the left, which takes none", async () => {
    put({
      version: 2,
      regions: [{ id: "navigation", side: "left", view: "panel:ext/stats/burn" }],
    });

    expect(navigationOf(remembered()).view).toBeUndefined();
  });
});

describe("Use the default layout (#1289), with each project's own (#1673)", () => {
  it("starts every project opened afterwards from the default, the file's and this launch's gone", async () => {
    put({
      version: 2,
      regions: [{ id: "navigation", side: "right", order: 0, collapsed: true }],
      projects: { "/one": { regions: [{ id: "navigation", side: "left", collapsed: true }] } },
    });
    const { result } = renderHook(() => useArrangement("/two"));
    act(() => result.current.pick("explorer"));

    act(() => usingTheDefaultLayout());

    expect(remembered("/one")).toEqual(DEFAULT_ARRANGEMENT);
    expect(remembered("/two")).toEqual(DEFAULT_ARRANGEMENT);
    expect(remembered("/three")).toEqual(DEFAULT_ARRANGEMENT);
  });
});
