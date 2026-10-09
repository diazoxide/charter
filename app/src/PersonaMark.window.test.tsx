import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  configure,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import App from "./App";
import type { PersonaMark as Mark } from "./bindings";
import { findStripNamed, stripNamed } from "./test-strips";

/**
 * **A persona's icon and colour, wherever the persona appears** (#1449), against the whole
 * window: its chat's tab, that chat's row in the explorer, its row in the Personas panel, its
 * own view's tab and heading, a needs-you item, a Notice that names it, and the new-chat
 * picker. One component draws all seven (`PersonaMark`), from one read of the plane's marks.
 *
 * `devops` declares a rocket on teal. `qa` declares nothing, and is its initials. The core is
 * recorded answers; what is asserted is what the window draws and what it asks the core to do.
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
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const chat = (session: number, persona: string | null) => ({
  session,
  name: String(session),
  cwd: ALPHA,
  harness: "claude",
  in_front: session === 1,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona,
  unreported: null,
  guessed: null,
  card: null,
  pinned: false,
  label: null,
  from: null,
});

const row = (name: string) => ({
  key: name,
  text: name,
  note: "0 memories",
  mark: "persona",
  tone: "plain",
  detail: null,
  runs: `persona.show:${name}`,
  actions: [],
});

const mark = (name: string, more: Partial<Mark> = {}): Mark => ({
  name,
  icon: null,
  colour: null,
  image: null,
  trouble: [],
  ...more,
});

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core. `marks` is what `persona_marks` answers now; a pick writes into it, as the
 *  definition on disk would be written. */
function core(marks: Mark[], chats = [chat(1, "devops"), chat(2, null)]) {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    const given = (args ?? {}) as Record<string, unknown>;
    asked.push({ cmd, args: given });
    if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
    if (cmd === "plane_sidebar")
      return {
        root: PLANE,
        workspaces: [
          { name: "alpha", path: ALPHA, vision: "", todos: [], chats, colour: null, live: false },
        ],
        personas: ["devops", "qa"],
        persona: "devops",
        unfiled: [],
      };
    if (cmd === "persona_marks") return marks;
    if (cmd === "persona_mark_set") {
      marks = marks.map((one) =>
        one.name === given.name
          ? { ...one, icon: given.icon as string | null, colour: given.colour as string | null }
          : one,
      );
      return null;
    }
    if (cmd === "opened_chats") return chats;
    if (cmd === "reopened_views") return [];
    if (cmd === "chat_states")
      return [
        {
          plane: PLANE,
          session: 1,
          state: "waiting",
          needs_you: true,
          queue: [1],
          sequence: 1,
          moved_at: 1,
          reports: [],
          refusals: [],
          children: [],
        },
      ];
    if (cmd === "chats_that_would_not_start") return [];
    if (cmd === "running_sessions") return [];
    if (cmd === "extension_views") return [];
    if (cmd === "extension_commands") return [];
    if (cmd === "extension_panels") return [];
    if (cmd === "extensions_on") return [];
    if (cmd === "project_theme_drawn") return null;
    // The chat as devops was handed off, and holds the asking chat's hosts (#1362).
    if (cmd === "persona_grants_held")
      return given.session === 1
        ? { from: "steward 3", persona: "devops", locked: null, waits_here: false }
        : null;
    if (cmd === "workspace_panels")
      return {
        workspace: "alpha",
        repos: [],
        paths: {},
        absent: [],
        refused: [],
        todos: [],
        todos_refused: null,
        personas: ["devops", "qa"],
        persona: "devops",
        sessions: [],
        contributed: [
          {
            key: "charter/personas",
            title: "Personas",
            order: 20,
            mark: "persona",
            from: null,
            about: "personas",
            blocks: [
              {
                kind: "list",
                rows: [row("devops"), row("qa")],
                empty: { headline: "No personas on this plane", body: null, offer: null },
              },
            ],
          },
        ],
      };
    if (cmd === "workspace_repos") return { workspace: "alpha", repos: [], cache_refused: null };
    if (cmd === "open_view")
      return {
        kind: "answered",
        blocks: [{ kind: "note", text: "It remembers 0 things.", tone: "plain" }],
        took_ms: 1,
        overreach: null,
      };
    if (cmd === "start_options")
      return {
        profiles: [
          {
            name: "claude",
            kind: "claude",
            shown: "claude",
            source: "built-in",
            is_default: true,
            approval: null,
          },
        ],
        refused: [],
        personas: ["devops", "qa"],
        persona: "devops",
        persona_profiles: {},
        ignore_fix: null,
        declares_none: true,
      };
    return null;
  });
  return asked;
}

