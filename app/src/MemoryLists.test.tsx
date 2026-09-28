import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, configure, render as renderBare, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { MemoryView, PanelRow } from "./bindings";
import { forgetDrafts } from "./memories";

/**
 * **A workspace's Memory section, against the whole window** (SI-9c, ADR 0065 Q5, Q9, Q10): the
 * `+` on its heading opens a new memory's tab in edit mode, Save writes it through
 * `memory_create`, and the section — read again on the trigger Todos is — lists it. The pieces
 * are `Panels.test.tsx` and `actions.test.ts`; this is where they meet the real `App`.
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

/** The Memory section as `panels::memory_panel` builds it, from what the journal holds. */
function memoryPanel(journal: { slug: string; title: string }[]) {
  return {
    key: "charter/memory",
    title: "Memory",
    order: 15,
    mark: "note",
    from: null,
    about: null,
    blocks: [
      {
        kind: "list",
        rows: journal.map(({ slug, title }): PanelRow => ({
          key: slug,
          text: title,
          note: "2026-09-28 16:05",
          mark: "note",
          tone: "plain",
          detail: { kind: "text", text: title },
          runs: `memory.open:workspace/alpha/${slug}`,
          actions: [],
        })),
        empty: { headline: "Nothing remembered yet", body: null, offer: null },
      },
    ],
  };
}

/** The core, whose alpha journal is what `memory_create` has written to it. */
function core() {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  const journal: { slug: string; title: string }[] = [];
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
        contributed: [memoryPanel([...journal].reverse())],
      };
    if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
    if (cmd === "vault_list") return [];
    if (cmd === "memory_create") {
      const title = String(given.title);
      const slug = `20260928-160500-${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`;
      journal.push({ slug, title });
      const made: MemoryView = {
        scope: { kind: "workspace", name: "alpha" },
        slug,
        title,
        stamp: "2026-09-28 16:05",
        place: "alpha",
        body: String(given.text),
        path: `workspaces/alpha/memory/${slug}.md`,
        text: `# ${title}\n\n_2026-09-28 16:05 · note_\n\n${String(given.text)}\n`,
      };
      return made;
    }
    return null;
  });
  return { asked };
}

describe("a workspace's Memory section", () => {
  it("makes a memory from its +, and lists it once it is saved", { timeout: 20_000 }, async () => {
    const { asked } = core();
    render(<App />);
    const section = await screen.findByTestId("panel-memory");
    expect(within(section).getByText("Nothing remembered yet")).toBeInTheDocument();

    await userEvent.click(within(section).getByRole("button", { name: "New memory in alpha…" }));
    // Typed with `skipClick`: in jsdom every element is at 0,0, and the region frame's
    // pointer handling takes a click anywhere for its first separator.
    const title = await screen.findByRole("textbox", { name: "Title" });
    title.focus();
    await userEvent.type(title, "Deploys freeze", { skipClick: true });
    const body = screen.getByRole("textbox", { name: "Body" });
    body.focus();
    await userEvent.type(body, "On Fridays.", { skipClick: true });
    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(
      await within(screen.getByTestId("panel-memory")).findByRole("button", {
        name: /Deploys freeze/,
      }),
    ).toBeInTheDocument();
    expect(asked.find((one) => one.cmd === "memory_create")?.args).toMatchObject({
      scope: { kind: "workspace", name: "alpha" },
      title: "Deploys freeze",
      text: "On Fridays.",
    });
  });
});
