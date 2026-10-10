import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { MemoryArchiveTab } from "./MemoryArchiveTab";
import { PAGE } from "./PanelList";
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
  restore: MemoryView | Error | ((args: Record<string, unknown>) => MemoryView | Error) = restored(
    "freeze",
  ),
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
      const answer =
        typeof restore === "function" ? restore(args as Record<string, unknown>) : restore;
      if (answer instanceof Error) throw answer.message;
      return answer;
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

  it("says what purlis could not restore, and why, and keeps the memory listed", async () => {
    core([[archived()]], new Error("the store already holds freeze.md, so freeze stays archived"));
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Freeze/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "purlis could not restore Freeze: the store already holds freeze.md, so freeze stays archived",
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

  it("says what purlis could not read, and why", async () => {
    core([new Error("no persona 'ghost'")]);
    draw(0, { kind: "persona", name: "ghost" });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "purlis could not read ghost's archive: no persona 'ghost'",
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

/** `count` archived memories, `note-01` to `note-<count>`, each with its own body. */
function many(count: number): ArchivedMemory[] {
  return Array.from({ length: count }, (_, at) => {
    const n = String(at + 1).padStart(2, "0");
    return archived({ archived: `note-${n}`, title: `Note ${n}`, body: `Body of note ${n}.` });
  });
}

describe("a long archive (#1191)", () => {
  it("draws a page of rows and offers the rest with Show more", async () => {
    core([many(PAGE + 1)]);
    draw();

    const list = await screen.findByRole("list");
    expect(within(list).getAllByRole("button")).toHaveLength(PAGE);
    expect(within(list).queryByRole("button", { name: /Note 13/ })).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: /^Show 1 more/ }));

    expect(within(list).getAllByRole("button")).toHaveLength(PAGE + 1);
    expect(within(list).getByRole("button", { name: /Note 13/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Show \d+ more/ })).toBeNull();
  });

  it("has no search box and no Show more while the archive fits on a page", async () => {
    core([many(PAGE)]);
    draw();

    const list = await screen.findByRole("list");
    expect(within(list).getAllByRole("button")).toHaveLength(PAGE);
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(screen.queryByRole("button", { name: /^Show \d+ more/ })).toBeNull();
  });

  it("searches what an archived memory says, its body too, ignoring case", async () => {
    const rows = many(PAGE + 1);
    rows[12] = { ...rows[12], body: "The Friday FREEZE holds until the release." };
    core([rows]);
    draw();

    const box = await screen.findByRole("searchbox", {
      name: "Search archived memories in steward",
    });
    await userEvent.type(box, "friday freeze");

    const list = screen.getByRole("list");
    expect(within(list).getAllByRole("button")).toHaveLength(1);
    expect(within(list).getByRole("button", { name: /Note 13/ })).toBeInTheDocument();
  });

  it("finds by title and by stamp, and shows every match past a page", async () => {
    const rows = many(PAGE * 2);
    rows[20] = { ...rows[20], stamp: "2026-10-01 09:00" };
    core([rows]);
    draw();
    const box = await screen.findByRole("searchbox");

    await userEvent.type(box, "note");
    expect(within(screen.getByRole("list")).getAllByRole("button")).toHaveLength(PAGE * 2);
    expect(screen.queryByRole("button", { name: /^Show \d+ more/ })).toBeNull();

    await userEvent.clear(box);
    await userEvent.type(box, "2026-10-01");
    expect(within(screen.getByRole("list")).getAllByRole("button")).toHaveLength(1);
  });

  it("says when a search finds nothing, and keeps the box", async () => {
    core([many(PAGE + 1)]);
    draw();
    const box = await screen.findByRole("searchbox");

    await userEvent.type(box, "nowhere");

    expect(screen.getByText("Nothing here matches “nowhere”.")).toBeInTheDocument();
    expect(screen.getByRole("searchbox")).toBeInTheDocument();
  });

  it("keeps the chosen memory's text drawn when a search hides its row", async () => {
    core([many(PAGE + 1)]);
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Note 01/ }));

    await userEvent.type(screen.getByRole("searchbox"), "note 13");

    expect(within(screen.getByRole("list")).queryByRole("button", { name: /Note 01/ })).toBeNull();
    expect(screen.getByTestId("archived-body")).toHaveTextContent("Body of note 01.");
    expect(screen.getByRole("button", { name: "Restore memory" })).toBeInTheDocument();
  });
});

describe("restoring a numbered archive name (#1191, D-1191-1)", () => {
  const numbered = () => [
    archived({ archived: "freeze", title: "Freeze" }),
    archived({ archived: "freeze-2", title: "Freeze again" }),
  ];

  it("restores it under the name archiving numbered it away from", async () => {
    const asked = core([numbered()], restored("freeze"));
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Freeze again/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Freeze again is back in steward's memory as freeze.",
    );
    const sent = asked.filter((one) => one.cmd === "memory_unarchive");
    expect(sent).toHaveLength(1);
    expect(sent[0].args).toMatchObject({ archived: "freeze-2", restoreAs: "freeze" });
  });

  it("falls back to its archived name when the store already holds the other", async () => {
    const asked = core([numbered()], (args) =>
      args.restoreAs === "freeze"
        ? new Error("the store already holds freeze.md, so freeze-2 stays archived")
        : restored("freeze-2"),
    );
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Freeze again/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Freeze again is back in steward's memory as freeze-2, since it already holds a freeze.",
    );
    const sent = asked.filter((one) => one.cmd === "memory_unarchive").map((one) => one.args);
    expect(sent).toMatchObject([
      { archived: "freeze-2", restoreAs: "freeze" },
      { archived: "freeze-2", restoreAs: null },
    ]);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says any other refusal, and does not try a second name", async () => {
    const asked = core([numbered()], new Error("the store is read-only"));
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Freeze again/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "purlis could not restore Freeze again: the store is read-only",
    );
    expect(asked.filter((one) => one.cmd === "memory_unarchive")).toHaveLength(1);
  });

  it("restores a name the core never numbered under that name", async () => {
    const asked = core([[archived({ archived: "release-2026", title: "Release" })]]);
    draw();
    await userEvent.click(await screen.findByRole("button", { name: /Release/ }));

    await userEvent.click(screen.getByRole("button", { name: "Restore memory" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Release is back in steward's memory.",
    );
    expect(asked.find((one) => one.cmd === "memory_unarchive")?.args).toMatchObject({
      archived: "release-2026",
      restoreAs: null,
    });
  });
});
