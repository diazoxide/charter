import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { AnswerBar } from "./AnswerBar";

afterEach(cleanup);

describe("AnswerBar", () => {
  it("is the one row a question's buttons stand in, in the order the dialog wrote them", () => {
    render(
      <AnswerBar>
        <button type="button" tabIndex={0}>
          Cancel
        </button>
        <button type="button" tabIndex={0} className="ends-it">
          Delete
        </button>
      </AnswerBar>,
    );
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const remove = screen.getByRole("button", { name: "Delete" });

    // `.answer` is what App.css lays out: a flex row at the trailing edge, with a gap.
    expect(cancel.parentElement).toHaveClass("answer");
    expect(remove.parentElement).toBe(cancel.parentElement);
    expect([...(cancel.parentElement?.children ?? [])]).toEqual([cancel, remove]);
    expect(remove).toHaveClass("ends-it");
  });

  it("holds what is not a button too, such as a line that says what a press did", () => {
    render(
      <AnswerBar>
        <span role="status">Copied.</span>
        <button type="button">Close</button>
      </AnswerBar>,
    );
    expect(screen.getByRole("status").parentElement).toHaveClass("answer");
  });

  it("leaves the dialog's keys to the dialog: Escape is its Cancel, Return the focused answer", async () => {
    const onCancel = vi.fn();
    const onDelete = vi.fn();
    render(
      <AlertDialog.Root open onOpenChange={(open) => !open && onCancel()}>
        <AlertDialog.Portal>
          <AlertDialog.Content aria-describedby={undefined}>
            <AlertDialog.Title>Delete it?</AlertDialog.Title>
            <AnswerBar>
              <AlertDialog.Cancel asChild>
                <button type="button" tabIndex={0}>
                  Cancel
                </button>
              </AlertDialog.Cancel>
              <button type="button" tabIndex={0} className="ends-it" onClick={onDelete}>
                Delete
              </button>
            </AnswerBar>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>,
    );
    // Radix focuses Cancel in an alert dialog: the act that ends something is not Return's.
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(onDelete).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledTimes(1);

    onCancel.mockClear();
    await userEvent.keyboard("{Escape}");
    expect(onDelete).not.toHaveBeenCalled();
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
