import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SettingsTab } from "./SettingsTab";
import { forgetThisLaunch } from "../regions";
import { DEFAULT_TEXT, setTextSize, textSizes } from "../textSize";
import { GLOBAL } from "../windowprefs";
import { yourEditor } from "../yourEditor";
import { chatsListPrefs } from "../chatsListPrefs";

/**
 * **The Settings tab at the You level** (SE-16, #1166; the spec on #558, rulings V89a–i): a
 * level switcher, a nav of groups, and the chosen group on the right. Driven the way a person
 * drives it — pick a group, change a value — and read back from what is on screen and from what
 * the value now is, never from the tab's insides.
 */

const PATH = "/home/op/.config/charter/layout.json";

beforeEach(() => {
  forgetThisLaunch();
  (globalThis as Record<string, unknown>)[GLOBAL] = {
    layout: { path: PATH, found: false, document: null, trouble: null },
    theme: { path: "", found: false, document: null, trouble: null },
  };
  mockIPC(() => null);
});
afterEach(() => {
  cleanup();
  clearMocks();
  Reflect.deleteProperty(globalThis, GLOBAL);
});

const nav = () => screen.getByRole("navigation", { name: "Groups" });
const group = (name: string) => within(nav()).getByRole("button", { name });
const shown = () => screen.getByRole("region", { name: /./ });
const slider = (name: RegExp) => screen.getByRole("slider", { name });

describe("the Settings tab, at the You level", () => {
  it("opens at You, with Text, Editor, Chats list and This machine in the nav and Text's settings on the right", () => {
    render(<SettingsTab />);

    const levels = screen.getByRole("radiogroup", { name: "Level" });
    expect(
      within(levels)
        .getAllByRole("radio")
        .map((one) => one.textContent),
    ).toEqual(["You"]);
    expect(within(levels).getByRole("radio", { name: "You" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(
      within(nav())
        .getAllByRole("button")
        .map((one) => one.textContent),
    ).toEqual(["Text", "Editor", "Chats list", "This machine"]);
    expect(group("Text")).toHaveAttribute("aria-current", "true");
    expect(shown()).toHaveAccessibleName("Text");
    expect(slider(/window text size/i)).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: "Your editor" })).not.toBeInTheDocument();
  });

  it("says the You level is this machine's, and where it is kept", () => {
    render(<SettingsTab />);

    expect(screen.getByText(`This machine only, in every project. Kept in ${PATH}.`)).toBeVisible();
  });

  it("shows only the Editor group's settings once Editor is picked", async () => {
    render(<SettingsTab />);

    await userEvent.click(group("Editor"));

    expect(group("Editor")).toHaveAttribute("aria-current", "true");
    expect(group("Text")).not.toHaveAttribute("aria-current");
    expect(shown()).toHaveAccessibleName("Editor");
    expect(screen.getByRole("radiogroup", { name: "Your editor" })).toBeInTheDocument();
    expect(screen.queryByRole("slider")).not.toBeInTheDocument();
  });
});

describe("text size, in Settings", () => {
  it("draws both sizes at what they are", () => {
    render(<SettingsTab />);

    expect(slider(/window text size/i)).toHaveValue(String(DEFAULT_TEXT.window));
    expect(slider(/terminal text size/i)).toHaveValue(String(DEFAULT_TEXT.terminal));
  });

  it("says a size in pixels to a screen reader, and in px on screen", () => {
    render(<SettingsTab />);

    expect(slider(/window text size/i)).toHaveAttribute(
      "aria-valuetext",
      `${DEFAULT_TEXT.window} pixels`,
    );
    expect(screen.getByText(`${DEFAULT_TEXT.window}px`)).toBeVisible();
  });

  it("applies a size as the slider moves", () => {
    render(<SettingsTab />);

    fireEvent.change(slider(/window text size/i), { target: { value: "18" } });

    expect(textSizes().window).toBe(18);
    expect(slider(/window text size/i)).toHaveValue("18");
    expect(screen.getByText("18px")).toBeInTheDocument();
  });

  it("redraws when a size changes from somewhere else — a key", () => {
    render(<SettingsTab />);

    act(() => setTextSize("terminal", 16));

    expect(slider(/terminal text size/i)).toHaveValue("16");
  });

  it("resets one size to its default, and cannot reset one that is already there", async () => {
    render(<SettingsTab />);
    const reset = () => screen.getByRole("button", { name: `Reset to ${DEFAULT_TEXT.terminal}px` });
    expect(reset()).toBeDisabled();

    act(() => setTextSize("terminal", 20));
    await userEvent.click(reset());

    expect(textSizes().terminal).toBe(DEFAULT_TEXT.terminal);
    expect(reset()).toBeDisabled();
  });

  it("says under each size where its keys work", () => {
    render(<SettingsTab />);

    expect(slider(/window text size/i)).toHaveAccessibleDescription(
      /Everything but the terminals\./,
    );
    expect(slider(/terminal text size/i)).toHaveAccessibleDescription(
      /Every chat's terminal, refitted to the new size\./,
    );
  });
});

