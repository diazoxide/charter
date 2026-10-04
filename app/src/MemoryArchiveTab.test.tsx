import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { MemoryArchiveTab } from "./MemoryArchiveTab";
import type { ArchivedMemory, MemoryScope, MemoryView } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const STEWARD: MemoryScope = { kind: "persona", name: "steward" };

function archived(over: Partial<ArchivedMemory> = {}): ArchivedMemory {
  const archived = over.archived ?? "freeze";
  return {
    archived,
    title: "Freeze",
    stamp: "2026-09-28 16:05",
    body: "No deploys on **Friday**.\n\n<script>alert(1)</script>",
    path: `personas/steward/memory/archive/${archived}.md`,
    ...over,
  };
}

function restored(slug: string): MemoryView {
  return {
    scope: STEWARD,
    slug,
    title: "Freeze",
    stamp: "2026-09-28 16:05",
    place: "steward",
    body: "No deploys on Friday.",
    path: `personas/steward/memory/${slug}.md`,
    text: "",
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: `memory_archived` answers `lists` in order (the last one repeated), and
 *  `memory_unarchive` answers `restore`. */
function core(
  lists: (ArchivedMemory[] | Error)[],
  restore: MemoryView | Error = restored("freeze"),
) {
  const asked: Asked[] = [];
  let reads = 0;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    if (cmd === "memory_archived") {
      const answer = lists[Math.min(reads++, lists.length - 1)];
      if (answer instanceof Error) throw answer.message;
      return answer;
    }
    if (cmd === "memory_unarchive") {
      if (restore instanceof Error) throw restore.message;
      return restore;
    }
    return null;
  });
  return asked;
}

function draw(changed = 0, scope: MemoryScope = STEWARD) {
  const view = (bump: number) => <MemoryArchiveTab plane={PLANE} scope={scope} changed={bump} />;
  const drawn = render(view(changed));
  return { rerender: (bump: number) => drawn.rerender(view(bump)) };
}

describe("a store's archive tab", () => {
  it("lists what the store's archive holds, by title and stamp", async () => {
    const asked = core([[archived(), archived({ archived: "old-runbook", title: "Old runbook" })]]);
    draw();

    expect(
      await screen.findByRole("button", { name: /^Freeze\s*·\s*2026-09-28 16:05$/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /^Old runbook\s*·\s*2026-09-28 16:05$/ }),
    ).toBeInTheDocument();
    expect(asked.find((one) => one.cmd === "memory_archived")?.args).toMatchObject({
      plane: PLANE,
      scope: STEWARD,
    });
  });

  it("shows an archived memory's text read-only, rendered with no HTML", async () => {
    core([[archived()]]);
    draw();

    await userEvent.click(await screen.findByRole("button", { name: /Freeze/ }));

    const body = await screen.findByTestId("archived-body");
    expect(body.querySelector("strong")).toHaveTextContent("Friday");
    expect(body.querySelector("script")).toBeNull();
    expect(body).not.toHaveTextContent("alert(1)");
    expect(screen.getByText("personas/steward/memory/archive/freeze.md")).toBeInTheDocument();
    // Read-only: nothing to type into.
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("restores one with an explicit button, by its name in the archive, and reads the archive again", async () => {
    const asked = core([[archived()], []]);
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Freeze/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    await waitFor(() =>
      expect(asked.find((one) => one.cmd === "memory_unarchive")?.args).toMatchObject({
        plane: PLANE,
        scope: STEWARD,
        archived: "freeze",
        restoreAs: null,
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Freeze is back in steward's memory.",
    );
    expect(await screen.findByText("Nothing archived")).toBeInTheDocument();
    expect(asked.filter((one) => one.cmd === "memory_archived")).toHaveLength(2);
  });

  it("moves focus to what Restore did, so it is read out and the keyboard is not stranded", async () => {
    core([[archived()], []]);
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /^Freeze/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    const said = await screen.findByRole("status");
    await waitFor(() => expect(document.activeElement).toBe(said));
  });

  it("says what charter could not restore, and why, and keeps the memory listed", async () => {
    core([[archived()]], new Error("the store already holds freeze.md, so freeze stays archived"));
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Freeze/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "charter could not restore Freeze: the store already holds freeze.md, so freeze stays archived",
    );
    expect(screen.getByRole("button", { name: /Freeze/ })).toBeInTheDocument();
  });

  it("says when nothing is archived, naming the store", async () => {
    core([[]]);
    draw(0, { kind: "shared" });

    const empty = await screen.findByTestId("archive-empty");
    expect(empty).toHaveTextContent("Nothing archived");
    expect(empty).toHaveTextContent("A memory you delete from shared memory lands here");
  });

  it("says what charter could not read, and why", async () => {
    core([new Error("no persona 'ghost'")]);
    draw(0, { kind: "persona", name: "ghost" });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "charter could not read ghost's archive: no persona 'ghost'",
    );
  });

  it("reads the archive again when the project changes", async () => {
    const asked = core([[], [archived()]]);
    const { rerender } = draw(0);
    await screen.findByText("Nothing archived");

    rerender(1);

    const list = await screen.findByRole("list");
    expect(within(list).getByRole("button", { name: /Freeze/ })).toBeInTheDocument();
    expect(asked.filter((one) => one.cmd === "memory_archived")).toHaveLength(2);
  });
});
