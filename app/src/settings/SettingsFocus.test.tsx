import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "../App";
import type { DoctorRow } from "../bindings";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";
import { useRef } from "react";
import { enterSettings, forgetEntering, landSettingsFocus, useEnteringFocus } from "./entering";
import { stripNamed } from "../test-strips";

/**
 * **The keyboard goes into Settings on every way in** (#1206; the spec on #558, user story 32):
 * the palette, `⌘,` and the app menu, the quiet gears, a doctor row's and a notice's link, a
 * link that brings an open tab forward, and the level switcher each leave `document.activeElement`
 * on the group nav's current group — never on the page, and never back on the button a closed
 * dialog or drawer was opened from.
 */

vi.mock("../SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane">session {session}</div>
  ),
}));

const PLANE = "/home/dev/plane";

const FILE = (which: "shared" | "local") => ({
  which,
  file: which === "shared" ? "charter.toml" : "charter.local.toml",
  exists: which === "shared",
  text: "",
  refusals: [],
  parsed: true,
  fields: [],
});

const WORKSPACE = (workspace: string) => ({
  workspace,
  file: `workspaces/${workspace}/workspace.json`,
  exists: true,
  text: "{}\n",
  refusals: [],
  parsed: true,
  fields: [],
  live: false,
});

const ROW: DoctorRow = {
  name: "charter.toml",
  status: "warn",
  detail: "plane.mod in charter.toml is not read",
  hint: "Fix or remove it: until then purlis reads the next file down, or the default.",
  checked: true,
  settings: "project.saving",
  fix: null,
};

/** The layout file, holding a window text size charter cannot use when `badText`. */
function layout(badText = false) {
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: badText
      ? {
          path: "/home/op/.config/charter/layout.json",
          found: true,
          document: { version: 1, regions: [], text: { window: 2.5 } },
          trouble: null,
        }
      : { path: "/home/op/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
}

function core(plane: string | null) {
  mockIPC(
    (cmd, args) => {
      if (cmd === "plane_at_launch")
        return { plane, from: plane, why: plane === null ? "no plane here" : null };
      if (cmd === "opened_chats") return [];
      if (cmd === "reopened_views") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "plane_sidebar")
        return {
          root: plane,
          personas: [],
          persona: null,
          unfiled: [],
          workspaces: ["alpha"].map((name) => ({
            name,
            path: `${PLANE}/workspaces/${name}`,
            vision: "",
            todos: [],
            chats: [],
          })),
        };
      if (cmd === "project_settings") return { shared: FILE("shared"), local: FILE("local") };
      if (cmd === "workspace_settings") return WORKSPACE((args as { workspace: string }).workspace);
      if (cmd === "plane_doctor") return { rows: [ROW], full: true, app_rows: [], path: null };
      if (cmd === "alerts_everywhere") return [];
      if (cmd === "start_options")
        return {
          profiles: [
            {
              name: "claude",
              kind: "claude",
              shown: "claude",
              source: "built-in",
              is_default: true,
              approval: null,
            },
          ],
          refused: [["bad", "has kind nope"]],
          personas: [],
          persona: null,
          persona_profiles: {},
          ignore_fix: null,
          ignore_fix_id: null,
          declares_none: true,
        };
      return null;
    },
    { shouldMockEvents: true },
  );
}

