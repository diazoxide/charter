import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { SavingView } from "./SavingView";
import type { PlaneSaving, SaveEntry } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function standing(over: Partial<PlaneSaving> = {}): PlaneSaving {
  return {
    stage: "saved",
    changed: [],
    ahead: 0,
    pr: null,
    blocked: null,
    branch: "main",
    pushes: true,
    mode: "push",
    modeFrom: "charter.toml",
    journal: [],
    ...over,
  };
}

function entry(over: Partial<SaveEntry> = {}): SaveEntry {
  return {
    at: 1_790_000_000,
    trigger: "manual",
    mode: "push",
    files: 1,
    commit: "1f488c66767d3706b46bcef51ce4921fa981a2e4",
    pr: null,
    outcome: "saved",
    detail: "",
    ...over,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: `plane_saving` answers each of `reads` in turn (the last one again after that),
 *  and `save_plane` answers `saved` — lines, or an `Error` whose message is the refusal. */
function core(reads: PlaneSaving[], saved: string[] | Error = ["✓ Committed"]): Asked[] {
  const asked: Asked[] = [];
  let read = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    if (cmd === "plane_saving") return reads[Math.min(read++, reads.length - 1)];
    if (cmd === "save_plane") {
      if (saved instanceof Error) throw saved.message;
      return saved;
    }
    return null;
  });
  return asked;
}

describe("SavingView", () => {
  it("says how much is unsaved and lists what the next save takes", async () => {
    core([standing({ stage: "changed", changed: ["notes/a.md", "todos/b.json"] })]);
    render(<SavingView plane={PLANE} />);

    expect(await screen.findByText("2 changed")).toBeTruthy();
    const files = screen.getByRole("list", { name: "What the next save takes" });
    expect(
      within(files)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(["notes/a.md", "todos/b.json"]);
    expect(screen.getByText("Mode: push (charter.toml) — a save pushes to main")).toBeTruthy();
  });

  it("saves with the message typed, then reads the plane again", async () => {
    const asked = core([
      standing({ stage: "changed", changed: ["a.md"] }),
      standing({ journal: [entry()] }),
    ]);
    const onSaved = vi.fn();
    render(<SavingView plane={PLANE} onSaved={onSaved} />);
    await screen.findByText("1 changed");

    await userEvent.type(screen.getByRole("textbox", { name: "Message" }), "by hand");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Saved")).toBeTruthy();
    expect(asked.find((a) => a.cmd === "save_plane")?.args).toEqual({
      plane: PLANE,
      message: "by hand",
    });
    expect(onSaved).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status").textContent).toContain("✓ Committed");
  });

  it("asks for the generated message when none is typed", async () => {
    const asked = core([standing({ stage: "changed", changed: ["a.md"] })]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("1 changed");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(asked.some((a) => a.cmd === "save_plane")).toBe(true));
    expect(asked.find((a) => a.cmd === "save_plane")?.args.message).toBeNull();
  });

  it("shows a refusal in the core's words", async () => {
    core(
      [standing({ stage: "changed", changed: ["a.md"] })],
      new Error("Refusing to save — a secret"),
    );
    render(<SavingView plane={PLANE} />);
    await screen.findByText("1 changed");

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect((await screen.findByRole("alert")).textContent).toContain("Refusing to save — a secret");
  });

  it("names why a blocked plane is blocked, and a plane with nothing to save offers no save", async () => {
    core([standing({ stage: "blocked", blocked: "CONFLICT in a.md" })]);
    render(<SavingView plane={PLANE} />);
    expect(await screen.findByText("Blocked: CONFLICT in a.md")).toBeTruthy();
    cleanup();

    core([standing()]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("Saved");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("says what a plane with no mode will do, and where a save that cannot push stops", async () => {
    core([standing({ mode: null, modeFrom: "default" })]);
    render(<SavingView plane={PLANE} />);
    expect(await screen.findByText("Mode: not set — a save pushes to main")).toBeTruthy();
    cleanup();

    core([standing({ mode: "commit", pushes: false })]);
    render(<SavingView plane={PLANE} />);
    expect(
      await screen.findByText("Mode: commit (charter.toml) — a save commits and goes no further"),
    ).toBeTruthy();
  });

  it("offers a save for commits only when a save would push them", async () => {
    core([standing({ stage: "committed", ahead: 2 })]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("2 committed, not pushed");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(
      false,
    );
    cleanup();

    core([standing({ stage: "committed", ahead: 2, pushes: false })]);
    render(<SavingView plane={PLANE} />);
    await screen.findByText("2 committed, not pushed");
    expect((screen.getByRole("button", { name: "Save" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("lists the last saves, newest first, each with how it ended", async () => {
    core([
      standing({
        journal: [
          entry({ outcome: "blocked", detail: "a secret-shaped value", files: 2, commit: null }),
          entry({ outcome: "saved", trigger: "cli" }),
        ],
      }),
    ]);
    render(<SavingView plane={PLANE} />);

    const rows = within(await screen.findByRole("list", { name: "Recent saves" })).getAllByRole(
      "listitem",
    );
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("blocked");
    expect(rows[0].textContent).toContain("a secret-shaped value");
    expect(rows[1].textContent).toContain("saved");
    expect(rows[1].textContent).toContain("cli");
    expect(rows[1].textContent).toContain("1f488c6");
  });
});
