import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PlaneId } from "./bindings";
import { RepoPicker } from "./RepoPicker";

afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

describe("the repo picker", () => {
  it("draws two repos with one name as two boxes, each named and ticked on its own", async () => {
    // Two owners can each have an `api` (`reachable_in` lists `a/api` and `b/api`): each is
    // its own box, its own label pointing at it, and no key React has to drop (D-DS3e-10).
    const warned = vi.spyOn(console, "error").mockImplementation(() => {});
    mockIPC((cmd) =>
      cmd === "reachable_repos"
        ? {
            repos: [
              { name: "api", path: "a/api", description: "" },
              { name: "api", path: "b/api", description: "" },
            ],
            trouble: [],
          }
        : null,
    );
    const picked: Set<string>[] = [];
    render(
      <RepoPicker plane={"p1" as PlaneId} picked={new Set()} onPicked={(n) => picked.push(n)} />,
    );

    const boxes = await screen.findAllByRole("checkbox", { name: "api" });
    expect(boxes).toHaveLength(2);
    expect(new Set(boxes.map((box) => box.id)).size).toBe(2);
    for (const box of boxes) {
      expect(document.querySelectorAll(`[id="${box.id}"]`)).toHaveLength(1);
      expect(document.querySelector(`label[for="${box.id}"]`)).toHaveTextContent("api");
    }
    expect(boxes[0]).toHaveAccessibleDescription("a/api");
    expect(boxes[1]).toHaveAccessibleDescription("b/api");
    expect(warned.mock.calls.flat().join(" ")).not.toMatch(/same key/);

    await userEvent.click(boxes[1]);
    expect(picked).toEqual([new Set(["api"])]);
  });
});