beforeEach(() => {
  forgetThisLaunch();
  layout();
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const workspaces = () => stripNamed("Workspaces");
const projects = () => stripNamed("Projects");
const level = (name: string) => screen.getByRole("radio", { name });

/** The keyboard is on the group nav's current group, named `group` when given. */
async function onTheCurrentGroup(group?: string) {
  await waitFor(() => {
    const current = within(nav())
      .getAllByRole("button")
      .find((one) => one.getAttribute("aria-current") === "true");
    expect(current).toBeDefined();
    if (group !== undefined) expect(current).toHaveAccessibleName(group);
    expect(document.activeElement).toBe(current);
  });
}

async function settled() {
  // The listeners register asynchronously; give them a turn.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function palette(typed: string) {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard(typed);
  await userEvent.keyboard("{Enter}");
}

async function fromTheDoctor() {
  const button = screen.getByTestId("status-doctor");
  await userEvent.click(button);
  const doctor = await screen.findByRole("dialog", { name: "Doctor" });
  await userEvent.click(await within(doctor).findByRole("button", { name: "Fix it in Settings" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Doctor" })).toBeNull());
  return button;
}

describe("every way into Settings leaves the keyboard on the nav's current group", () => {
  it("the palette's Settings…, and not back where the palette was opened", async () => {
    core(PLANE);
    render(<App />);
    const tab = await screen.findByRole("tab", { name: /plane/ });
    // Opened over something still on the page, which the palette would hand the keyboard back to.
    tab.focus();

    await palette("Settings…");

    await onTheCurrentGroup();
    await settled();
    expect(document.activeElement).not.toBe(tab);
    await onTheCurrentGroup();
  });

  it("the palette's Your settings…", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    await palette("Your settings");

    await waitFor(() => expect(level("You")).toBeChecked());
    await onTheCurrentGroup();
  });

  it("the app menu's Settings… (⌘,)", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await settled();

    await act(async () => {
      await emit("settings-asked");
    });

    await onTheCurrentGroup();
  });

  it("⌘, with no project open, where Settings is drawn in place of the opener", async () => {
    core(null);
    render(<App />);
    await waitFor(() =>
      expect(screen.queryByRole("heading", { name: /project/, level: 1 })).toBeInTheDocument(),
    );
    await settled();

    await act(async () => {
      await emit("settings-asked");
    });

    await waitFor(() => expect(level("You")).toBeChecked());
    await onTheCurrentGroup();
  });

  it("the project gear", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /alpha/ });

    await userEvent.click(within(projects()).getByRole("button", { name: "Project settings…" }));

    await waitFor(() => expect(level("Project")).toBeChecked());
    await onTheCurrentGroup();
  });

  it("the workspace gear", async () => {
    core(PLANE);
    render(<App />);
    await userEvent.click(await screen.findByRole("tab", { name: /alpha/ }));

    await userEvent.click(
      within(workspaces()).getByRole("button", { name: "Workspace settings…" }),
    );

    await waitFor(() => expect(level("Workspace")).toBeChecked());
    await onTheCurrentGroup();
  });

  it("a doctor row's link, and not back on the doctor's button", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    const button = await fromTheDoctor();

    await onTheCurrentGroup("Saving");
    // Past the closed dialog's own way back to its trigger, which runs a turn later.
    await settled();
    expect(document.activeElement).not.toBe(button);
    await onTheCurrentGroup("Saving");
  });

  it("a link that brings the open tab forward", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await fromTheDoctor();
    await onTheCurrentGroup("Saving");
    // Another tab in front, and the keyboard elsewhere.
    await palette("Saving…");
    await waitFor(() => expect(screen.queryByRole("navigation", { name: "Groups" })).toBeNull());

    await fromTheDoctor();

    await onTheCurrentGroup("Saving");
  });

  it("a notice's link in the alerts drawer, and not back on the drawer's opener", async () => {
    layout(true);
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    const opener = await screen.findByRole("button", { name: /^Alerts/ });
    await userEvent.click(opener);
    const drawer = await screen.findByRole("dialog", { name: "Alerts" });
    const machine = within(drawer).getByRole("region", { name: "Alerts about this machine" });

    await userEvent.click(within(machine).getByRole("button", { name: "Fix it in Settings" }));

    await waitFor(() => expect(level("You")).toBeChecked());
    await onTheCurrentGroup("Text");
    await settled();
    expect(document.activeElement).not.toBe(opener);
  });

  it("the level switcher, moving the tab to another level", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await palette("Settings…");
    await onTheCurrentGroup();

    await userEvent.click(level("You"));

    await waitFor(() => expect(level("You")).toBeChecked());
    await onTheCurrentGroup();
  });
  it("a tab's menu, and not back on the tab", async () => {
    core(PLANE);
    render(<App />);
    const tab = await screen.findByRole("tab", { name: /alpha/ });
    await userEvent.click(tab);
    tab.focus();
    fireEvent.keyDown(tab, { key: "F10", shiftKey: true });
    const menu = await screen.findByRole("menu");

    await userEvent.click(within(menu).getByRole("menuitem", { name: "Workspace settings…" }));

    await waitFor(() => expect(level("Workspace")).toBeChecked());
    await onTheCurrentGroup();
    await settled();
    expect(document.activeElement).not.toBe(tab);
    await onTheCurrentGroup();
  });

  it("the start dialog's link to a refused profile in Settings", async () => {
    core(PLANE);
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await userEvent.click(screen.getAllByRole("button", { name: "New tab" })[0]);
    await userEvent.click(await screen.findByText("1 refused"));

    await userEvent.click(screen.getByRole("button", { name: "Open bad in Settings" }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Start a chat" })).not.toBeInTheDocument(),
    );
    await onTheCurrentGroup("Harness & profiles");
  });
});

