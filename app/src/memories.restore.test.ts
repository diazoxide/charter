import { describe, expect, it } from "vitest";
import { unnumbered } from "./memories";

describe("the name a numbered archive restores under", () => {
  it("drops the number archiving added when the name was taken", () => {
    // `archive_one` (memstore.rs) adds `-2` to a name `archive/` already holds.
    expect(unnumbered("freeze-2", ["freeze", "freeze-2"])).toBe("freeze");
    expect(unnumbered("freeze-2.md", ["freeze.md", "freeze-2.md"])).toBe("freeze");
  });

  it("undoes the whole chain the core's numbering writes for a third", () => {
    // The third `freeze` is numbered from the second's name: `freeze-2-3`.
    expect(unnumbered("freeze-2-3", ["freeze", "freeze-2", "freeze-2-3"])).toBe("freeze");
  });

  it("leaves a number the core never writes alone", () => {
    const held = ["release", "release-2026", "step", "step-1", "step-3", "step-02", "a-2-4"];
    expect(unnumbered("release-2026", held)).toBeUndefined();
    expect(unnumbered("step-1", held)).toBeUndefined();
    expect(unnumbered("step-3", held)).toBeUndefined();
    expect(unnumbered("step-02", held)).toBeUndefined();
    expect(unnumbered("a-2-4", held)).toBeUndefined();
    expect(unnumbered("freeze", ["freeze"])).toBeUndefined();
    expect(unnumbered("-2", ["-2"])).toBeUndefined();
  });

  it("is only offered while the archive holds the name it was numbered away from", () => {
    // A memory its writer named `step-2` was never numbered: nothing called `step` was archived.
    expect(unnumbered("step-2", ["step-2"])).toBeUndefined();
    expect(unnumbered("freeze-2-3", ["freeze-2", "freeze-2-3"])).toBeUndefined();
  });
});
