import { describe, expect, it } from "vitest";
import type { PlaneChange } from "./bindings";
import {
  concerns,
  PLANE_SHAPE,
  ROOT_PANELS,
  SETTINGS,
  SIDEBAR,
  workspaceInterest,
} from "./planeChanged";

const change = (over: Partial<PlaneChange> & Pick<PlaneChange, "kind">): PlaneChange => ({
  workspace: null,
  persona: null,
  path: "x",
  ...over,
});

describe("whether a change concerns a reader of the plane (FD-10)", () => {
  it("concerns every reader when the core could not say what changed", () => {
    expect(concerns(null, SIDEBAR)).toBe(true);
    expect(concerns(undefined, workspaceInterest("alpha"))).toBe(true);
  });

  it("concerns a reader with no interest named, whatever changed", () => {
    expect(concerns([change({ kind: "memory", workspace: "alpha" })], undefined)).toBe(true);
  });

  it("concerns nobody when nothing changed", () => {
    expect(concerns([], undefined)).toBe(false);
    expect(concerns([], SIDEBAR)).toBe(false);
  });

  it("keeps the sidebar to todos, workspaces, personas and the project", () => {
    expect(concerns([change({ kind: "todos", workspace: "beta" })], SIDEBAR)).toBe(true);
    expect(concerns([change({ kind: "workspace", workspace: "beta" })], SIDEBAR)).toBe(true);
    expect(concerns([change({ kind: "persona", persona: "steward" })], SIDEBAR)).toBe(true);
    expect(concerns([change({ kind: "project" })], SIDEBAR)).toBe(true);
    expect(concerns([change({ kind: "memory", workspace: "beta" })], SIDEBAR)).toBe(false);
    expect(concerns([change({ kind: "sessions", workspace: "beta" })], SIDEBAR)).toBe(false);
    expect(concerns([change({ kind: "harness" })], SIDEBAR)).toBe(false);
  });

  it("keeps a workspace's panels to that workspace, the personas and the project", () => {
    const alpha = workspaceInterest("alpha");
    expect(concerns([change({ kind: "memory", workspace: "alpha" })], alpha)).toBe(true);
    expect(concerns([change({ kind: "sessions", workspace: "alpha" })], alpha)).toBe(true);
    expect(concerns([change({ kind: "todos", workspace: "beta" })], alpha)).toBe(false);
    expect(concerns([change({ kind: "memory", workspace: "beta" })], alpha)).toBe(false);
    // The Personas panel counts each persona's memories.
    expect(concerns([change({ kind: "memory", persona: "steward" })], alpha)).toBe(true);
    expect(concerns([change({ kind: "persona", persona: "steward" })], alpha)).toBe(true);
    expect(concerns([change({ kind: "project" })], alpha)).toBe(true);
    expect(concerns([change({ kind: "harness" })], alpha)).toBe(false);
  });

  it("keeps the plane root's panels to the root's own session records", () => {
    expect(concerns([change({ kind: "sessions" })], ROOT_PANELS)).toBe(true);
    expect(concerns([change({ kind: "sessions", workspace: "alpha" })], ROOT_PANELS)).toBe(false);
    expect(concerns([change({ kind: "todos", workspace: "alpha" })], ROOT_PANELS)).toBe(false);
  });

  it("keeps the git readers and the window's own readers off memory and session records", () => {
    for (const kind of ["memory", "sessions"] as const) {
      expect(concerns([change({ kind, workspace: "alpha" })], PLANE_SHAPE)).toBe(false);
      expect(concerns([change({ kind, workspace: "alpha" })], SETTINGS)).toBe(false);
    }
    for (const kind of ["project", "harness", "workspace", "todos", "persona"] as const) {
      expect(concerns([change({ kind })], PLANE_SHAPE)).toBe(true);
    }
    expect(concerns(null, PLANE_SHAPE)).toBe(true);
  });

  it("keeps what a project has on to its settings files and workspace manifests", () => {
    expect(concerns([change({ kind: "project" })], SETTINGS)).toBe(true);
    expect(concerns([change({ kind: "workspace", workspace: "alpha" })], SETTINGS)).toBe(true);
    expect(concerns([change({ kind: "todos", workspace: "alpha" })], SETTINGS)).toBe(false);
    expect(concerns([change({ kind: "harness" })], SETTINGS)).toBe(false);
  });

  it("is told by any one change of a batch", () => {
    const batch = [change({ kind: "harness" }), change({ kind: "todos", workspace: "alpha" })];
    expect(concerns(batch, SIDEBAR)).toBe(true);
  });
});