/** A Settings tab's half alone: its nav's current group, in a pane of the window. */
function Holder({ place, group, front }: { place: string; group: string; front?: boolean }) {
  const own = useRef<HTMLDivElement>(null);
  useEnteringFocus(place, own);
  return (
    <div className={front === undefined ? undefined : front ? "pane view focused" : "pane view"}>
      <div ref={own}>
        <nav className="ui-settings-nav" aria-label={`Groups of ${group}`}>
          <button type="button" aria-current="true">
            {group}
          </button>
        </nav>
      </div>
    </div>
  );
}

describe("a way in the person has moved on from (#1600)", () => {
  afterEach(() => forgetEntering());

  it("is dropped on their next key, before its tab is drawn", async () => {
    const elsewhere = render(<button type="button">Elsewhere</button>);
    const button = elsewhere.getByRole("button", { name: "Elsewhere" });
    button.focus();
    enterSettings("you");

    await userEvent.keyboard("a");
    render(<Holder place="you" group="Text" />);
    await settled();

    expect(button).toHaveFocus();
  });

  it("is dropped on their next click, before its tab is drawn", async () => {
    render(<button type="button">Elsewhere</button>);
    enterSettings("you");

    await userEvent.click(screen.getByRole("button", { name: "Elsewhere" }));
    render(<Holder place="you" group="Text" />);
    await settled();

    expect(screen.getByRole("button", { name: "Elsewhere" })).toHaveFocus();
  });

  it("no longer holds a closing surface's keyboard once they act after it landed", async () => {
    render(<Holder place="you" group="Text" />);
    enterSettings("you");
    await waitFor(() => expect(screen.getByRole("button", { name: "Text" })).toHaveFocus());
    expect(landSettingsFocus()).toBe(true);

    await userEvent.keyboard("{ArrowDown}");

    expect(landSettingsFocus()).toBe(false);
  });

  it("is not dropped by a modifier key alone", async () => {
    enterSettings("you");

    await userEvent.keyboard("{Shift}");
    render(<Holder place="you" group="Text" />);

    await waitFor(() => expect(screen.getByRole("button", { name: "Text" })).toHaveFocus());
  });
});

describe("two Settings tabs of one level side by side (#1600, #1292)", () => {
  afterEach(() => forgetEntering());

  it("the one in the focused pane takes the keyboard, not the first one opened", async () => {
    render(
      <>
        <Holder place="project" group="Behind" front={false} />
        <Holder place="project" group="In front" front />
      </>,
    );

    enterSettings("project");

    await waitFor(() => expect(screen.getByRole("button", { name: "In front" })).toHaveFocus());
  });

  it("the first one opened, when neither pane is the focused one", async () => {
    render(
      <>
        <Holder place="project" group="First" />
        <Holder place="project" group="Second" />
      </>,
    );

    enterSettings("project");

    await waitFor(() => expect(screen.getByRole("button", { name: "First" })).toHaveFocus());
  });
});

describe("a way in that moves the window's focus to another pane (#1600)", () => {
  afterEach(() => forgetEntering());

  /** Two Settings tabs of one level split side by side; `focused` is the pane with the focus. */
  function Split({ focused }: { focused: "left" | "right" }) {
    return (
      <>
        <Holder place="project" group="Left" front={focused === "left"} />
        <Holder place="project" group="Right" front={focused === "right"} />
      </>
    );
  }

  it("lands in the pane it focused, not the one that had the focus before", async () => {
    const view = render(<Split focused="right" />);

    // The way in asks, then the window draws the pane it brought forward, as `showView` does.
    act(() => {
      enterSettings("project");
      view.rerender(<Split focused="left" />);
    });

    await waitFor(() => expect(screen.getByRole("button", { name: "Left" })).toHaveFocus());
  });
});
