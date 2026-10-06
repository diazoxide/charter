import { describe, expect, it } from "vitest";
import type { PlaneAnswer } from "./bindings";
import {
  concerns,
  CURATIONS,
  GIT,
  INSTRUCTIONS,
  panelsOf,
  ROOT_PANELS,
  SETTINGS,
  SIDEBAR,
  VIEWS,
} from "./planeChanged";

/**
 * Which answer a change concerns is the core's question (`purlis_core::planechange::answers`,
 * whose tests hold the mapping). The window only names the answer it holds, and these say how a
 * name is matched against what the core sent.
 */
describe("whether what the core says moved concerns a reader of the plane (FD-10)", () => {
  it("concerns every reader when the core could not say what changed", () => {
    expect(concerns(null, SIDEBAR)).toBe(true);
    expect(concerns(undefined, panelsOf("alpha"))).toBe(true);
  });

  it("concerns nobody when nothing changed", () => {
    expect(concerns([], SIDEBAR)).toBe(false);
    expect(concerns([], VIEWS)).toBe(false);
  });

  it("concerns a reader whose answer the core named, and no other", () => {
    const told: PlaneAnswer[] = [{ answer: "sidebar" }, { answer: "git" }];
    expect(concerns(told, SIDEBAR)).toBe(true);
    expect(concerns(told, GIT)).toBe(true);
    expect(concerns(told, INSTRUCTIONS)).toBe(false);
    expect(concerns(told, CURATIONS)).toBe(false);
    expect(concerns(told, SETTINGS)).toBe(false);
    expect(concerns(told, ROOT_PANELS)).toBe(false);
    expect(concerns(told, panelsOf("alpha"))).toBe(false);
  });

  it("keeps a workspace's panels to that workspace's, or every workspace's", () => {
    const beta: PlaneAnswer[] = [{ answer: "panels", workspace: "beta" }];
    expect(concerns(beta, panelsOf("beta"))).toBe(true);
    expect(concerns(beta, panelsOf("alpha"))).toBe(false);
    const every: PlaneAnswer[] = [{ answer: "panels", workspace: null }];
    expect(concerns(every, panelsOf("alpha"))).toBe(true);
    expect(concerns(every, panelsOf("beta"))).toBe(true);
  });

  it("tells the git readers what auto-save did, and nobody else", () => {
    const saved: PlaneAnswer[] = [{ answer: "git" }];
    expect(concerns(saved, GIT)).toBe(true);
    for (const other of [
      SIDEBAR,
      INSTRUCTIONS,
      CURATIONS,
      SETTINGS,
      ROOT_PANELS,
      VIEWS,
      panelsOf("alpha"),
    ])
      expect(concerns(saved, other)).toBe(false);
  });
});
