import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { EXPLORER_MOST_BYTES, keepTreeFolds, keptTreeFolds } from "./explorerFolds";
import { forgetProjectViews, keepFacet, keptFacet } from "./projectViews";
import { GLOBAL } from "./windowprefs";

const put = (document: unknown) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/cfg/layout.json", found: true, document, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};
const set = (...keys: string[]): ReadonlySet<string> => new Set(keys);
const NONE = { folded: set(), opened: set(), shut: set() };

beforeEach(() => forgetProjectViews());
afterEach(() => Reflect.deleteProperty(globalThis, GLOBAL));

describe("the rows folded and opened in Explorer's trees, kept per project (#1686)", () => {
  it("are none for a project that kept none", () => {
    expect(keptTreeFolds("/one")).toEqual(NONE);
    expect(keptTreeFolds(undefined)).toEqual(NONE);
  });

  it("come back as they were kept, each project its own", () => {
    keepTreeFolds("/one", { folded: set("alpha/svc"), opened: set("files:alpha"), shut: set() });

    expect(keptTreeFolds("/one")).toEqual({
      folded: set("alpha/svc"),
      opened: set("files:alpha"),
      shut: set(),
    });
    expect(keptTreeFolds("/two")).toEqual(NONE);
  });

  it("are read from the file, and what is not a list of names is none", () => {
    put({
      version: 2,
      regions: [],
      projects: {
        "/one": { explorer: { folded: ["alpha/svc", 7], opened: "files:x", shut: ["s"] } },
      },
    });

    expect(keptTreeFolds("/one")).toEqual({
      folded: set("alpha/svc"),
      opened: set(),
      shut: set("s"),
    });
  });

  it("keep the sections folded beside them, in the one explorer entry", () => {
    keepFacet("/one", "explorer", { closed: ["files"] });

    keepTreeFolds("/one", { folded: set("alpha/svc"), opened: set(), shut: set() });

    expect(keptFacet("/one", "explorer")).toEqual({ closed: ["files"], folded: ["alpha/svc"] });
  });

  it("leave nothing in the file when every row is as it starts", () => {
    keepTreeFolds("/one", { folded: set("alpha/svc"), opened: set(), shut: set() });

    keepTreeFolds("/one", NONE);

    expect(keptFacet("/one", "explorer")).toBeUndefined();
  });

  it("are held to the project's budget, letting go of the folders opened longest ago first", () => {
    const opened = Array.from({ length: 200 }, (_, at) => `files:alpha\u0000svc/:dir-${at}`);

    keepTreeFolds("/one", { folded: set("alpha/svc"), opened: new Set(opened), shut: set() });

    const kept = keptFacet("/one", "explorer");
    expect(new TextEncoder().encode(JSON.stringify(kept, null, 2)).length).toBeLessThanOrEqual(
      EXPLORER_MOST_BYTES,
    );
    const back = keptTreeFolds("/one");
    expect(back.folded).toEqual(set("alpha/svc"));
    expect(back.opened.has(opened[199])).toBe(true);
    expect(back.opened.has(opened[0])).toBe(false);
  });
});