const DECLARED = [mark("devops", { icon: "rocket", colour: "teal" }), mark("qa")];

/** The marks of `persona` inside `within`, as what each shows. */
const marksIn = (inside: Element, persona: string) =>
  [...inside.querySelectorAll<HTMLElement>(`.persona-mark[data-persona="${persona}"]`)].map(
    (one) => ({
      mark: one.dataset.mark,
      icon: one.dataset.icon,
      initials: one.dataset.initials,
      colour: one.dataset.colour,
    }),
  );

const ROCKET_ON_TEAL = { mark: "icon", icon: "rocket", initials: undefined, colour: "teal" };

const strip = () => stripNamed("Tabs");

/** The window, drawn, with its chats on the strip and the plane's marks read. */
async function opened(marks = DECLARED) {
  const asked = core(marks);
  render(<App />);
  const tab = await within(await findStripNamed("Tabs")).findByRole("tab", {
    name: /devops 1/,
  });
  await waitFor(() => expect(marksIn(tab, "devops")).toEqual([ROCKET_ON_TEAL]));
  return { asked, tab };
}

describe("a persona with an icon and a colour is drawn with them", () => {
  it("on its chat's tab, where a chat that runs as no persona wears no mark", async () => {
    const { tab } = await opened();

    expect(marksIn(tab, "devops")).toEqual([ROCKET_ON_TEAL]);
    // Its name is still its name: the mark adds no words to the tab.
    expect(tab.querySelector(".tab-name")).toHaveTextContent(/^devops 1$/);
    const plain = within(strip())
      .getAllByRole("tab")
      .find((one) => one.querySelector(".tab-name")?.textContent === "claude 2");
    expect(plain).toBeDefined();
    expect(plain?.querySelector(".persona-mark")).toBeNull();
  });

  it("on that chat's row in the explorer, in place of the terminal a chat with none keeps", async () => {
    await opened();

    const explorer = await screen.findByTestId("explorer");
    const rows = [...explorer.querySelectorAll<HTMLElement>("button.chat")];
    const mine = rows.find((one) => one.textContent?.includes("devops 1"));
    const plain = rows.find((one) => one.textContent?.includes("claude 2"));
    expect(mine && marksIn(mine, "devops")).toEqual([ROCKET_ON_TEAL]);
    expect(mine?.querySelector("svg.lucide-square-terminal")).toBeNull();
    expect(plain?.querySelector(".persona-mark")).toBeNull();
    expect(plain?.querySelector("svg.lucide-square-terminal")).not.toBeNull();
  });

  it("on its row in the Personas panel, beside a persona that is its initials", async () => {
    await opened();

    const panel = await screen.findByTestId("panel-personas");
    expect(marksIn(panel, "devops")).toEqual([ROCKET_ON_TEAL]);
    const [qa] = marksIn(panel, "qa");
    expect(qa).toMatchObject({ mark: "initials", initials: "QA" });
    // The generic person is the section's own glyph now, and no row's.
    expect(panel.querySelector(".row svg.lucide-user-round")).toBeNull();
  });

  it("on its own view's tab and heading", async () => {
    await opened();
    const panel = await screen.findByTestId("panel-personas");

    await userEvent.click(within(panel).getByRole("button", { name: /devops/ }));
    await screen.findByText("It remembers 0 things.");

    const tab = within(strip()).getByRole("tab", { selected: true });
    expect(tab.querySelector(".tab-name")).toHaveTextContent(/^devops$/);
    expect(marksIn(tab, "devops")).toEqual([ROCKET_ON_TEAL]);
    const heading = screen.getByRole("heading", { level: 2, name: "devops" });
    expect(marksIn(heading, "devops")).toEqual([ROCKET_ON_TEAL]);
  });

  it("on its chat's item in the needs-you list", async () => {
    await opened();

    await userEvent.click(await screen.findByRole("button", { name: "1 chat needs you" }));

    const item = await screen.findByRole("menuitem", { name: /^Go to devops 1/ });
    expect(marksIn(item, "devops")).toEqual([ROCKET_ON_TEAL]);
  });

  it("on a Notice that names it", async () => {
    await opened();

    const notice = await screen.findByRole("status", { name: "Persona's hosts held" });
    expect(notice).toHaveTextContent("This chat was opened by steward 3 as devops.");
    expect(marksIn(notice, "devops")).toEqual([ROCKET_ON_TEAL]);
  });

  it("on its choice in the new-chat picker, where each persona wears its own", async () => {
    await opened();

    await userEvent.click(await screen.findByRole("button", { name: "New tab" }));

    const dialog = await screen.findByRole("dialog", { name: "Start a chat" });
    const personas = within(dialog).getByRole("radiogroup", { name: "Persona" });
    expect(marksIn(personas, "devops")).toEqual([ROCKET_ON_TEAL]);
    expect(marksIn(personas, "qa")).toEqual([
      expect.objectContaining({ mark: "initials", initials: "QA" }),
    ]);
    // The mark is not part of the choice's name.
    expect(within(personas).getByRole("radio", { name: "devops" })).toBeChecked();
  });
});

