import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render as renderBare,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import App from "./App";
import type { AskOffer, DispatchPending, VaultRefused } from "./bindings";
import { forgetDismissals } from "./dismissals";

/**
 * **Ask {persona}…, from a chat's tab**, against the whole window: the row on the tab's menu,
 * the dialog it opens, the one command its answer sends (`ask_persona_chat`), and the row's
 * absence where an administrator's policy locks all dispatch.
 *
 * The pane stands in for a chat's own pane and carries one button, which opens the dialog the
 * way a Notice on that pane does (`useAskPersona`).
 */

vi.mock("./SessionPane", async () => {
  const { useAskPersona } = await import("./AskPersona");
  return {
    SessionPane: ({ session }: { session: number }) => {
      const ask = useAskPersona();
      return (
        <div data-testid="pane">
          session {session}
          <button
            onClick={() =>
              ask(session, "devops", {
                name: "use vault prod",
                ask: "Run the check that needs vault prod.",
              })
            }
          >
            A Notice names devops
          </button>
        </div>
      );
    },
  };
});

const render = (ui: React.ReactElement) => renderBare(<StrictMode>{ui}</StrictMode>);

afterEach(() => {
  cleanup();
  clearMocks();
  forgetDismissals();
});

const PLANE = "/home/dev/plane";
const ALPHA = `${PLANE}/workspaces/alpha`;

const CHAT = {
  session: 4,
  name: "4",
  cwd: ALPHA,
  harness: "claude",
  in_front: true,
  resumed: null,
  fresh: null,
  profile: "claude",
  persona: "steward",
  unreported: null,
  guessed: null,
  pinned: false,
  label: null,
  from: null,
};

const OPEN: AskOffer = { personas: ["devops", "steward"], locked: null, locked_for: [] };
const LOCKED =
  "No chat is dispatched to a persona in this project. Locked by policy, set by root in /etc/purlis/policy.toml.";

type Core = {
  offer: AskOffer;
  refused?: string;
  /** The vaults the steward chat was refused. */
  refusals?: VaultRefused[];
  /** What the steward chat asked of other personas, held for the person. */
  dispatches?: DispatchPending[];
};

/** Vault devops, refused to the steward chat: it is tagged for devops. */
const REFUSED: VaultRefused = {
  plane: PLANE,
  session: 4,
  vault: "devops",
  persona: "steward",
  tagged_for: "devops",
  dispatch_to: "devops",
  locked: null,
};

/** The steward chat's own dispatch to devops, held for the person's answer. */
const HELD: DispatchPending = {
  plane: PLANE,
  id: 3,
  session: 4,
  chat: "4",
  asking: "steward",
  target: "devops",
  brief: "# Verify the release on prod\n\nRead-only.",
  brief_cut: false,
  brief_lines: 3,
  levels: ["chat", "you", "project"],
  locked: null,
  never_unread: null,
  works_with: "devops works with its own access: no vault; no hosts beyond the project's.",
  also: [],
  shown: "s0",
};

function core(now: Core) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  mockIPC(
    (cmd, args) => {
      asked.push({ cmd, args: (args ?? {}) as Record<string, unknown> });
      if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
      if (cmd === "opened_chats") return [CHAT];
      if (cmd === "plane_sidebar")
        return {
          root: PLANE,
          personas: ["devops", "steward"],
          persona: "steward",
          unfiled: [],
          workspaces: [
            { name: "alpha", path: ALPHA, vision: "", todos: [], chats: [CHAT] },
            {
              name: "beta",
              path: `${PLANE}/workspaces/beta`,
              vision: "",
              todos: [],
              chats: [],
            },
          ],
        };
      if (cmd === "chat_states") return [];
      if (cmd === "chats_that_would_not_start") return [];
      if (cmd === "running_sessions") return [];
      if (cmd === "ask_persona_offer") return now.offer;
      if (cmd === "vault_refusals") return now.refusals ?? [];
      if (cmd === "dispatch_grants_needed") return now.dispatches ?? [];
      if (cmd === "ask_persona_chat") {
        if (now.refused !== undefined) throw new Error(now.refused);
        return 9;
      }
      return null;
    },
    { shouldMockEvents: true },
  );
  return {
    asked: (cmd: string) => asked.filter((one) => one.cmd === cmd).map(({ args }) => args),
  };
}

async function settled() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function aStewardChat(now: Partial<Core> = {}) {
  const said = core({ offer: OPEN, ...now });
  render(<App />);
  await screen.findByTestId("pane");
  await settled();
  return said;
}

/** The menu on the steward chat's tab, as a right-click opens it. */
async function tabMenu() {
  const strip = screen.getByRole("tablist", { name: "Tabs" });
  fireEvent.contextMenu(within(strip).getAllByRole("tab")[0]);
  return screen.findByRole("menu");
}

