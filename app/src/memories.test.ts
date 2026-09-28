import { describe, expect, it } from "vitest";
import { draftView, memoryKey, memoryRefOf, memoryView, isMemory, DRAFT } from "./memories";

describe("a memory's key", () => {
  it("is spelled as the core spells it, for each store", () => {
    // The literals `memories::tests::a_memory_is_keyed_by_its_store_and_its_slug` spells too:
    // a persona's memory row arrives carrying `memory.open:<this>`.
    expect(memoryKey({ scope: { kind: "workspace", name: "alpha" }, slug: "x" })).toBe(
      "workspace/alpha/x",
    );
    expect(memoryKey({ scope: { kind: "persona", name: "steward" }, slug: "x" })).toBe(
      "persona/steward/x",
    );
    expect(memoryKey({ scope: { kind: "shared" }, slug: "x" })).toBe("shared/x");
  });

  it("reads back to the store and slug it was made from", () => {
    for (const key of [
      "workspace/alpha/20260928-160505-si-9b",
      "persona/steward/a.b",
      "shared/x",
    ]) {
      const ref = memoryRefOf(key);
      expect(ref && memoryKey(ref)).toBe(key);
    }
  });

  it("names nothing when it is not a memory's", () => {
    for (const key of [
      "",
      "shared",
      "shared/",
      "persona/steward",
      "persona//x",
      "vault/x/y",
      "a/b/c/d",
    ]) {
      expect(memoryRefOf(key), key).toBeUndefined();
    }
  });

  it("gives a new memory a tab of its own per store, which no slug can be", () => {
    expect(draftView({ kind: "shared" })).toEqual({ from: null, view: "memory", key: "shared/+" });
    expect(DRAFT).not.toMatch(/^[A-Za-z0-9]/);
    expect(isMemory(memoryView({ scope: { kind: "shared" }, slug: "x" }))).toBe(true);
    expect(isMemory({ from: "ext", view: "memory", key: "shared/x" })).toBe(false);
  });
});
