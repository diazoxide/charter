import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  configure,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { ViewTab } from "./bindings";
import { BUILT_IN, DEFAULT_THEME, drawIn, inForce } from "./theme/theme";

/**
 * **A tab that holds something other than a chat**, against the whole window (ADR 0043,
 * as amended 2026-09-23 — the operator: *"that in tabs we can have what we want - not only
 * harnesses"*).
 *
 * The model is `tabs.test.ts` and the view itself is `Views.test.tsx`. This is where they meet
 * the real `App`: the persona row, the palette row and a relaunch each opening the same tab, the
 * strip drawing it, and the core being told about it so the next launch puts it back.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane" tabIndex={-1}>
      session {session}
    </div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

// The whole window renders under StrictMode here, twice per effect, and a loaded runner takes
// longer than Testing Library's one second to draw it. What these assert is WHAT the window
// becomes, never how fast, so they wait as long as a slow machine needs.
configure({ asyncUtilTimeout: 5_000 });

beforeEach(() => globalThis.localStorage.clear());
afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const SIDEBAR = {
  root: PLANE,
  workspaces: [
    {
      name: "alpha",
      path: ALPHA,
      vision: "Ship it",
      todos: [],
      chats: [
        {
          session: 1,
          name: "1",
          cwd: ALPHA,
          harness: "claude",
          in_front: true,
          resumed: null,
          fresh: null,
          profile: "claude",
          persona: "steward",
        },
      ],
    },
  ],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

/** The chat the core put back at this launch. */
const OPEN_CHAT = {
  session: 1,
  name: "1",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "steward",
  unreported: null,
  pinned: false,
};

const PERSONAS_PANEL = {
  key: "charter/personas",
  title: "Personas",
  order: 20,
  mark: "persona",
  from: null,
  about: "personas",
  blocks: [
    {
      kind: "list",
      rows: [
        {
          key: "steward",
          text: "steward",
          note: "default · 1 memory",
          mark: "persona",
          tone: "default",
          detail: null,
          runs: "persona.show:steward",
        },
      ],
      empty: { headline: "No personas on this plane", body: null, offer: null },
    },
  ],
};

const STATISTICS = {
  extension: "persona-statistics",
  id: "statistics",
  title: "Statistics",
  about: "personas",
};

/** The core, answering every command the window sends, and recording what it was asked. */
function core(
  on: {
    reopened?: ViewTab[];
    chats?: (typeof OPEN_CHAT)[];
    offered?: (typeof STATISTICS)[];
    /** What `extensions_on` answers: the extensions this project has on (charter-app#253). */
    extensionsOn?: string[];
    /** What `project_theme_drawn` answers: the theme this project draws (charter-app#273). */
    projectTheme?: string | null;
  } = {},
) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats") return on.chats ?? [OPEN_CHAT];
    if (cmd === "reopened_views") return on.reopened ?? [];
    if (cmd === "chat_states") return [];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "extension_views") return on.offered ?? [];
    if (cmd === "extension_panels") return [];
    if (cmd === "extensions_on") return on.extensionsOn ?? ["persona-statistics"];
    if (cmd === "project_theme_drawn") return on.projectTheme ?? null;
    if (cmd === "workspace_panels")
      return {
        workspace: "alpha",
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
        contributed: [PERSONAS_PANEL],
      };
    if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
    if (cmd === "open_view")
      return given.from === null
        ? {
            kind: "answered",
            blocks: [{ kind: "note", text: "It remembers 1 thing.", tone: "plain" }],
            took_ms: 1,
          }
        : { kind: "answered", blocks: [], took_ms: 1 };
    return null;
  });
  return { asked };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.textContent);

/** Opens the persona's tab from its row on the personas panel. */
async function openFromTheRow() {
  const panel = await screen.findByTestId("panel-personas");
  await userEvent.click(within(panel).getByRole("button", { name: /steward/ }));
  return screen.findByText("It remembers 1 thing.");
}

