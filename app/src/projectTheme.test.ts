import { afterEach, describe, expect, it } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { projectThemeChanged, useProjectTheme, useProjectThemeAnswers } from "./projectTheme";

/**
 * The window's store of what each project draws in each workspace (charter-app#273, #281). What
 * is held here is what it asks the core, and when: once per project and workspace something on
 * screen asks about, again when told something changed, and never again for a pair nothing asks
 * about any more — a workspace switched away from, or deleted.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function core() {
  const asked: (string | null)[] = [];
  mockIPC((cmd, args) => {
    if (cmd !== "project_theme_drawn") return null;
    const workspace = (args as { workspace: string | null }).workspace;
    asked.push(workspace);
    return workspace === "alpha" ? "charter-light" : null;
  });
  return asked;
}

describe("the theme each workspace draws", () => {
  it("is asked for the project and workspace together, once", async () => {
    const asked = core();
    const { result } = renderHook(() => useProjectTheme(PLANE, "alpha"));
    await waitFor(() => expect(result.current).toBe("charter-light"));
    expect(asked).toEqual(["alpha"]);
  });

  it("is asked again, when something changed, only for what is still on screen", async () => {
    const asked = core();
    const gone = renderHook(() => useProjectTheme(PLANE, "beta"));
    const here = renderHook(() => useProjectTheme(PLANE, "alpha"));
    await waitFor(() => expect(here.result.current).toBe("charter-light"));
    await waitFor(() => expect(gone.result.current).toBeNull());
    gone.unmount();
    asked.length = 0;

    projectThemeChanged(PLANE);
    await waitFor(() => expect(asked).toEqual(["alpha"]));
  });

  it("counts a settings tab's answers for its own workspace alone", async () => {
    core();
    const alpha = renderHook(() => useProjectThemeAnswers(PLANE, "alpha"));
    const beta = renderHook(() => useProjectThemeAnswers(PLANE, "beta"));
    await waitFor(() => expect(alpha.result.current).toBe(1));
    await waitFor(() => expect(beta.result.current).toBe(1));

    projectThemeChanged(PLANE);
    await waitFor(() => expect(alpha.result.current).toBe(2));
    expect(beta.result.current).toBe(2);
  });
});
