import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  configure,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { MemoryView, PanelRow } from "./bindings";
import { forgetDrafts } from "./memories";

/**
 * **A memory's tab, against the whole window** (SI-9b, ADR 0065): a persona's memory rows open
 * it in the strip's preview tab, a double-click keeps it, and a row's Delete archives it,
 * closes its tab and offers Undo. The pieces are `tabs.preview.test.ts`, `MemoryTab.test.tsx`
 * and `actions.test.ts`; this is where they meet the real `App`.
 */

vi.mock("./SessionPane", () => ({
  SessionPane: ({ session }: { session: number }) => (
    <div data-testid="pane" tabIndex={-1}>
      session {session}
    </div>
  ),
}));

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);
configure({ asyncUtilTimeout: 5_000 });

beforeEach(() => globalThis.localStorage.clear());
afterEach(() => {
  cleanup();
  clearMocks();
  forgetDrafts();
});

const PLANE = "/home/dev/plane";

/** The Notice a memory's Delete leaves, with its Undo (`Notice`, cause `memory-deleted`). */
const memoryUndo = () =>
  waitFor(() => {
    const line = document.querySelector<HTMLElement>('[data-cause="memory-deleted"]');
    if (!line) throw new Error("no Undo line yet");
    return line;
  });
const ALPHA = `${PLANE}/workspaces/alpha`;

const CHAT = {
  session: 1,
  name: "1",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "steward",
};

const SIDEBAR = {
  root: PLANE,
  workspaces: [{ name: "alpha", path: ALPHA, vision: "Ship it", todos: [], chats: [CHAT] }],
  personas: ["steward"],
  persona: "steward",
  unfiled: [],
};

const PERSONAS_PANEL = {
  key: "charter/personas",
  title: "Personas",
  order: 20,
  mark: "persona",
  from: null,
  about: "personas",
  blocks: [
    {
      kind: "list",
      rows: [
        {
          key: "steward",
          text: "steward",
          note: "default · 2 memories",
          mark: "persona",
          tone: "default",
          detail: null,
          runs: "persona.show:steward",
          actions: [],
        },
      ],
      empty: { headline: "No personas on this plane", body: null, offer: null },
    },
  ],
};

const TITLES: Record<string, string> = {
  "defects-go-upstream": "Charter defects go upstream",
  "never-pkill": "Never pkill by name",
};

function memoryRow(slug: string): PanelRow {
  return {
    key: slug,
    text: TITLES[slug],
    note: "2026-09-28",
    mark: "note",
    tone: "plain",
    detail: { kind: "text", text: `The body of ${TITLES[slug]}.` },
    runs: `memory.open:persona/steward/${slug}`,
    actions: [],
  };
}

function memory(slug: string): MemoryView {
  return {
    scope: { kind: "persona", name: "steward" },
    slug,
    title: TITLES[slug],
    stamp: "2026-09-28 16:05",
    place: "steward",
    body: `The body of ${TITLES[slug]}.`,
    path: `personas/steward/memory/${slug}.md`,
    text: `# ${TITLES[slug]}\n\n_2026-09-28 16:05 · persistent_\n\nThe body of ${TITLES[slug]}.\n`,
  };
}

/** `slug` where it is: steward's, or moved to the shared store. */
function inStore(slug: string, inShared: boolean): MemoryView {
  if (!inShared) return memory(slug);
  return {
    ...memory(slug),
    scope: { kind: "shared" },
    place: "shared",
    path: `personas/_shared/memory/${slug}.md`,
  };
}

/** The core. The persona's memories are the ones not archived. */
function core({ gone = [] as string[] } = {}) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const archived = new Set<string>();
  /** Memories moved to the shared store, by slug. */
  const shared = new Set<string>();
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar") return SIDEBAR;
    if (cmd === "opened_chats")
      return [{ ...CHAT, unreported: null, guessed: null, pinned: false }];
    if (cmd === "reopened_views") return [];
    if (["chat_states", "chats_that_would_not_start", "running_sessions"].includes(cmd)) return [];
    if (["extension_views", "extension_commands", "extension_panels"].includes(cmd)) return [];
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme_drawn") return null;
    if (cmd === "workspace_panels")
      return {
        workspace: "alpha",
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["steward"],
        persona: "steward",
        contributed: [PERSONAS_PANEL],
      };
    if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
    if (cmd === "vault_list") return [];
    if (cmd === "open_view")
      return {
        kind: "answered",
        blocks: [
          {
            kind: "list",
            rows: Object.keys(TITLES)
              .filter((slug) => !archived.has(slug))
              .map(memoryRow),
            empty: { headline: "Nothing remembered yet", body: null, offer: null },
          },
        ],
        took_ms: 1,
        overreach: null,
      };
    if (cmd === "memory_read") {
      const slug = String(given.slug);
      // `gone`: archived behind the window's back, still on the list it last read.
      if (archived.has(slug) || gone.includes(slug)) return null;
      const there = given.scope as { kind: string };
      return (there.kind === "shared") === shared.has(slug)
        ? inStore(slug, shared.has(slug))
        : null;
    }
    if (cmd === "memory_scopes") return [{ kind: "persona", name: "steward" }, { kind: "shared" }];
    if (cmd === "memory_move") {
      const slug = String(given.slug);
      const toShared = (given.to as { kind: string }).kind === "shared";
      if (toShared) shared.add(slug);
      else shared.delete(slug);
      return inStore(slug, toShared);
    }
    if (cmd === "memory_archive") {
      archived.add(String(given.slug));
      return { slug: given.slug, archived: `${String(given.slug)}-2` };
    }
    if (cmd === "memory_unarchive") {
      archived.delete(String(given.restoreAs));
      return memory(String(given.restoreAs));
    }
    return null;
  });
  return { asked };
}