describe("Ask a persona from a chat's tab", () => {
  it("offers one row per persona the project has finished, on the tab's menu", async () => {
    await aStewardChat();

    const menu = await tabMenu();

    expect(within(menu).getByRole("menuitem", { name: "Ask devops…" })).toBeEnabled();
    expect(within(menu).getByRole("menuitem", { name: "Ask steward…" })).toBeEnabled();
  });

  it("asks for a task name and what to ask, and starts a devops chat under this one", async () => {
    const { asked } = await aStewardChat();

    await userEvent.click(within(await tabMenu()).getByRole("menuitem", { name: "Ask devops…" }));

    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });
    const send = within(dialog).getByRole("button", { name: "Ask devops" });
    // Nothing to send yet, so nothing is sent.
    expect(send).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Task name"), "check prod");
    expect(send).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("What to ask"), "Is prod healthy?");
    // It says where the report goes and who it is marked as started by.
    expect(dialog).toHaveTextContent(
      "Its report comes back to steward 4, marked as started by you.",
    );
    expect(asked("ask_persona_chat")).toEqual([]);

    await userEvent.click(send);

    await waitFor(() =>
      expect(asked("ask_persona_chat")).toEqual([
        {
          plane: PLANE,
          session: 4,
          persona: "devops",
          name: "check prod",
          ask: "Is prod healthy?",
          // Nothing was picked: this chat's folder.
          place: null,
          columns: 80,
          rows: 24,
        },
      ]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Ask devops" })).toBeNull());

    // The core tells the window of the chat it started, as it does for one a chat started:
    // its tab arrives behind the one being read.
    await act(() =>
      emit("handoff-arrived", {
        plane: PLANE,
        session: 9,
        name: "9",
        label: "check prod",
        from: { name: "steward 4", workspace: "alpha", chat: 4, task: true },
        workspace: "alpha",
        persona: "devops",
        harness: "claude",
      }),
    );
    const strip = screen.getByRole("tablist", { name: "Tabs" });
    const tabs = await waitFor(() => {
      const all = within(strip).getAllByRole("tab");
      expect(all).toHaveLength(2);
      return all;
    });
    expect(tabs[1]).toHaveTextContent("check prod");
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
  });

  it("offers three places to work, and sends the core its one word for the pick", async () => {
    // #1453: this chat's folder, a branch of its own, or another workspace. The window says
    // branch and folder, never worktree (ADR 0072 §4); the core's word for it is `worktree`.
    const { asked } = await aStewardChat();
    await userEvent.click(within(await tabMenu()).getByRole("menuitem", { name: "Ask devops…" }));
    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });
    await userEvent.type(within(dialog).getByLabelText("Task name"), "check prod");
    await userEvent.type(within(dialog).getByLabelText("What to ask"), "Is prod healthy?");

    const places = within(dialog).getByRole("radiogroup", { name: "Where it works" });
    expect(
      within(places)
        .getAllByRole("radio")
        .map((radio) => radio.getAttribute("aria-checked")),
    ).toEqual(["true", "false", "false"]);
    expect(dialog).not.toHaveTextContent(/worktree/i);

    await userEvent.click(within(places).getByRole("radio", { name: "A branch of its own" }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Ask devops" }));

    await waitFor(() =>
      expect(asked("ask_persona_chat")).toEqual([
        {
          plane: PLANE,
          session: 4,
          persona: "devops",
          name: "check prod",
          ask: "Is prod healthy?",
          place: "worktree",
          columns: 80,
          rows: 24,
        },
      ]),
    );
  });

  it("asks which workspace before it sends a chat into another one", async () => {
    const { asked } = await aStewardChat();
    await userEvent.click(within(await tabMenu()).getByRole("menuitem", { name: "Ask devops…" }));
    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });
    await userEvent.type(within(dialog).getByLabelText("Task name"), "check prod");
    await userEvent.type(within(dialog).getByLabelText("What to ask"), "Is prod healthy?");
    const send = within(dialog).getByRole("button", { name: "Ask devops" });

    await userEvent.click(within(dialog).getByRole("radio", { name: "Another workspace" }));

    // The pick has a second half, and nothing is sent without it.
    expect(send).toBeDisabled();
    const which = within(dialog).getByLabelText("Workspace");
    // The workspace this chat works in is not offered: there, this chat's folder is the pick.
    expect(within(which).queryByRole("option", { name: "alpha" })).toBeNull();
    await userEvent.selectOptions(which, "beta");
    expect(send).toBeEnabled();
    await userEvent.click(send);

    await waitFor(() =>
      expect(asked("ask_persona_chat").map((args) => args.place)).toEqual(["workspace:beta"]),
    );
  });

  it("keeps what you typed and says the core's sentence when the chat does not start", async () => {
    const why =
      "this chat already has 6 persona chats running, which is as many as it may have at once. Wait for one to report, then dispatch again.";
    await aStewardChat({ refused: why });
    await userEvent.click(within(await tabMenu()).getByRole("menuitem", { name: "Ask devops…" }));
    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });
    await userEvent.type(within(dialog).getByLabelText("Task name"), "check prod");
    await userEvent.type(within(dialog).getByLabelText("What to ask"), "Is prod healthy?");

    await userEvent.click(within(dialog).getByRole("button", { name: "Ask devops" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(why);
    expect(within(dialog).getByLabelText("Task name")).toHaveValue("check prod");
    expect(within(dialog).getByLabelText("What to ask")).toHaveValue("Is prod healthy?");
  });

  it("starts nothing on Cancel", async () => {
    const { asked } = await aStewardChat();
    await userEvent.click(within(await tabMenu()).getByRole("menuitem", { name: "Ask devops…" }));
    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });

    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(screen.queryByRole("dialog", { name: "Ask devops" })).toBeNull();
    expect(asked("ask_persona_chat")).toEqual([]);
  });

  it("opens prefilled from a Notice on the chat's pane, and sends the same command", async () => {
    const { asked } = await aStewardChat();

    await userEvent.click(screen.getByRole("button", { name: "A Notice names devops" }));

    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });
    expect(within(dialog).getByLabelText("Task name")).toHaveValue("use vault prod");
    expect(within(dialog).getByLabelText("What to ask")).toHaveValue(
      "Run the check that needs vault prod.",
    );
    // The name is given, so the question is where the keyboard starts.
    expect(within(dialog).getByLabelText("What to ask")).toHaveFocus();
    await userEvent.click(within(dialog).getByRole("button", { name: "Ask devops" }));
    await waitFor(() =>
      expect(asked("ask_persona_chat")).toEqual([
        {
          plane: PLANE,
          session: 4,
          persona: "devops",
          name: "use vault prod",
          ask: "Run the check that needs vault prod.",
          place: null,
          columns: 80,
          rows: 24,
        },
      ]),
    );
  });
});