describe("the persona's view lets the person pick", () => {
  it("writes the pick to the definition, and every surface draws it", async () => {
    const { asked, tab } = await opened();
    const panel = await screen.findByTestId("panel-personas");
    await userEvent.click(within(panel).getByRole("button", { name: /devops/ }));
    const picker = await screen.findByRole("region", { name: "Icon and colour" });
    expect(within(picker).getByRole("radio", { name: "rocket" })).toBeChecked();

    await userEvent.click(within(picker).getByRole("radio", { name: "shield" }));
    await waitFor(() => expect(marksIn(tab, "devops")[0]).toMatchObject({ icon: "shield" }));
    await userEvent.click(within(picker).getByRole("radio", { name: "pink" }));

    const SHIELD_ON_PINK = { mark: "icon", icon: "shield", initials: undefined, colour: "pink" };
    await waitFor(() => expect(marksIn(tab, "devops")).toEqual([SHIELD_ON_PINK]));
    expect(marksIn(panel, "devops")).toEqual([SHIELD_ON_PINK]);
    expect(asked.filter((one) => one.cmd === "persona_mark_set").map((one) => one.args)).toEqual([
      { plane: PLANE, name: "devops", icon: "shield", colour: "teal" },
      { plane: PLANE, name: "devops", icon: "shield", colour: "pink" },
    ]);
  });

  it("says why an oversized image is not drawn, and draws the initials everywhere", async () => {
    const why =
      "icon.png is over the 64 KB limit for an icon, so purlis shows the initials instead.";
    core([mark("devops", { colour: "teal", trouble: [why] }), mark("qa")]);
    render(<App />);
    const tab = await within(await findStripNamed("Tabs")).findByRole("tab", { name: /devops 1/ });
    await waitFor(() =>
      expect(marksIn(tab, "devops")).toEqual([
        { mark: "initials", icon: undefined, initials: "DE", colour: "teal" },
      ]),
    );

    const panel = await screen.findByTestId("panel-personas");
    await userEvent.click(within(panel).getByRole("button", { name: /devops/ }));

    const picker = await screen.findByRole("region", { name: "Icon and colour" });
    expect(await within(picker).findByText(why)).toBeInTheDocument();
  });

  it("draws a custom image in place of the icon, on every surface", async () => {
    const drawImage = vi.fn();
    vi.stubGlobal(
      "createImageBitmap",
      vi.fn(async () => ({ width: 64, height: 64, close: () => undefined })),
    );
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      drawImage,
    } as unknown as CanvasRenderingContext2D);
    core([
      mark("devops", {
        icon: "rocket",
        colour: "teal",
        image: { mime: "image/png", base64: "iVBORw0KGgo=" },
      }),
      mark("qa"),
    ]);
    render(<App />);
    const tab = await within(await findStripNamed("Tabs")).findByRole("tab", { name: /devops 1/ });

    await waitFor(() => expect(marksIn(tab, "devops")[0]).toMatchObject({ mark: "image" }));
    await waitFor(() => expect(drawImage).toHaveBeenCalled());
    expect(tab.querySelector(".persona-mark canvas")).not.toBeNull();
    const panel = await screen.findByTestId("panel-personas");
    expect(marksIn(panel, "devops")[0]).toMatchObject({ mark: "image", icon: undefined });
  });
});
