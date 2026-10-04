import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { GLOBAL } from "../windowprefs";

/**
 * **Filter settings by name** (SE-21, #1171; the spec on #558, V89c): the box above the nav
 * narrows the nav to the groups that match and the right column to the settings that match, by
 * their labels and help text, case-insensitively, and says so when nothing does. Driven at the
 * You level, whose groups are Text (two sizes) and Editor (your editor), by typing in the box
 * the way a person does and reading what is on screen.
 */

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: {
      path: "/home/op/.config/charter/layout.json",
      found: false,
      document: null,
      trouble: null,
    },
    theme: { path: "", found: false, document: null, trouble: null },
  };
  mockIPC(() => null);
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const box = () => screen.getByRole("searchbox", { name: "Filter settings" });
const nav = () => screen.getByRole("navigation", { name: "Groups" });
const inNav = () =>
  within(nav())
    .queryAllByRole("button")
    .map((one) => one.textContent);
const group = (name: string) => within(nav()).getByRole("button", { name });
const shown = () => screen.getByRole("region", { name: /./ });
const sliders = () =>
  screen.queryAllByRole("slider").map((one) => one.getAttribute("aria-label") ?? one.id);
const count = () => screen.getByRole("status", { name: "Settings found" });

describe("filtering the Settings tab by name", () => {
  it("has a labelled filter box, empty, with the whole nav under it", () => {
    render(<SettingsTab />);

    expect(box()).toHaveValue("");
    expect(inNav()).toEqual(["Text", "Editor"]);
    expect(box().compareDocumentPosition(nav()) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("narrows the nav and the settings to a setting's label, whatever its case", async () => {
    render(<SettingsTab />);

    await userEvent.type(box(), "TERMINAL TEXT");

    expect(inNav()).toEqual(["Text"]);
    expect(screen.getByRole("slider", { name: /terminal text size/i })).toBeInTheDocument();
    expect(screen.queryByRole("slider", { name: /window text size/i })).not.toBeInTheDocument();
  });

  it("matches a setting's help text, and shows the group that holds it", async () => {
    render(<SettingsTab />);

    await userEvent.type(box(), "the line you are reading");

    expect(inNav()).toEqual(["Editor"]);
    expect(group("Editor")).toHaveAttribute("aria-current", "true");
    expect(shown()).toHaveAccessibleName("Editor");
    expect(screen.getByRole("radiogroup", { name: "Your editor" })).toBeInTheDocument();
    expect(sliders()).toEqual([]);
  });

  it("shows every setting of a group whose own label or help matches", async () => {
    render(<SettingsTab />);

    await userEvent.type(box(), "how big");

    expect(inNav()).toEqual(["Text"]);
    expect(screen.getByRole("slider", { name: /window text size/i })).toBeInTheDocument();
    expect(screen.getByRole("slider", { name: /terminal text size/i })).toBeInTheDocument();
  });

  it("says in a sentence when nothing matches, and the nav is empty", async () => {
    render(<SettingsTab />);

    await userEvent.type(box(), "colour of the moon");

    expect(inNav()).toEqual([]);
    expect(screen.queryByRole("region", { name: /./ })).not.toBeInTheDocument();
    // Said in the right column, where the group would be; the live region says it once more,
    // to a screen reader only.
    expect(
      screen.getByText("No setting at this level matches “colour of the moon”.", {
        ignore: "[role=status]",
      }),
    ).toBeVisible();
  });

  it("says how many settings match, politely, and nothing while the box is empty", async () => {
    render(<SettingsTab />);
    expect(count()).toHaveAttribute("aria-live", "polite");
    expect(count()).toHaveTextContent(/^$/);

    await userEvent.type(box(), "text size");
    expect(count()).toHaveTextContent("2 settings match");

    await userEvent.clear(box());
    await userEvent.type(box(), "terminal text");
    expect(count()).toHaveTextContent("1 setting matches");

    await userEvent.type(box(), "zzz");
    expect(count()).toHaveTextContent("No setting at this level matches “terminal textzzz”.");
  });

  it("puts the whole nav back when the box is cleared, at the group that was shown", async () => {
    render(<SettingsTab />);

    await userEvent.type(box(), "your editor");
    expect(inNav()).toEqual(["Editor"]);
    await userEvent.clear(box());

    expect(inNav()).toEqual(["Text", "Editor"]);
    expect(group("Editor")).toHaveAttribute("aria-current", "true");
    expect(count()).toHaveTextContent(/^$/);
  });

  it("clears the box at Escape", async () => {
    render(<SettingsTab />);

    await userEvent.type(box(), "zzz{Escape}");

    expect(box()).toHaveValue("");
    expect(inNav()).toEqual(["Text", "Editor"]);
  });
});

describe("the filtered nav, by keyboard", () => {
  it("is reached by Tab from the box at the first group still in it", async () => {
    render(<SettingsTab />);

    await userEvent.type(box(), "your editor");
    await userEvent.tab();

    expect(group("Editor")).toHaveFocus();
  });

  it("still moves between the groups left in it with the arrow keys", async () => {
    render(<SettingsTab />);

    // "size" is in Text's sliders and "sends" in Editor's help: both groups, each narrowed.
    await userEvent.type(box(), "s");
    expect(inNav()).toEqual(["Text", "Editor"]);
    group("Text").focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(group("Editor")).toHaveFocus();
    await userEvent.keyboard("{Enter}");

    expect(shown()).toHaveAccessibleName("Editor");
  });
});