/**
 * **Dispatching is the chat's work, and the pane says so** (#1481). The operator pressed
 * Dispatch to devops… on a refused vault's Notice, met an empty form and asked: *"is
 * dispatching a manual job? why is it not handled by the caller persona?"* The chat had
 * already dispatched; its request, with its Allow buttons, was on the same pane, hidden by a
 * broken layout, under a Notice that offered a second, manual way.
 */
describe("a refused vault, on a pane whose chat has already asked that persona", () => {
  /** The pane's Notices, top to bottom, by their accessible names. */
  const said = () =>
    [...document.querySelectorAll(".pane-notices > .notice-pane-box > [role='status']")].map(
      (one) => one.getAttribute("aria-label"),
    );

  it("draws the request that waits for an answer first, above what purlis only reports", async () => {
    await aStewardChat({ refusals: [REFUSED], dispatches: [HELD] });
    await screen.findByRole("status", { name: "Vault" });

    expect(said()).toEqual(["Dispatch to devops", "Vault"]);
    // The chat at a glance is a row of its own, and the Notices are not items of it.
    const corner = document.querySelector(".pane-corner.at-start");
    expect([...(corner?.children ?? [])].map((one) => one.className)).toEqual([
      "pane-chips",
      "pane-notices",
    ]);
  });

  it("reads the request top to bottom: the sentence, the answers in order, then the brief", async () => {
    await aStewardChat({ refusals: [REFUSED], dispatches: [HELD] });

    const request = await screen.findByRole("status", { name: "Dispatch to devops" });
    expect([...request.children].map((one) => one.textContent)).toEqual([
      expect.stringMatching(/^This chat runs as steward and wants to dispatch to devops\./),
      "Allow for this chat",
      "Allow for me on this machine",
      "Allow for everyone in this project",
      "Keep blocked",
      "Never for this pair",
    ]);
    // The brief is under the answers, in the same box, in a box of its own that scrolls.
    const brief = screen.getByRole("region", { name: "Brief from the chat" });
    expect(request.nextElementSibling).toContainElement(brief);
    expect(request.nextElementSibling).toHaveClass("notice-under-pane");
    expect(brief.querySelector("pre")).toHaveClass("block-report-draft", "block-report-brief");
    expect(brief.querySelector("pre")).toHaveTextContent("# Verify the release on prod");
  });

  it("points the vault's Notice at that request, and offers no manual dispatch beside it", async () => {
    const { asked } = await aStewardChat({ refusals: [REFUSED], dispatches: [HELD] });

    const vault = await screen.findByRole("status", { name: "Vault" });
    await within(vault).findByRole("button", { name: "Show the request" });
    expect(vault).toHaveTextContent("This chat has already asked devops: answer that above.");
    expect(within(vault).queryByRole("button", { name: /^Dispatch to/ })).toBeNull();

    await userEvent.click(within(vault).getByRole("button", { name: "Show the request" }));

    // On the request's line, and no dialog: nothing is asked a second time, and nothing starts.
    expect(screen.getByRole("status", { name: "Dispatch to devops" })).toHaveFocus();
    expect(screen.queryByRole("dialog", { name: "Ask devops" })).toBeNull();
    expect(asked("allow_dispatch")).toEqual([]);
    expect(asked("ask_persona_chat")).toEqual([]);
  });
});

