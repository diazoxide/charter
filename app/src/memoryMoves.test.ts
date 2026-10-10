import { describe, expect, it } from "vitest";
import type { MemoryScope } from "./bindings";
import {
  PUBLISHED_WITH_THE_PROJECT,
  movesPublishSaid,
  publishedSaid,
  publishedStore,
} from "./memoryMoves";

/**
 * **Who reads a memory once it has moved** (#1190): persona and shared memory are published
 * with the project, and so is a LIVE workspace's journal. A LOCAL workspace's journal is not.
 * The tab's Move help and the Move rows are both worded here.
 */
const alpha: MemoryScope = { kind: "workspace", name: "alpha" };
const beta: MemoryScope = { kind: "workspace", name: "beta" };
const gamma: MemoryScope = { kind: "workspace", name: "gamma" };
const steward: MemoryScope = { kind: "persona", name: "steward" };
const shared: MemoryScope = { kind: "shared" };

describe("a move's audience (#1190)", () => {
  it("is the project's for a persona's memory, shared memory and a LIVE workspace's journal", () => {
    expect([steward, shared, alpha, beta].map((to) => publishedStore(to, ["beta"]))).toEqual([
      true,
      true,
      false,
      true,
    ]);
  });

  it("says so on a row into a LIVE journal, and nothing on one into a LOCAL journal", () => {
    expect(publishedSaid(beta, ["beta"])).toBe(
      "beta is LIVE, so its journal is published with the project.",
    );
    expect(publishedSaid(alpha, ["beta"])).toBeUndefined();
    expect(publishedSaid(shared, [])).toBe(PUBLISHED_WITH_THE_PROJECT);
    expect(publishedSaid(steward, [])).toBe(PUBLISHED_WITH_THE_PROJECT);
  });

  it("names the LIVE workspaces among the tab's stores in its help, and only those", () => {
    expect(movesPublishSaid([steward, shared, alpha], ["beta"])).toBe(PUBLISHED_WITH_THE_PROJECT);
    expect(movesPublishSaid([steward, shared, alpha, beta], ["beta"])).toBe(
      "Persona and shared memory, and the journal of beta, which is LIVE, are published with the project.",
    );
    expect(movesPublishSaid([alpha, beta, gamma], ["alpha", "beta", "gamma"])).toBe(
      "Persona and shared memory, and the journals of alpha, beta and gamma, which are LIVE, are published with the project.",
    );
  });
});
