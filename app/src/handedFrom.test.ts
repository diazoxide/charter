import { describe, expect, it } from "vitest";

import { handedFromNote } from "./handedFrom";

describe("where a handed-off chat came from", () => {
  it("names the parent and its workspace", () => {
    expect(handedFromNote({ name: "steward 3", workspace: "platform-next" })).toBe(
      "↳ from steward 3 · platform-next",
    );
  });

  it("says nothing for a chat no handoff opened", () => {
    expect(handedFromNote(null)).toBeUndefined();
    expect(handedFromNote(undefined)).toBeUndefined();
  });
});
