import { describe, expect, it } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  it("lets the later of two conflicting utilities win", () => {
    // This is the half that `clsx` alone does not do, and the reason `tailwind-merge` is a
    // dependency: without it the winner is whichever rule Tailwind happened to emit last,
    // which is a fact about the stylesheet rather than about the call.
    expect(cn("p-2", "p-4")).toBe("p-4");
    expect(cn("bg-surface-base", "bg-surface-raised")).toBe("bg-surface-raised");
  });

  it("keeps utilities that do not conflict", () => {
    expect(cn("flex", "items-center", "gap-2")).toBe("flex items-center gap-2");
  });

  it("drops what a condition turned off", () => {
    const hidden = false;
    expect(cn("flex", hidden && "hidden", undefined, null, ["gap-2"])).toBe("flex gap-2");
  });
});
