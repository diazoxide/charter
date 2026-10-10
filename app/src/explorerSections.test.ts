import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  closedSections,
  explorerSectionsDocument,
  loadExplorerSections,
  onExplorerSections,
  setSectionOpen,
} from "./explorerSections";
import { forgetThisLaunch } from "./regions";
import {
  aboutThisMachine,
  GLOBAL,
  sayAboutThisMachine,
  usingTheDefaultLayout,
} from "./windowprefs";

const PATH = "/home/op/.config/purlis/layout.json";

const handed = (document: unknown) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: true, document, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};

beforeEach(() => {
  forgetThisLaunch();
  sayAboutThisMachine("explorer", undefined);
});
afterEach(() => {
  Reflect.deleteProperty(globalThis, GLOBAL);
});

describe("Explorer's folded sections in the layout file (#1677)", () => {
  it("are none until the person folds one, and a file without the field says nothing", () => {
    expect(loadExplorerSections({ version: 2, regions: [] })).toEqual({
      closed: new Set(),
      said: [],
    });
    expect(closedSections()).toEqual(new Set());
  });

  it("keeps the sections it knows, and says what it could not read", () => {
    expect(loadExplorerSections({ explorer: { closed: ["files", "outline"] } })).toEqual({
      closed: new Set(["files"]),
      said: [`"explorer.closed" names "outline", which Explorer has no section called`],
    });
    expect(loadExplorerSections({ explorer: { closed: "files" } })).toEqual({
      closed: new Set(),
      said: [`"explorer.closed" "files" is not a list of sections, so every section is open`],
    });
  });

  it("says what it put right in the alerts drawer, with the file's path", () => {
    handed({ version: 2, regions: [], explorer: { closed: ["repos", 7] } });

    expect(closedSections()).toEqual(new Set(["repos"]));
    expect(aboutThisMachine().find((one) => one.subject === "explorer")?.detail).toBe(
      `${PATH}: "explorer.closed" names 7, which Explorer has no section called`,
    );
  });

  it("are every one open again once the default layout is used, and the next write says none", () => {
    handed({ version: 2, regions: [], explorer: { closed: ["files"] } });
    setSectionOpen("workspaces", false);
    const told: number[] = [];
    const stop = onExplorerSections((closed) => told.push(closed.size));

    usingTheDefaultLayout();

    expect(closedSections()).toEqual(new Set());
    expect(explorerSectionsDocument()).toBeUndefined();
    // The file was just moved aside: nothing writes it again for this.
    expect(told).toEqual([]);
    stop();
  });

  it("reads only a list under closed", () => {
    expect(loadExplorerSections({ explorer: ["files"] }).closed).toEqual(new Set());
  });
});