describe("a refused vault, on a pane whose chat has not asked", () => {
  it("opens Ask devops empty, and the dialog says why it is empty", async () => {
    await aStewardChat({ refusals: [REFUSED] });

    const vault = await screen.findByRole("status", { name: "Vault" });
    await userEvent.click(within(vault).getByRole("button", { name: "Dispatch to devops…" }));

    // Nothing a chat produced is typed for the person (the vault's name, the refused command).
    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });
    expect(within(dialog).getByLabelText("Task name")).toHaveValue("");
    expect(within(dialog).getByLabelText("What to ask")).toHaveValue("");
    expect(dialog).toHaveTextContent(
      "Write the request yourself: the chat's own words are not copied here.",
    );
    expect(dialog).toHaveClass("ask-persona");
  });

  it("does not say so when you open it yourself, from the tab's menu", async () => {
    await aStewardChat();

    await userEvent.click(within(await tabMenu()).getByRole("menuitem", { name: /Ask devops/ }));

    const dialog = await screen.findByRole("dialog", { name: "Ask devops" });
    expect(dialog).not.toHaveTextContent("Write the request yourself");
  });
});

describe("where policy locks all dispatch", () => {
  it("has no Ask row on the tab's menu, and the palette says why", async () => {
    await aStewardChat({ offer: { personas: [], locked: LOCKED, locked_for: [] } });

    const menu = await tabMenu();
    expect(within(menu).queryByRole("menuitem", { name: /^Ask / })).toBeNull();
    await userEvent.keyboard("{Escape}");

    await userEvent.keyboard("{F2}");
    await userEvent.type(screen.getByRole("combobox"), "Ask a persona");
    const row = await screen.findByRole("option", { name: /Ask a persona…/ });
    expect(row).toHaveTextContent(LOCKED);
    expect(row).toHaveAttribute("aria-disabled", "true");
  });

  it("takes a locked pair off this chat's tab, and the palette's row says who locked it", async () => {
    const why =
      "Policy forbids steward chats dispatching to devops. Locked by policy, set by the platform team in /etc/purlis/policy.json.";
    await aStewardChat({
      offer: { ...OPEN, locked_for: [{ session: 4, persona: "devops", why }] },
    });

    const menu = await tabMenu();
    expect(within(menu).queryByRole("menuitem", { name: "Ask devops…" })).toBeNull();
    expect(within(menu).getByRole("menuitem", { name: "Ask steward…" })).toBeEnabled();
    await userEvent.keyboard("{Escape}");

    await userEvent.keyboard("{F2}");
    await userEvent.type(screen.getByRole("combobox"), "Ask devops");
    const row = await screen.findByRole("option", { name: /Ask devops…/ });
    expect(row).toHaveTextContent(why);
    expect(row).toHaveAttribute("aria-disabled", "true");
  });

  it("offers nothing until the core has said who can be asked", async () => {
    // A core that answers nothing for the offer: an older one.
    mockIPC(
      (cmd) => {
        if (cmd === "plane_at_launch") return { plane: PLANE, from: PLANE, why: null };
        if (cmd === "opened_chats") return [CHAT];
        if (cmd === "plane_sidebar")
          return {
            root: PLANE,
            personas: ["devops"],
            persona: null,
            unfiled: [],
            workspaces: [{ name: "alpha", path: ALPHA, vision: "", todos: [], chats: [CHAT] }],
          };
        if (["chat_states", "chats_that_would_not_start", "running_sessions"].includes(cmd))
          return [];
        return null;
      },
      { shouldMockEvents: true },
    );
    render(<App />);
    await screen.findByTestId("pane");
    await settled();

    expect(within(await tabMenu()).queryByRole("menuitem", { name: /^Ask / })).toBeNull();
  });
});
