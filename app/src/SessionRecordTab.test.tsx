import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SessionRecordTab } from "./SessionRecordTab";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const PATH = "workspaces/ide/sessions/2026-10-09-a.md";

describe("a session record's tab", () => {
  it("says why its record could not be read, and Read again asks again (NO-8, #1296)", async () => {
    const asked: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd !== "session_record") return null;
      asked.push(args);
      if (asked.length === 1) throw "the record could not be read: permission denied";
      return null;
    });
    render(<SessionRecordTab plane={PLANE} path={PATH} />);

    expect(await screen.findByText(/permission denied/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Read again" }));

    // The second read answers that nothing is there, which is not an error.
    expect(await screen.findByText("This session record is not here any more")).toBeTruthy();
    expect(asked).toEqual([
      { plane: PLANE, path: PATH },
      { plane: PLANE, path: PATH },
    ]);
  });
});
