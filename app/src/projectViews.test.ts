import { afterEach, beforeEach, describe, expect, it } from "vitest";
import {
  forgetProjectViews,
  keepFacet,
  keptFacet,
  onProjectViews,
  projectsTouched,
} from "./projectViews";
import { GLOBAL, usingTheDefaultLayout } from "./windowprefs";

/** A layout file holding `document`, handed as the initialization script hands it. */
const put = (document: unknown) => {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/cfg/layout.json", found: true, document, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
};

beforeEach(() => forgetProjectViews());
afterEach(() => Reflect.deleteProperty(globalThis, GLOBAL));

describe("what a view keeps for each project in layout.json v2 (B-11)", () => {
  it("is read from the project's entry, and a project the file does not name keeps nothing", () => {
    put({
      version: 2,
      regions: [],
      projects: { "/one": { regions: [], chats: { scope: "all" } } },
    });

    expect(keptFacet("/one", "chats")).toEqual({ scope: "all" });
    expect(keptFacet("/two", "chats")).toBeUndefined();
    expect(keptFacet(undefined, "chats")).toBeUndefined();
  });

  it("is what this launch kept once it kept something, and tells who listens", () => {
    put({ version: 2, regions: [], projects: { "/one": { chats: { scope: "all" } } } });
    const told: string[] = [];
    const stop = onProjectViews((project) => told.push(project));

    keepFacet("/one", "chats", { scope: "tab" });

    expect(keptFacet("/one", "chats")).toEqual({ scope: "tab" });
    expect(told).toEqual(["/one"]);
    stop();
  });

  it("tells nobody when what is kept is what was there", () => {
    put({ version: 2, regions: [], projects: { "/one": { chats: { scope: "all" } } } });
    const told: string[] = [];
    const stop = onProjectViews((project) => told.push(project));

    keepFacet("/one", "chats", { scope: "all" });
    keepFacet("/two", "chats", undefined);

    expect(told).toEqual([]);
    stop();
  });

  it("lists the projects this launch kept something in, the one kept in last, last", () => {
    keepFacet("/one", "chats", { scope: "tab" });
    keepFacet("/two", "explorer", { folded: ["a/svc"] });
    keepFacet("/one", "explorer", { folded: ["a/svc"] });

    expect(projectsTouched()).toEqual(["/two", "/one"]);
  });

  it("cannot reach a prototype through a project's name", () => {
    put({ version: 2, regions: [], projects: { "/one": {} } });

    expect(keptFacet("constructor", "chats")).toBeUndefined();
    expect(keptFacet("toString", "chats")).toBeUndefined();
    expect(keptFacet("/one", "constructor" as "chats")).toBeUndefined();
  });

  it("is nothing once the default layout is used, for the file's projects and this launch's", () => {
    put({ version: 2, regions: [], projects: { "/one": { chats: { scope: "all" } } } });
    keepFacet("/two", "chats", { scope: "tab" });

    usingTheDefaultLayout();

    expect(keptFacet("/one", "chats")).toBeUndefined();
    expect(keptFacet("/two", "chats")).toBeUndefined();
    expect(projectsTouched()).toEqual([]);
  });
});
