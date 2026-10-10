import { afterEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Choice, SettingActions, SettingGroup, SettingRow, SettingsLayout } from "./components";

afterEach(cleanup);

const OPTIONS = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Beta" },
  { value: "c", label: "Gamma" },
];

function radio(value: string | undefined, disabled = false) {
  const onValueChange = vi.fn();
  render(
    <>
      <button type="button">before</button>
      <SettingRow
        label="Letter"
        grouped
        control={(ids) => (
          <Choice
            ids={ids}
            kind="radio"
            options={OPTIONS}
            value={value}
            onValueChange={onValueChange}
            disabled={disabled}
          />
        )}
      />
    </>,
  );
  return { onValueChange, user: userEvent.setup() };
}

describe("a radio choice and the arrow keys (docs/ui-primitives.md)", () => {
  it("picks the next option on a single arrow, once", async () => {
    const { onValueChange, user } = radio("a");

    screen.getByRole("radio", { name: "Alpha" }).focus();
    await user.keyboard("{ArrowDown}");

    expect(screen.getByRole("radio", { name: "Beta" })).toHaveFocus();
    expect(onValueChange).toHaveBeenCalledTimes(1);
    expect(onValueChange).toHaveBeenCalledWith("b");
  });

  it("picks nothing when Tab brings the keyboard into a group with no value", async () => {
    const { onValueChange, user } = radio(undefined);

    screen.getByRole("button", { name: "before" }).focus();
    await user.tab();

    expect(screen.getByRole("radio", { name: "Alpha" })).toHaveFocus();
    expect(onValueChange).not.toHaveBeenCalled();
  });

  it("picks nothing in a disabled group", async () => {
    const { onValueChange, user } = radio("a", true);

    screen.getByRole("radio", { name: "Alpha" }).focus();
    await user.keyboard("{ArrowDown}");
    await user.click(screen.getByRole("radio", { name: "Gamma" }));

    expect(onValueChange).not.toHaveBeenCalled();
  });
});

const LEVELS = [
  { id: "you", label: "You" },
  { id: "project", label: "Project" },
  { id: "workspace", label: "Workspace" },
];

/**
 * Settings as `SettingsTab` draws it: the caller holds the level, and a level is drawn by a
 * layout of its own (`remount`), as You, Project and Workspace are three components there, so
 * the switcher the arrow left is not the one the focus comes back to. `follows` false is a
 * caller that does not move, as Workspace is when there is no workspace to be at.
 */
function levels({ remount = false, follows = true } = {}) {
  const onLevelChange = vi.fn();
  function Settings() {
    const [level, setLevel] = useState("you");
    return (
      <>
        <button type="button">before</button>
        <button type="button" onClick={() => setLevel("workspace")}>
          to workspace
        </button>
        <SettingsLayout
          key={remount ? level : "one"}
          levels={LEVELS}
          level={level}
          onLevelChange={(to) => {
            onLevelChange(to);
            if (follows) setLevel(to);
          }}
          groups={[{ id: "look", label: "Look" }]}
          group="look"
          onGroupChange={() => {}}
          filter=""
          onFilterChange={() => {}}
        >
          <SettingGroup label="Look">{null}</SettingGroup>
        </SettingsLayout>
      </>
    );
  }
  render(<Settings />);
  return { onLevelChange, user: userEvent.setup() };
}

const level = (name: string) => screen.getByRole("radio", { name });

describe("the level switcher and the arrow keys (D-626-4)", () => {
  it("is a radio group named Level, its level shown the one checked", () => {
    levels();

    expect(screen.getByRole("radiogroup", { name: "Level" })).toBeInTheDocument();
    expect(level("You")).toHaveAttribute("aria-checked", "true");
    expect(level("Project")).toHaveAttribute("aria-checked", "false");
  });

  it("goes to the next level on a single arrow, once, as a radio choice picks", async () => {
    const { onLevelChange, user } = levels();

    level("You").focus();
    await user.keyboard("{ArrowRight}");

    expect(level("Project")).toHaveFocus();
    expect(onLevelChange).toHaveBeenCalledTimes(1);
    expect(onLevelChange).toHaveBeenCalledWith("project");
    expect(level("Project")).toHaveAttribute("aria-checked", "true");
    // The roving stop moved with it: Tab comes back to the level shown.
    expect(level("Project")).toHaveAttribute("tabindex", "0");
    expect(level("You")).toHaveAttribute("tabindex", "-1");
  });

  it("goes back on the other arrow, and wraps", async () => {
    const { onLevelChange, user } = levels();

    level("You").focus();
    await user.keyboard("{ArrowLeft}");

    expect(level("Workspace")).toHaveFocus();
    expect(onLevelChange).toHaveBeenCalledWith("workspace");
  });

  it("is one Tab stop, on the level shown, and picks nothing when Tab brings the keyboard in", async () => {
    const { onLevelChange, user } = levels();

    screen.getByRole("button", { name: "before" }).focus();
    await user.tab();
    await user.tab();

    expect(level("You")).toHaveFocus();
    expect(onLevelChange).not.toHaveBeenCalled();
    await user.tab();
    expect(screen.getByRole("searchbox", { name: "Filter settings" })).toHaveFocus();
  });

  it("keeps the keyboard on the switcher when the level is drawn by a layout of its own", async () => {
    const { onLevelChange, user } = levels({ remount: true });

    level("You").focus();
    await user.keyboard("{ArrowRight}");
    expect(level("Project")).toHaveFocus();
    await user.keyboard("{ArrowRight}");

    expect(level("Workspace")).toHaveFocus();
    expect(onLevelChange.mock.calls).toEqual([["project"], ["workspace"]]);
  });

  it("takes no focus later for an arrow whose level was not shown", async () => {
    const { onLevelChange, user } = levels({ remount: true, follows: false });

    level("You").focus();
    await user.keyboard("{ArrowLeft}");
    expect(onLevelChange).toHaveBeenCalledWith("workspace");
    const elsewhere = screen.getByRole("button", { name: "to workspace" });
    await user.click(elsewhere);

    expect(level("Workspace")).toHaveAttribute("aria-checked", "true");
    expect(elsewhere).toHaveFocus();
  });
});