const strip = () => screen.getByRole("tablist", { name: "Tabs" });
const tabNames = () =>
  within(strip())
    .getAllByRole("tab")
    .map((tab) => tab.textContent);

/** Opens steward's tab, and answers its list of memories. */
async function personaTab() {
  const panel = await screen.findByTestId("panel-personas");
  await userEvent.click(within(panel).getByRole("button", { name: /steward/ }));
  return screen.findByRole("list", { name: "steward" });
}

async function memoryRowIn(slug: string) {
  const list = await personaTab();
  return within(list).getByRole("button", { name: new RegExp(TITLES[slug]) });
}

describe("a persona's memory row", () => {
  it("opens the memory in a preview tab, rendered, and opens no popover", async () => {
    const { asked } = core();
    render(<App />);

    await userEvent.click(await memoryRowIn("defects-go-upstream"));

    expect(await screen.findByTestId("memory-body")).toHaveTextContent(
      "The body of Charter defects go upstream.",
    );
    expect(screen.queryByTestId("row-detail-defects-go-upstream")).toBeNull();
    const tab = within(strip()).getByRole("tab", { selected: true });
    expect(tab).toHaveTextContent("Charter defects go upstream");
    expect(tab.querySelector(".tab-name.is-preview")).not.toBeNull();
    expect(asked.find((one) => one.cmd === "memory_read")?.args).toMatchObject({
      scope: { kind: "persona", name: "steward" },
      slug: "defects-go-upstream",
    });
  });

  it("replaces what the preview tab showed on the next single click", async () => {
    core();
    render(<App />);
    await userEvent.click(await memoryRowIn("defects-go-upstream"));
    await screen.findByTestId("memory-body");

    await userEvent.click(await memoryRowIn("never-pkill"));

    await waitFor(() =>
      expect(tabNames()).toEqual(["steward 1", "steward", "Never pkill by name"]),
    );
  });

  it("keeps the preview tab on a double-click of the tab, so the next click opens beside it", async () => {
    // A row in a persona's tab is gone from the screen once its first click has brought the
    // memory's tab forward, so the tab is where this double-click lands — VS Code's own second
    // way to keep a preview. The row's double-click is `PanelList.test.tsx`'s, for the lists in
    // the side region (SI-9c).
    core();
    render(<App />);
    await userEvent.click(await memoryRowIn("defects-go-upstream"));
    await screen.findByTestId("memory-body");

    await userEvent.dblClick(
      within(strip()).getByRole("tab", { name: /Charter defects go upstream/ }),
    );
    await waitFor(() =>
      expect(
        within(strip())
          .getByRole("tab", { name: /Charter defects go upstream/ })
          .querySelector(".is-preview"),
      ).toBeNull(),
    );
    await userEvent.click(within(strip()).getByRole("tab", { name: /^steward$/ }));
    await userEvent.click(await memoryRowIn("never-pkill"));

    await waitFor(() =>
      expect(tabNames()).toEqual([
        "steward 1",
        "steward",
        "Charter defects go upstream",
        "Never pkill by name",
      ]),
    );
  });

  it("offers Open and Edit, and under the line Delete, on its menu", async () => {
    core();
    render(<App />);
    const row = await memoryRowIn("never-pkill");

    fireEvent.contextMenu(row);

    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent),
    ).toEqual([
      "Open memory: Never pkill by name",
      "Edit memory: Never pkill by name",
      "Delete memory: Never pkill by nameMoves it to the store's archive. Undo puts it back.",
    ]);
  });

  it("opens straight into the editor from Edit, and keeps the tab", async () => {
    core();
    render(<App />);
    fireEvent.contextMenu(await memoryRowIn("never-pkill"));

    await userEvent.click(await screen.findByRole("menuitem", { name: /Edit memory/ }));

    expect(await screen.findByRole("textbox", { name: "Title" })).toHaveValue(
      "Never pkill by name",
    );
    expect(
      within(strip()).getByRole("tab", { selected: true }).querySelector(".is-preview"),
    ).toBeNull();
  });
});