describe("the Chats list, in Settings (#1499, V100-73)", () => {
  it("chooses two lines or one for a row, and starts at two", async () => {
    render(<SettingsTab />);
    await userEvent.click(group("Chats list"));

    const rows = screen.getByRole("radiogroup", { name: "Rows" });
    expect(within(rows).getByRole("radio", { name: "Two lines" })).toBeChecked();

    await userEvent.click(within(rows).getByRole("radio", { name: "One line" }));

    expect(chatsListPrefs().lines).toBe(1);
    expect(within(rows).getByRole("radio", { name: "One line" })).toBeChecked();
  });

  it("groups the list by workspace, off until it is turned on", async () => {
    render(<SettingsTab />);
    await userEvent.click(group("Chats list"));

    const box = screen.getByRole("checkbox", { name: "Group by workspace" });
    expect(box).not.toBeChecked();

    await userEvent.click(box);

    expect(chatsListPrefs().grouped).toBe(true);
    expect(box).toBeChecked();
  });

  it("opens tasks in their own tabs, off until it is turned on (#1489)", async () => {
    render(<SettingsTab />);
    await userEvent.click(group("Chats list"));

    const box = screen.getByRole("checkbox", { name: "Open tasks in their own tabs" });
    expect(box).not.toBeChecked();

    await userEvent.click(box);

    expect(chatsListPrefs().tabbed).toBe(true);
    expect(box).toBeChecked();
  });

  it("sums up what happened while you were away, on until it is turned off (#1514)", async () => {
    render(<SettingsTab />);
    await userEvent.click(group("Chats list"));

    const box = screen.getByRole("checkbox", { name: "Away summary" });
    expect(box).toBeChecked();

    await userEvent.click(box);

    expect(chatsListPrefs().away).toBe(false);
    expect(box).not.toBeChecked();
  });
});

describe("your editor, in Settings", () => {
  it("names the choice by its row, and no label points at the group", async () => {
    const { container } = render(<SettingsTab />);
    await userEvent.click(group("Editor"));

    expect(screen.getByRole("radiogroup", { name: "Your editor" })).toHaveAccessibleDescription(
      "Where Open in your editor sends a file, at the line you are reading.",
    );
    const labels = [...container.querySelectorAll("label[for]")];
    expect(labels.map((one) => one.textContent)).not.toContain("Your editor");
  });

  it("offers the four, with none chosen until you pick one", async () => {
    render(<SettingsTab />);
    await userEvent.click(group("Editor"));

    const choices = within(screen.getByRole("radiogroup", { name: "Your editor" })).getAllByRole(
      "radio",
    );
    expect(choices.map((one) => one.getAttribute("aria-checked"))).toEqual([
      "false",
      "false",
      "false",
      "false",
    ]);
    expect(screen.getByRole("radio", { name: "Zed" })).toBeInTheDocument();
  });

  it("chooses one, and it applies at once", async () => {
    render(<SettingsTab />);
    await userEvent.click(group("Editor"));

    await userEvent.click(screen.getByRole("radio", { name: "A JetBrains IDE" }));

    expect(yourEditor()).toBe("idea");
    expect(screen.getByRole("radio", { name: "A JetBrains IDE" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
});

describe("the Settings tab, by keyboard", () => {
  it("moves between groups with the arrow keys, and Enter shows the one reached", async () => {
    render(<SettingsTab />);

    group("Text").focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(group("Editor")).toHaveFocus();
    await userEvent.keyboard("{Enter}");

    expect(shown()).toHaveAccessibleName("Editor");
  });
});
