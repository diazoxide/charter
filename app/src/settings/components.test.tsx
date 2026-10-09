import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Choice, SettingActions, SettingRow } from "./components";

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