describe("Delete, in the window", () => {
  it("archives the memory, closes its tab, and Undo puts it back under its own slug", async () => {
    const { asked } = core();
    render(<App />);
    await userEvent.click(await memoryRowIn("never-pkill"));
    await screen.findByTestId("memory-body");

    // The heading's Delete, the same catalogue row the row's menu lists.
    await userEvent.click(screen.getByRole("button", { name: /Delete memory/ }));

    const undo = await memoryUndo();
    expect(undo).toHaveTextContent("Deleted “Never pkill by name”");
    await waitFor(() => expect(tabNames()).toEqual(["steward 1", "steward"]));
    expect(asked.find((one) => one.cmd === "memory_archive")?.args).toMatchObject({
      scope: { kind: "persona", name: "steward" },
      slug: "never-pkill",
    });
    // The persona's list reads again, and the row has gone.
    await userEvent.click(within(strip()).getByRole("tab", { name: /^steward$/ }));
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /Never pkill by name/ })).toBeNull(),
    );

    await userEvent.click(within(undo).getByRole("button", { name: "Undo" }));

    await waitFor(() => expect(document.querySelector('[data-cause="memory-deleted"]')).toBeNull());
    expect(asked.find((one) => one.cmd === "memory_unarchive")?.args).toMatchObject({
      scope: { kind: "persona", name: "steward" },
      archived: "never-pkill-2",
      restoreAs: "never-pkill",
    });
    expect(await screen.findByRole("button", { name: /Never pkill by name/ })).toBeInTheDocument();
  });

  it("asks nothing first, because nothing is lost", async () => {
    core();
    render(<App />);
    fireEvent.contextMenu(await memoryRowIn("never-pkill"));

    await userEvent.click(await screen.findByRole("menuitem", { name: /Delete memory/ }));

    await memoryUndo();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("a memory that is not there any more", () => {
  it("offers its store's archive, which takes the gone memory's tab (#1191)", async () => {
    const { asked } = core({ gone: ["never-pkill"] });
    render(<App />);
    await userEvent.click(await memoryRowIn("never-pkill"));
    const gone = await screen.findByTestId("view-gone");

    await userEvent.click(within(gone).getByRole("button", { name: "Open steward's archive" }));

    await waitFor(() =>
      expect(tabNames()).toEqual(["steward 1", "steward", "Archived memory · steward"]),
    );
    await waitFor(() =>
      expect(asked.find((one) => one.cmd === "memory_archived")?.args).toMatchObject({
        scope: { kind: "persona", name: "steward" },
      }),
    );
  });
});

describe("Move, in the window", () => {
  it("moves the memory, its tab following it, and Undo moves it back (#1190)", async () => {
    const { asked } = core();
    render(<App />);
    await userEvent.click(await memoryRowIn("never-pkill"));
    await screen.findByTestId("memory-body");

    await userEvent.selectOptions(
      await screen.findByRole("combobox", { name: "Move to" }),
      "shared",
    );
    await userEvent.click(screen.getByRole("button", { name: "Move" }));

    const line = await waitFor(() => {
      const found = document.querySelector<HTMLElement>('[data-cause="memory-moved"]');
      if (!found) throw new Error("no Undo line yet");
      return found;
    });
    expect(line).toHaveTextContent("Moved “Never pkill by name” to shared memory");
    await waitFor(() =>
      expect(screen.getByTestId("memory-meta")).toHaveTextContent(
        "personas/_shared/memory/never-pkill.md",
      ),
    );

    await userEvent.click(within(line).getByRole("button", { name: "Undo" }));

    await waitFor(() => expect(document.querySelector('[data-cause="memory-moved"]')).toBeNull());
    expect(asked.filter((one) => one.cmd === "memory_move").map((one) => one.args)).toEqual([
      expect.objectContaining({
        scope: { kind: "persona", name: "steward" },
        slug: "never-pkill",
        to: { kind: "shared" },
      }),
      expect.objectContaining({
        scope: { kind: "shared" },
        slug: "never-pkill",
        to: { kind: "persona", name: "steward" },
      }),
    ]);
    // The tab follows it back, as it followed it out.
    await waitFor(() =>
      expect(screen.getByTestId("memory-meta")).toHaveTextContent(
        "personas/steward/memory/never-pkill.md",
      ),
    );
  });
});
