import { afterEach, describe, expect, it, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { extensionsChanged, forgetExtensionsOn, useExtensionsOn } from "./extensionsOn";

/**
 * What each project, and each of its workspaces, has on (charter-app#253, #280): asked per
 * project AND workspace, and asked again after a save — including where the last question
 * failed.
 */

afterEach(() => {
  forgetExtensionsOn();
  clearMocks();
});

const PLANE = "/home/dev/plane";

describe("what a workspace has on", () => {
  it("is asked for the project and the workspace, and kept apart from the project's", async () => {
    const asked: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "extensions_on") return undefined;
      asked.push(args);
      const { workspace } = args as { workspace: string | null };
      return workspace === "alpha" ? [] : ["stats"];
    });
    const project = renderHook(() => useExtensionsOn(PLANE));
    const alpha = renderHook(() => useExtensionsOn(PLANE, "alpha"));

    await waitFor(() => expect(project.result.current).toEqual(new Set(["stats"])));
    await waitFor(() => expect(alpha.result.current).toEqual(new Set()));
    expect(asked).toEqual([
      { plane: PLANE, workspace: null },
      { plane: PLANE, workspace: "alpha" },
    ]);
  });

  it("is asked again after a save even where the last question failed", async () => {
    let fail = true;
    const asked = vi.fn();
    mockIPC((cmd, args) => {
      if (cmd !== "extensions_on") return undefined;
      asked(args);
      if (fail) throw new Error("could not read");
      return ["stats"];
    });
    const alpha = renderHook(() => useExtensionsOn(PLANE, "alpha"));
    await waitFor(() => expect(asked).toHaveBeenCalledTimes(1));
    expect(alpha.result.current).toBeUndefined();

    fail = false;
    extensionsChanged(PLANE);

    await waitFor(() => expect(alpha.result.current).toEqual(new Set(["stats"])));
    expect(asked).toHaveBeenLastCalledWith({ plane: PLANE, workspace: "alpha" });
  });
});
