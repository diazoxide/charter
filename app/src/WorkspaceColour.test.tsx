import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, configure, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import {
  BUILT_IN,
  DEFAULT_THEME,
  drawIn,
  drawTint,
  inForce,
  property,
  tinted,
} from "./theme/theme";

/**
 * **A workspace's theme and colour, against the whole window** (charter-app#281).
 *
 * The resolver is the core's and the arithmetic is `theme/tint.ts`'s; what is held here is where
 * the window puts them: the theme the window draws follows the WORKSPACE in front, not only the
 * project (#313 left it following the project); the workspace in front's colour is on the
 * window's accent and focus ring, and on its chat strip; and every workspace tab shows its own
 * colour whether or not it is in front. A switch changes all of it at once.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane" tabIndex={-1}>
      session {session}
    </div>
  ),
}));

configure({ asyncUtilTimeout: 5_000 });

beforeEach(() => globalThis.localStorage.clear());
afterEach(() => {
  cleanup();
  clearMocks();
  drawTint(null);
  drawIn(DEFAULT_THEME);
});

const PLANE = "/home/dev/plane";

const workspace = (name: string, colour: string | null) => ({
  name,
  path: `${PLANE}/workspaces/${name}`,
  vision: "",
  todos: [],
  chats: [],
  colour,
});

/** alpha is teal and picks charter-light; beta has neither, and draws the project's nothing. */
function core() {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    // The operator has pinned every workspace, so every one is on the strip and can be
    // clicked there: the strip draws what is pinned and the one you are in (ADR 0054).
    if (cmd === "plane_pins")
      return { project: false, workspaces: ["alpha", "beta", "gamma"], missing: [] };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        workspaces: [
          workspace("alpha", "teal"),
          workspace("beta", null),
          // A grey `#rrggbb`: the core reads it, and it has no hue to tint with.
          workspace("gamma", DEFAULT_THEME.values["text.muted"]),
        ],
        personas: ["steward"],
        persona: "steward",
        unfiled: [],
      };
    if (cmd === "opened_chats") return [];
    if (cmd === "reopened_views") return [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "extension_views") return [];
    if (cmd === "extension_panels") return [];
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme_drawn") return given.workspace === "alpha" ? "charter-light" : null;
    if (cmd === "workspace_panels")
      return {
        workspace: given.workspace,
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
        contributed: [],
      };
    if (cmd === "workspace_repos")
      return { workspace: given.workspace, repos: [], cache_refused: null };
    return null;
  });
  return { asked };
}

const workspaceTab = (name: string) =>
  within(screen.getByRole("tablist", { name: "Workspaces" })).getByRole("tab", {
    name: new RegExp(name),
  });

const root = () => document.documentElement.style;

describe("a workspace's theme and colour", () => {
  it("draws the theme the workspace in front picks, and the project's when it picks none", async () => {
    const { asked } = core();
    render(<App />);
    await waitFor(() => expect(inForce()).toBe(BUILT_IN["charter-light"]));
    expect(
      asked.some((one) => one.cmd === "project_theme_drawn" && one.args.workspace === "alpha"),
    ).toBe(true);

    await userEvent.click(await screen.findByRole("tab", { name: /beta/ }));
    await waitFor(() => expect(inForce()).toBe(DEFAULT_THEME));
  });

  it("tints the window's accent and focus ring with the workspace in front, live on a switch", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(inForce()).toBe(BUILT_IN["charter-light"]));
    const teal = tinted(BUILT_IN["charter-light"], "teal");
    await waitFor(() =>
      expect(root().getPropertyValue(property("accent.base"))).toBe(teal.values["accent.base"]),
    );
    expect(root().getPropertyValue(property("focus.ring"))).toBe(teal.values["focus.ring"]);
    // The text is the theme's own.
    expect(root().getPropertyValue(property("text.primary"))).toBe(
      BUILT_IN["charter-light"].values["text.primary"],
    );

    await userEvent.click(workspaceTab("beta"));
    await waitFor(() =>
      expect(root().getPropertyValue(property("accent.base"))).toBe(
        DEFAULT_THEME.values["accent.base"],
      ),
    );
  });

  it("draws the chat strip in the colour of the workspace whose chats it holds", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(inForce()).toBe(BUILT_IN["charter-light"]));
    const bar = screen.getByRole("tablist", { name: "Tabs" }).closest("header");
    const teal = tinted(BUILT_IN["charter-light"], "teal");
    await waitFor(() =>
      expect(bar?.style.getPropertyValue(property("layer.chat"))).toBe(teal.values["layer.chat"]),
    );

    await userEvent.click(workspaceTab("beta"));
    await waitFor(() => expect(bar?.style.getPropertyValue(property("layer.chat"))).toBe(""));
  });

  it("marks each workspace tab with its own colour, in front or not", async () => {
    core();
    render(<App />);
    await waitFor(() => expect(inForce()).toBe(BUILT_IN["charter-light"]));
    await userEvent.click(workspaceTab("beta"));
    await waitFor(() => expect(inForce()).toBe(DEFAULT_THEME));

    // alpha is behind beta now, and still says it is teal: its shade and its mark are its own.
    const alpha = workspaceTab("alpha");
    expect(alpha).toHaveAttribute("aria-selected", "false");
    expect(alpha).toHaveAttribute("data-colour", "teal");
    expect(alpha.style.getPropertyValue(property("layer.workspace"))).toBe(
      tinted(DEFAULT_THEME, "teal").values["layer.workspace"],
    );
    expect(alpha.querySelector(".workspace-mark")).not.toBeNull();

    const beta = workspaceTab("beta");
    expect(beta).not.toHaveAttribute("data-colour");
    expect(beta.querySelector(".workspace-mark")).toBeNull();
    // A grey tints nothing, so it draws no mark that would say it did.
    expect(workspaceTab("gamma")).not.toHaveAttribute("data-colour");
    expect(workspaceTab("gamma").querySelector(".workspace-mark")).toBeNull();
  });
});
