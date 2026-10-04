import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { Choice, SettingRow } from "./components";

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
