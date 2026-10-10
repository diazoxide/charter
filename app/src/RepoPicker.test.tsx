import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PlaneId } from "./bindings";
import { FORGE_LOGIN, RepoPicker, type LoginAsk } from "./RepoPicker";

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

  it("says a filter matched nothing, rather than drawing an empty list (#630)", async () => {
    mockIPC((cmd) =>
      cmd === "reachable_repos"
        ? { repos: [{ name: "api", path: "a/api", description: "" }], trouble: [] }
        : null,
    );
    render(<RepoPicker plane={"p1" as PlaneId} picked={new Set()} onPicked={() => undefined} />);
    await screen.findByRole("checkbox", { name: "api" });
    await userEvent.type(screen.getByPlaceholderText("Filter"), "zzz");
    expect(screen.getByText("No repo you can reach matches zzz.")).toBeInTheDocument();
  });

  describe("a forge that did not answer (NO-8, #1233)", () => {
    function answering(trouble: { said: string; login: string | null }[]) {
      mockIPC((cmd) => (cmd === "reachable_repos" ? { repos: [], trouble } : null));
    }
    /** What the picker asked of its project's window. */
    function asked(): LoginAsk[] {
      const heard: LoginAsk[] = [];
      const hear = (event: Event) => heard.push((event as CustomEvent<LoginAsk>).detail);
      window.addEventListener(FORGE_LOGIN, hear);
      onTestFinished(() => window.removeEventListener(FORGE_LOGIN, hear));
      return heard;
    }

    it("asks its project's window to type the login its CLI names in a shell tab", async () => {
      answering([
        {
          said: "gh is not authenticated for github.com. Run: gh auth login",
          login: "gh auth login --hostname github.com",
        },
      ]);
      const heard = asked();
      const onLeave = vi.fn();
      render(
        <RepoPicker
          plane={"p1" as PlaneId}
          picked={new Set()}
          onPicked={() => {}}
          onLeave={onLeave}
        />,
      );

      expect(await screen.findByRole("status")).toHaveTextContent("gh is not authenticated");
      await userEvent.click(screen.getByRole("button", { name: "Type it in a shell tab" }));

      expect(heard).toEqual([{ plane: "p1", line: "gh auth login --hostname github.com" }]);
      // A dialog holding the picker closes, so the shell tab is not under it.
      expect(onLeave).toHaveBeenCalledOnce();
    });

    it("offers no login where logging in would not help", async () => {
      answering([{ said: "purlis could not find gh on PATH", login: null }]);
      render(<RepoPicker plane={"p1" as PlaneId} picked={new Set()} onPicked={() => {}} />);

      expect(await screen.findByRole("status")).toHaveTextContent("could not find gh");
      expect(screen.queryByRole("button", { name: /shell tab/ })).not.toBeInTheDocument();
    });
  });
});
