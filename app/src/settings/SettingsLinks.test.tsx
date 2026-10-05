import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "../App";
import type { DoctorRow } from "../bindings";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";

/**
 * **Deep links into Settings** (SE-22, #1172; the spec on #558, V89c): a group's stable id is
 * its address. A doctor row that names a setting opens Settings at that level with that group
 * shown; opening Settings for a target whose tab is open brings that tab forward; and each
 * level shows the group last looked at there, for as long as the window runs. Driven from the
 * window the way a person drives it, and read back from what is on screen.
 */

const PLANE = "/home/dev/plane";

/** An empty settings file, as `project_settings` answers it. */
const FILE = (which: "shared" | "local") => ({
  which,
  file: which === "shared" ? "charter.toml" : "charter.local.toml",
  exists: which === "shared",
  text: "",
  refusals: [],
  parsed: true,
  fields: [],
});

const ROW = (over: Partial<DoctorRow> = {}): DoctorRow => ({
  name: "charter.toml",
  status: "warn",
  detail: "plane.mod in charter.toml is not read",
  hint: "Fix or remove it: until then charter reads the next file down, or the default.",
  checked: true,
  settings: "project.saving",
  ...over,
});

let doctorRows: DoctorRow[] = [];

beforeEach(() => {
  forgetThisLaunch();
  doctorRows = [ROW()];
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: "/home/op/.config/charter/layout.json", found: false, document: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
  mockIPC(
    (cmd) => {
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [];
      if (cmd === "reopened_views") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "chat_states") return [];
      if (cmd === "plane_sidebar")
        return { root: PLANE, personas: [], persona: null, unfiled: [], workspaces: [] };
      if (cmd === "project_settings") return { shared: FILE("shared"), local: FILE("local") };
      if (cmd === "plane_doctor") return { rows: doctorRows, full: true, app_rows: [], path: null };
      if (cmd === "alerts_everywhere") return [];
      return null;
    },
    { shouldMockEvents: true },
  );
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

async function palette(typed: string) {
  await userEvent.keyboard("{F2}");
  await screen.findByRole("dialog", { name: "Command palette" });
  await userEvent.keyboard(typed);
  await userEvent.keyboard("{Enter}");
}

const settingsTabs = () =>
  within(screen.getByRole("tablist", { name: "Tabs" }))
    .queryAllByRole("tab")
    .filter((tab) => /^Settings/.test(tab.textContent ?? ""));
const nav = () => screen.getByRole("navigation", { name: "Groups" });
const group = (name: string) => within(nav()).getByRole("button", { name });
const level = (name: string) => screen.getByRole("radio", { name });
const filterBox = () => screen.getByRole("searchbox", { name: "Filter settings" });

/** Opens the doctor from the status line and presses the row's way into Settings. */
async function fromTheDoctor() {
  await userEvent.click(screen.getByTestId("status-doctor"));
  const doctor = await screen.findByRole("dialog", { name: "Doctor" });
  await userEvent.click(await within(doctor).findByRole("button", { name: "Fix it in Settings" }));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Doctor" })).toBeNull());
}

describe("a link into Settings", () => {
  it("opens Settings at the row's level with its group shown, from a doctor row", async () => {
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    await fromTheDoctor();

    await waitFor(() => expect(group("Saving")).toHaveAttribute("aria-current", "true"));
    expect(level("Project")).toBeChecked();
    expect(screen.getByRole("region", { name: "Saving" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Doctor" })).not.toBeInTheDocument();
    expect(settingsTabs()).toHaveLength(1);
  });

  it("brings the open tab for that target forward, at the linked group, rather than another", async () => {
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await fromTheDoctor();
    await waitFor(() => expect(group("Saving")).toHaveAttribute("aria-current", "true"));
    await userEvent.click(group("General"));
    // Another tab in front: the project's Saving tab.
    await palette("Saving…");
    await waitFor(() => expect(settingsTabs()[0]).toHaveAttribute("aria-selected", "false"));

    await fromTheDoctor();

    await waitFor(() => expect(settingsTabs()[0]).toHaveAttribute("aria-selected", "true"));
    expect(level("Project")).toBeChecked();
    expect(group("Saving")).toHaveAttribute("aria-current", "true");
    expect(settingsTabs()).toHaveLength(1);
  });

  it("shows its group when the tab had a file open as TOML", async () => {
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await fromTheDoctor();
    await waitFor(() => expect(group("Saving")).toBeInTheDocument());
    await userEvent.click(
      within(screen.getByRole("group", { name: "Edit as TOML" })).getByRole("button", {
        name: "charter.toml",
      }),
    );
    expect(screen.getByRole("textbox", { name: "charter.toml, as TOML" })).toBeInTheDocument();

    await fromTheDoctor();

    await waitFor(() => expect(group("Saving")).toHaveAttribute("aria-current", "true"));
    expect(screen.getByRole("region", { name: "Saving" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "charter.toml, as TOML" })).toBeNull();
  });

  it("shows its group even when the tab's filter had hidden it", async () => {
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    await fromTheDoctor();
    await waitFor(() => expect(group("Saving")).toBeInTheDocument());
    // Typed as a change: in the whole window under jsdom, a click on the box lands the
    // keyboard on a pane divider (the palette's way in does the same), not in the box.
    fireEvent.change(filterBox(), { target: { value: "icons" } });
    expect(within(nav()).queryByRole("button", { name: "Saving" })).not.toBeInTheDocument();

    await fromTheDoctor();

    await waitFor(() => expect(group("Saving")).toHaveAttribute("aria-current", "true"));
    expect(filterBox()).toHaveValue("");
  });

  it("is drawn only on a doctor row that names a setting", async () => {
    doctorRows = [ROW({ name: "git identity", settings: null, detail: "no user.email" })];
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });

    await userEvent.click(screen.getByTestId("status-doctor"));
    const doctor = await screen.findByRole("dialog", { name: "Doctor" });

    await within(doctor).findByText("no user.email");
    expect(within(doctor).queryByRole("button", { name: "Fix it in Settings" })).toBeNull();
  });
});

describe("a notice about this machine that names a setting", () => {
  it("opens Settings at You with that group shown, from the alerts drawer", async () => {
    // The layout file holds a window text size charter cannot use: the Text group is the fix.
    (globalThis as Record<string, unknown>)[GLOBAL] = {
      layout: {
        path: "/home/op/.config/charter/layout.json",
        found: true,
        document: { version: 1, regions: [], text: { window: 2.5 } },
        trouble: null,
      },
      theme: { path: "", found: false, document: null, trouble: null },
    };
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    // You was last left at Editor, reached through the level switcher.
    await fromTheDoctor();
    await waitFor(() => expect(group("Saving")).toBeInTheDocument());
    await userEvent.click(level("You"));
    await userEvent.click(await screen.findByRole("button", { name: "Editor" }));
    await userEvent.click(level("Project"));
    await waitFor(() => expect(group("Saving")).toBeInTheDocument());

    await userEvent.click(await screen.findByRole("button", { name: /^Alerts/ }));
    const drawer = await screen.findByRole("dialog", { name: "Alerts" });
    const machine = within(drawer).getByRole("region", { name: "Alerts about this machine" });
    await userEvent.click(within(machine).getByRole("button", { name: "Fix it in Settings" }));

    await waitFor(() => expect(level("You")).toBeChecked());
    expect(group("Text")).toHaveAttribute("aria-current", "true");
    expect(screen.queryByRole("dialog", { name: "Alerts" })).toBeNull();
  });
});

describe("each level", () => {
  it("shows the group last looked at there, while the window runs", async () => {
    render(<App />);
    await screen.findByRole("tab", { name: /plane/ });
    // Reached through a link and the level switcher, whatever Settings… opens at.
    await fromTheDoctor();
    await waitFor(() => expect(group("Saving")).toHaveAttribute("aria-current", "true"));
    await userEvent.click(level("You"));
    await waitFor(() => expect(group("Text")).toHaveAttribute("aria-current", "true"));
    await userEvent.click(group("Editor"));

    await userEvent.click(level("Project"));
    await waitFor(() => expect(group("Saving")).toHaveAttribute("aria-current", "true"));
    await userEvent.click(group("Sandbox"));
    await userEvent.click(level("You"));

    await waitFor(() => expect(group("Editor")).toHaveAttribute("aria-current", "true"));
    await userEvent.click(level("Project"));
    await waitFor(() => expect(group("Sandbox")).toHaveAttribute("aria-current", "true"));
  });
});
