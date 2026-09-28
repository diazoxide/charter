import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { MemoryTab } from "./MemoryTab";
import { forgetDrafts, memoryKey, wantEdit, type MemoryRef } from "./memories";
import type { MemoryEdited, MemoryView } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
  forgetDrafts();
});

const PLANE = "/home/dev/plane";
const REF: MemoryRef = { scope: { kind: "persona", name: "steward" }, slug: "where-prod-1-is" };
const KEY = memoryKey(REF);

function memory(over: Partial<MemoryView> = {}): MemoryView {
  const title = over.title ?? "Where prod-1 is";
  const body = over.body ?? "It lives in **eu-west-1**.\n\n<script>alert(1)</script>";
  return {
    scope: REF.scope,
    slug: REF.slug,
    title,
    stamp: "2026-09-28 16:05",
    place: "steward",
    body,
    path: "personas/steward/memory/where-prod-1-is.md",
    text: `# ${title}\n\n_2026-09-28 16:05 · persistent_\n\n${body}\n`,
    ...over,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: `memory_read` answers `read` (in order, the last one repeated), and each write
 *  answers what `writes` says for it. */
function core(
  read: (MemoryView | null)[],
  writes: Record<string, unknown | ((args: Record<string, unknown>) => unknown)> = {},
): Asked[] {
  const asked: Asked[] = [];
  let reads = 0;
  mockIPC((cmd, args) => {
    const given = args as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "memory_read") return read[Math.min(reads++, read.length - 1)];
    const answer = writes[cmd];
    if (typeof answer === "function") return answer(given);
    if (answer instanceof Error) throw answer.message;
    return answer ?? null;
  });
  return asked;
}

function draw(
  over: {
    at?: MemoryRef;
    changed?: number;
    onSaved?: (memory: MemoryView) => void;
    onClose?: () => void;
  } = {},
) {
  const onSaved = over.onSaved ?? vi.fn();
  const onClose = over.onClose ?? vi.fn();
  const view = (changed: number) => (
    <MemoryTab
      plane={PLANE}
      at={over.at ?? REF}
      changed={changed}
      onSaved={onSaved}
      onClose={onClose}
    />
  );
  const drawn = render(view(over.changed ?? 0));
  return { onSaved, onClose, rerender: (changed: number) => drawn.rerender(view(changed)) };
}

describe("a memory's tab", () => {
  it("says where the memory is, and renders its body as Markdown with no HTML", async () => {
    core([memory()]);
    draw();

    const body = await screen.findByTestId("memory-body");
    expect(body.querySelector("strong")).toHaveTextContent("eu-west-1");
    expect(body.querySelector("script")).toBeNull();
    expect(body).not.toHaveTextContent("alert(1)");
    // The heading and the stamp line are the header's, not repeated in the body.
    expect(body).not.toHaveTextContent("Where prod-1 is");
    expect(body).not.toHaveTextContent("persistent");
    const meta = screen.getByTestId("memory-meta");
    expect(meta).toHaveTextContent("steward");
    expect(meta).toHaveTextContent("2026-09-28 16:05");
    expect(meta).toHaveTextContent("personas/steward/memory/where-prod-1-is.md");
  });

  it("says a memory that is not there any more is gone, rather than failing", async () => {
    core([null]);
    draw();

    expect(await screen.findByTestId("view-gone")).toHaveTextContent(/not here any more/);
  });

  it("reads the memory again when the plane changes on disk", async () => {
    const asked = core([memory(), memory({ body: "Moved to eu-west-2." })]);
    const { rerender } = draw();
    await screen.findByTestId("memory-body");

    rerender(1);

    await waitFor(() => expect(screen.getByTestId("memory-body")).toHaveTextContent("eu-west-2"));
    expect(asked.filter((one) => one.cmd === "memory_read")).toHaveLength(2);
  });
});

describe("editing a memory", () => {
  async function editing(read: MemoryView[] = [memory()], writes = {}) {
    const asked = core(read, writes);
    const drawn = draw();
    await screen.findByTestId("memory-body");
    act(() => wantEdit(PLANE, KEY));
    await screen.findByRole("textbox", { name: "Title" });
    return { asked, ...drawn };
  }

  it("flips the same tab into a title field and the raw body, and caps the title at 72", async () => {
    await editing();

    const title = screen.getByRole("textbox", { name: "Title" });
    expect(title).toHaveValue("Where prod-1 is");
    expect(title).toHaveAttribute("maxLength", "72");
    expect(screen.getByRole("textbox", { name: "Body" })).toHaveValue(
      "It lives in **eu-west-1**.\n\n<script>alert(1)</script>",
    );
    expect(screen.queryByTestId("memory-body")).toBeNull();
  });

  it("opens straight into the editor when the edit was asked for before the read arrived", async () => {
    core([memory()]);
    act(() => wantEdit(PLANE, KEY));
    draw();

    expect(await screen.findByRole("textbox", { name: "Title" })).toHaveValue("Where prod-1 is");
  });

  it("saves against the text it read, and shows what was saved", async () => {
    const saved = memory({ title: "Where prod-1 lives", body: "eu-west-2 since May" });
    const { asked, onSaved } = await editing([memory()], {
      memory_edit: { kind: "saved", memory: saved } satisfies MemoryEdited,
    });

    await userEvent.clear(screen.getByRole("textbox", { name: "Title" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Title" }), "Where prod-1 lives");
    await userEvent.clear(screen.getByRole("textbox", { name: "Body" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Body" }), "eu-west-2 since May");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(screen.getByTestId("memory-body")).toHaveTextContent("eu-west-2"));
    const edit = asked.find((one) => one.cmd === "memory_edit");
    expect(edit?.args).toMatchObject({
      scope: REF.scope,
      slug: REF.slug,
      title: "Where prod-1 lives",
      text: "eu-west-2 since May",
      read: memory().text,
      overwrite: false,
    });
    expect(onSaved).toHaveBeenCalledWith(saved);
  });

  it("keeps what was typed when the operator looks at another tab and comes back", async () => {
    // Only the tab in front is on screen, so the tab is unmounted in between.
    await editing();
    await userEvent.type(screen.getByRole("textbox", { name: "Body" }), " typed");
    cleanup();

    draw();

    expect(await screen.findByRole("textbox", { name: "Body" })).toHaveValue(
      "It lives in **eu-west-1**.\n\n<script>alert(1)</script> typed",
    );
  });

  it("cancels back to the memory as it was, and nothing is written", async () => {
    const { asked } = await editing();
    await userEvent.type(screen.getByRole("textbox", { name: "Body" }), " typed");

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(await screen.findByTestId("memory-body")).not.toHaveTextContent("typed");
    expect(asked.some((one) => one.cmd === "memory_edit")).toBe(false);
  });

  it("says the core's refusal and stays in the editor", async () => {
    await editing([memory()], { memory_edit: new Error("empty memory") });
    await userEvent.clear(screen.getByRole("textbox", { name: "Body" }));

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("empty memory");
    expect(screen.getByRole("textbox", { name: "Body" })).toBeInTheDocument();
  });
});

describe("a save the file changed under", () => {
  const theirs = memory({ body: "Changed by a chat meanwhile." });

  async function stale(writes: Record<string, unknown> = {}) {
    const asked = core([memory()], {
      memory_edit: (args: Record<string, unknown>) =>
        args.overwrite
          ? { kind: "saved", memory: memory({ body: "mine" }) }
          : { kind: "stale", now: theirs },
      ...writes,
    });
    draw();
    await screen.findByTestId("memory-body");
    act(() => wantEdit(PLANE, KEY));
    await userEvent.clear(await screen.findByRole("textbox", { name: "Body" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Body" }), "mine");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await screen.findByRole("alert");
    return asked;
  }

  it("is refused, says so, and keeps what was typed", async () => {
    await stale();

    expect(screen.getByRole("alert")).toHaveTextContent(/changed on disk/);
    expect(screen.getByRole("textbox", { name: "Body" })).toHaveValue("mine");
    expect(screen.getByRole("button", { name: "Reload" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Overwrite" })).toBeInTheDocument();
  });

  it("reloads to what is on disk now, dropping the edit", async () => {
    await stale();

    await userEvent.click(screen.getByRole("button", { name: "Reload" }));

    expect(await screen.findByTestId("memory-body")).toHaveTextContent("Changed by a chat");
    expect(screen.queryByRole("textbox", { name: "Body" })).toBeNull();
  });

  it("overwrites what is on disk when told to", async () => {
    const asked = await stale();

    await userEvent.click(screen.getByRole("button", { name: "Overwrite" }));

    expect(await screen.findByTestId("memory-body")).toHaveTextContent("mine");
    const edits = asked.filter((one) => one.cmd === "memory_edit");
    expect(edits.map((one) => one.args.overwrite)).toEqual([false, true]);
  });

  it("offers no Overwrite when the memory is not there any more", async () => {
    await stale({ memory_edit: { kind: "stale", now: null } });

    expect(screen.getByRole("alert")).toHaveTextContent(/not there any more/);
    expect(screen.queryByRole("button", { name: "Overwrite" })).toBeNull();
  });
});

describe("a new memory's tab", () => {
  const DRAFT_AT: MemoryRef = { scope: { kind: "shared" }, slug: "+" };

  it("opens in the editor, reads nothing, and writes through create", async () => {
    const made = memory({ scope: { kind: "shared" }, place: "shared", slug: "freeze" });
    const asked = core([null], { memory_create: made });
    const { onSaved } = draw({ at: DRAFT_AT });

    await userEvent.type(await screen.findByRole("textbox", { name: "Title" }), "Freeze");
    await userEvent.type(screen.getByRole("textbox", { name: "Body" }), "No deploys on Friday");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onSaved).toHaveBeenCalledWith(made));
    expect(asked.some((one) => one.cmd === "memory_read")).toBe(false);
    expect(asked.find((one) => one.cmd === "memory_create")?.args).toMatchObject({
      scope: { kind: "shared" },
      title: "Freeze",
      text: "No deploys on Friday",
    });
  });

  it("closes its tab on Cancel, since there is nothing to go back to", async () => {
    core([null]);
    const { onClose } = draw({ at: DRAFT_AT });

    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(onClose).toHaveBeenCalled();
  });
});