describe("a persona's own tab", () => {
  it("opens from the persona's row, in front, on the strip in front, beside the chats", async () => {
    const { asked } = core();
    render(<App />);
    await within(await screen.findByRole("tablist", { name: "Tabs" })).findByRole("tab", {
      name: /steward 1/,
    });

    await openFromTheRow();

    const tab = within(strip()).getByRole("tab", { selected: true });
    expect(tab).toHaveTextContent("steward");
    expect(tabNames()).toEqual(["steward 1", "steward"]);
    expect(asked.filter((one) => one.cmd === "open_view").map((one) => one.args)).toEqual([
      { plane: PLANE, from: null, view: "persona", key: "steward" },
    ]);
  });

  it("is a mark and a name on the strip, and no chat state, because it has no chat", async () => {
    core();
    render(<App />);
    await openFromTheRow();

    const tab = within(strip()).getByRole("tab", { selected: true });

    expect(tab.querySelector(".tab-mark")).not.toBeNull();
    expect(tab.querySelector(".state")).toBeNull();
  });

  it("is brought forward, not opened twice, when the persona is opened again", async () => {
    core();
    render(<App />);
    await openFromTheRow();
    await userEvent.click(within(strip()).getByRole("tab", { name: /steward 1/ }));

    await openFromTheRow();

    expect(tabNames()).toEqual(["steward 1", "steward"]);
    expect(within(strip()).getByRole("tab", { selected: true })).toHaveTextContent(/^steward$/);
  });

  it("opens from the palette and stays open when the keyboard goes back to the terminal", async () => {
    // The adversarial review of #212 (finding 5): the palette hands focus back to the pane it
    // was opened over, and a non-modal card dismissed itself on that focus. A tab is not
    // dismissed by focus.
    core();
    render(<App />);
    const terminal = await screen.findByTestId("pane");
    terminal.focus();

    await userEvent.keyboard("{F2}");
    await screen.findByRole("dialog", { name: "Command palette" });
    await userEvent.keyboard("Show what steward is{Enter}");
    await screen.findByText("It remembers 1 thing.");
    // Wherever the keyboard went, the view is still the tab in front.
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Command palette" })).toBeNull(),
    );

    expect(within(strip()).getByRole("tab", { selected: true })).toHaveTextContent(/^steward$/);
    expect(screen.getByText("It remembers 1 thing.")).toBeInTheDocument();
  });

  it("closes without ending anything and without asking, because nothing runs in it", async () => {
    const { asked } = core();
    render(<App />);
    await openFromTheRow();

    await userEvent.click(within(strip()).getByRole("button", { name: "Close steward" }));

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(tabNames()).toEqual(["steward 1"]);
    expect(asked.some((one) => one.cmd === "close_session")).toBe(false);
  });

  it("closes on Delete from the keyboard, as its × does: without ending or asking", async () => {
    // charter-app#239: the key runs the row the `×` runs, so a view tab is still not asked about.
    const { asked } = core();
    render(<App />);
    await openFromTheRow();
    within(strip()).getByRole("tab", { selected: true }).focus();

    await userEvent.keyboard("{Delete}");

    await waitFor(() => expect(tabNames()).toEqual(["steward 1"]));
    await waitFor(() =>
      expect(within(strip()).getByRole("tab", { name: /steward 1/ })).toHaveFocus(),
    );
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(asked.some((one) => one.cmd === "close_session")).toBe(false);
  });

  it("is told to the core, so the record brings it back at the next launch", async () => {
    const { asked } = core();
    render(<App />);
    await openFromTheRow();

    await waitFor(() =>
      expect(asked.filter((one) => one.cmd === "window_views").at(-1)?.args).toEqual({
        plane: PLANE,
        views: [
          {
            from: null,
            view: "persona",
            key: "steward",
            title: "steward",
            workspace: "alpha",
            at: 1,
            active: true,
            pinned: false,
          },
        ],
      }),
    );
  });
});