describe("an option out of reach on its own (DS-3e)", () => {
  it("cannot be picked in a radio group, and says why on its title", async () => {
    const onValueChange = vi.fn();
    render(
      <SettingRow
        label="Letter"
        grouped
        control={(ids) => (
          <Choice
            ids={ids}
            kind="radio"
            options={[
              { value: "a", label: "Alpha" },
              { value: "b", label: "Beta", disabled: true, title: "Beta is not installed" },
            ]}
            value="a"
            onValueChange={onValueChange}
          />
        )}
      />,
    );
    const beta = screen.getByRole("radio", { name: "Beta" });

    expect(beta).toBeDisabled();
    expect(beta).toHaveAttribute("title", "Beta is not installed");
    await userEvent.click(beta);
    expect(onValueChange).not.toHaveBeenCalled();
  });
});

describe("a toggle", () => {
  function toggle(disabled: boolean) {
    const onCheckedChange = vi.fn();
    render(
      <SettingRow
        label="Push after saving"
        control={(ids) => (
          <Choice
            ids={ids}
            kind="toggle"
            checked={false}
            onCheckedChange={onCheckedChange}
            disabled={disabled}
          />
        )}
      />,
    );
    return { onCheckedChange, box: screen.getByRole("checkbox", { name: "Push after saving" }) };
  }

  it("answers a click with the new value", async () => {
    const { onCheckedChange, box } = toggle(false);
    await userEvent.click(box);
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("is held while what it feeds is being done", async () => {
    const { onCheckedChange, box } = toggle(true);
    expect(box).toBeDisabled();
    await userEvent.click(box);
    expect(onCheckedChange).not.toHaveBeenCalled();
  });
});

describe("checks: any of a few, each ticked on its own", () => {
  function checks() {
    const onCheckedChange = vi.fn();
    render(
      <SettingRow
        label="Repos"
        grouped
        control={(ids) => (
          <Choice
            ids={ids}
            kind="checks"
            options={[
              { value: "api", label: "api", says: "acme/api" },
              { value: "web", label: "web" },
              { value: "ops", label: "ops", disabled: true, title: "ops is not cloned" },
            ]}
            checked={new Set(["api"])}
            onCheckedChange={onCheckedChange}
          />
        )}
      />,
    );
    return { onCheckedChange };
  }

  it("is a group the row names, with a box per option ticked from the set", () => {
    checks();
    const group = screen.getByRole("group", { name: "Repos" });
    const boxes = Array.from(group.querySelectorAll<HTMLElement>("[role=checkbox]"));

    expect(boxes.map((box) => box.getAttribute("aria-checked"))).toEqual([
      "true",
      "false",
      "false",
    ]);
    expect(screen.getByRole("checkbox", { name: "api" })).toHaveAccessibleDescription("acme/api");
  });

  it("answers the option's value and whether it is ticked now", async () => {
    const { onCheckedChange } = checks();
    await userEvent.click(screen.getByRole("checkbox", { name: "web" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "api" }));
    expect(onCheckedChange.mock.calls).toEqual([
      ["web", true],
      ["api", false],
    ]);
  });

  it("holds an option out of reach, with why on its title", async () => {
    const { onCheckedChange } = checks();
    const ops = screen.getByRole("checkbox", { name: "ops" });
    expect(ops).toBeDisabled();
    expect(ops).toHaveAttribute("title", "ops is not cloned");
    await userEvent.click(ops);
    expect(onCheckedChange).not.toHaveBeenCalled();
  });
});

describe("SettingActions", () => {
  it("is the one row a form's buttons stand in, drawn as the caller wrote them", async () => {
    const onSave = vi.fn();
    render(
      <SettingActions>
        <button type="button" tabIndex={0} onClick={onSave}>
          Save
        </button>
        <button type="button" tabIndex={0} className="ends-it" disabled>
          Delete
        </button>
      </SettingActions>,
    );
    const save = screen.getByRole("button", { name: "Save" });
    const remove = screen.getByRole("button", { name: "Delete" });

    expect(save.parentElement).toHaveClass("ui-setting-actions");
    expect(remove.parentElement).toBe(save.parentElement);
    expect(remove).toHaveClass("ends-it");
    expect(remove).toBeDisabled();
    await userEvent.click(save);
    expect(onSave).toHaveBeenCalledTimes(1);
  });
});
