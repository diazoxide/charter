import { afterEach, describe, expect, it } from "vitest";
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  projectThemeChanged,
  useProjectTheme,
  useProjectThemeAnswers,
  useProjectThemeKept,
} from "./projectTheme";

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

  it("keeps a project's answer for a hook that only holds it, and redraws that hook for none", async () => {
    // A project behind the one in front holds its theme so a switch back finds it (FR-27). Its
    // view must not be drawn again each time the core answers what it already said.
    const asked = core();
    let renders = 0;
    renderHook(() => {
      renders += 1;
      useProjectThemeKept(PLANE, "alpha");
    });
    await waitFor(() => expect(asked).toEqual(["alpha"]));
    const front = renderHook(() => useProjectTheme(PLANE, "alpha"));
    await waitFor(() => expect(front.result.current).toBe("charter-light"));
    // The window asking found the answer held, and asked nothing more.
    expect(asked).toEqual(["alpha"]);
    const before = renders;

    projectThemeChanged(PLANE);
    await waitFor(() => expect(asked).toEqual(["alpha", "alpha"]));
    await waitFor(() => expect(front.result.current).toBe("charter-light"));

    expect(renders).toBe(before);
  });
});