describe("view tabs at a relaunch", () => {
  const STEWARD_BACK: ViewTab = {
    from: null,
    view: "persona",
    key: "steward",
    title: "steward",
    workspace: "alpha",
    at: 0,
    active: true,
    pinned: false,
  };
  const STATISTICS_BACK: ViewTab = {
    from: "persona-statistics",
    view: "statistics",
    key: "",
    title: "Statistics",
    workspace: "alpha",
    at: 2,
    active: false,
    pinned: true,
  };

  it("come back where they were among the chats, with the one that was in front in front", async () => {
    core({
      reopened: [STEWARD_BACK, STATISTICS_BACK],
      chats: [{ ...OPEN_CHAT, in_front: false }],
      offered: [STATISTICS],
    });
    render(<App />);

    await waitFor(() => expect(tabNames()).toEqual(["Statistics", "steward", "steward 1"]));
    // Pinned first on the strip (ADR 0039), and the pin rode the record back.
    expect(within(strip()).getByRole("tab", { name: /Statistics/ })).toContainElement(
      within(strip()).getByRole("img", { name: "pinned tab" }),
    );
    expect(within(strip()).getByRole("tab", { selected: true })).toHaveTextContent(/^steward$/);
  });

  it("ask an extension's program nothing until the operator presses for it", async () => {
    const { asked } = core({
      reopened: [{ ...STATISTICS_BACK, active: true }],
      chats: [{ ...OPEN_CHAT, in_front: false }],
      offered: [STATISTICS],
    });
    render(<App />);

    const waiting = await screen.findByTestId("view-waits");

    expect(asked.some((one) => one.cmd === "open_view")).toBe(false);
    await userEvent.click(within(waiting).getByRole("button", { name: "Ask persona-statistics" }));
    await waitFor(() =>
      expect(asked.filter((one) => one.cmd === "open_view").map((one) => one.args)).toEqual([
        { plane: PLANE, from: "persona-statistics", view: "statistics", key: "" },
      ]),
    );
  });

  it("do not write over the record before the window has read it", async () => {
    const { asked } = core({
      reopened: [STEWARD_BACK],
      chats: [{ ...OPEN_CHAT, in_front: false }],
    });
    render(<App />);
    await screen.findByText("It remembers 1 thing.");

    const told = asked.filter((one) => one.cmd === "window_views");
    expect(told.length).toBeGreaterThan(0);
    for (const one of told)
      expect((one.args.views as ViewTab[]).map((view) => view.key)).toEqual(["steward"]);
  });
});

describe("a project's own extensions (charter-app#253)", () => {
  it("offers an extension's view only while the project in front has that extension on", async () => {
    core({ offered: [STATISTICS], extensionsOn: ["persona-statistics"] });
    render(<App />);
    const panel = await screen.findByTestId("panel-personas");
    expect(await within(panel).findByRole("button", { name: /Statistics/ })).toBeInTheDocument();
  });

  it("offers nothing from an extension the project turned off", async () => {
    const { asked } = core({ offered: [STATISTICS], extensionsOn: [] });
    render(<App />);
    const panel = await screen.findByTestId("panel-personas");
    await waitFor(() =>
      expect(asked.some((one) => one.cmd === "extensions_on" && one.args.plane === PLANE)).toBe(
        true,
      ),
    );
    await within(panel).findByRole("button", { name: /steward/ });

    expect(within(panel).queryByRole("button", { name: /Statistics/ })).toBeNull();
  });
});

describe("a project's own theme (charter-app#273)", () => {
  afterEach(() => drawIn(DEFAULT_THEME));

  it("is drawn while that project is in front", async () => {
    const { asked } = core({ projectTheme: "charter-light" });
    render(<App />);
    await waitFor(() => expect(inForce()).toBe(BUILT_IN["charter-light"]));
    expect(asked.some((one) => one.cmd === "project_theme_drawn" && one.args.plane === PLANE)).toBe(
      true,
    );
  });

  it("leaves the window its own theme for a project that picked nothing", async () => {
    const { asked } = core({ projectTheme: null });
    render(<App />);
    await waitFor(() => expect(asked.some((one) => one.cmd === "project_theme_drawn")).toBe(true));
    await screen.findByTestId("panel-personas");
    expect(inForce()).toBe(DEFAULT_THEME);
  });
});
