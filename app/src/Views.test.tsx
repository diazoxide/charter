import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ViewPane } from "./Views";
import { catalogue, catalogued, type Offer } from "./actions";
import { noTabs } from "./tabs";
import { searchFromFocus, searchView } from "./contentSearch";
import type {
  ActionAnswer,
  ExtensionView,
  LandQuestion,
  PanelBlock,
  RowAction,
  ViewAnswer,
} from "./bindings";
import type { ViewRef } from "./tabs";

/**
 * A view in a pane (`Views.tsx`): charter's persona view and an extension's statistics, drawn
 * by the one piece of code, asked through the one command.
 *
 * What the tab around it does — opening, deduplicating, coming back at a launch — is
 * `tabs.test.ts` for the model and `ViewTabs.test.tsx` for the window.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const STEWARD: ViewRef = { from: null, view: "persona", key: "steward" };

const STATISTICS: ExtensionView = {
  extension: "persona-statistics",
  id: "statistics",
  title: "Statistics",
  about: "personas",
};

const THE_PLANE_S_STATISTICS: ViewRef = {
  from: "persona-statistics",
  view: "statistics",
  key: "",
};

/** What `panels::persona_view` answers for a persona with two memories. */
const PERSONA: ViewAnswer = {
  kind: "answered",
  blocks: [
    { kind: "note", text: "The steward", tone: "default" },
    {
      kind: "facts",
      facts: [
        { label: "Dispatch to it for", value: "routing" },
        { label: "Vault", value: "none; this persona holds no credentials of its own" },
      ],
    },
    { kind: "note", text: "It remembers 2 things.", tone: "plain" },
    {
      kind: "list",
      rows: [
        {
          key: "a",
          text: "Charter defects go upstream",
          note: "2026-09-20",
          mark: "note",
          tone: "plain",
          detail: { kind: "text", text: "File the issue." },
          runs: null,
          actions: [],
        },
        {
          key: "b",
          text: "Grill back hard",
          note: "2026-09-21",
          mark: "note",
          tone: "plain",
          detail: { kind: "text", text: "Recommend, don't offer menus." },
          runs: null,
          actions: [],
        },
      ],
      empty: { headline: "Nothing remembered yet", body: null, offer: null },
    },
  ],
  took_ms: 2,
  overreach: null,
};

/** What persona statistics' program answers: a sentence and a chart. */
const CHARTED: ViewAnswer = {
  kind: "answered",
  blocks: [
    { kind: "note", text: "4 memories across 2 personas", tone: "plain" },
    {
      kind: "chart",
      title: "Memories per persona",
      shape: "bars",
      unit: "memories",
      points: [
        { label: "steward", value: 3, note: "default · 75%" },
        { label: "devops", value: 1, note: "25%" },
      ],
    },
  ],
  took_ms: 6,
  overreach: null,
};

/** The core, answering `open_view` and recording what it was asked. */
function core(answer: (asked: Record<string, unknown>) => ViewAnswer | Error) {
  const opened: Record<string, unknown>[] = [];
  mockIPC((cmd, args) => {
    if (cmd !== "open_view") return undefined;
    const given = (args ?? {}) as Record<string, unknown>;
    opened.push(given);
    const said = answer(given);
    if (said instanceof Error) throw said;
    return said;
  });
  return { opened };
}

function draw(
  view: ViewRef,
  on: {
    title?: string;
    waits?: boolean;
    offered?: ExtensionView[];
    onOpenView?: (view: ViewRef, title: string) => void;
    onAsk?: () => void;
    strict?: boolean;
    workspace?: string;
    offerFor?: (id: string) => Offer | undefined;
    onPress?: (offer: Offer) => void;
  } = {},
) {
  const pane = (
    <ViewPane
      plane={PLANE}
      view={view}
      title={on.title ?? "steward"}
      workspace={on.workspace}
      waits={on.waits ?? false}
      offered={on.offered ?? []}
      onOpenView={on.onOpenView ?? (() => {})}
      onAsk={on.onAsk ?? (() => {})}
      onVaultChanged={() => {}}
      offerFor={on.offerFor}
      onPress={on.onPress}
    />
  );
  render(on.strict ? <StrictMode>{pane}</StrictMode> : pane);
}

describe("the persona view", () => {
  it("draws its definition as a description list, each value under its label", async () => {
    core(() => PERSONA);
    draw(STEWARD);

    const facts = await screen.findByTestId("facts");
    expect(facts.tagName).toBe("DL");
    const labels = within(facts)
      .getAllByRole("term")
      .map((it) => it.textContent);
    const values = within(facts)
      .getAllByRole("definition")
      .map((it) => it.textContent);
    expect(labels).toEqual(["Dispatch to it for", "Vault"]);
    expect(values).toEqual(["routing", "none; this persona holds no credentials of its own"]);
  });

  it("is asked of the core as charter's own view, through the command every view is asked through", async () => {
    const { opened } = core(() => PERSONA);
    draw(STEWARD);

    await screen.findByText("routing");

    expect(opened).toEqual([
      { plane: PLANE, from: null, view: "persona", key: "steward", workspace: null },
    ]);
  });

  it("draws the definition, how much it remembers, and the memories as the list primitive", async () => {
    core(() => PERSONA);
    draw(STEWARD);

    expect(await screen.findByText("It remembers 2 things.")).toBeInTheDocument();
    const memories = screen.getByRole("list", { name: "steward" });
    expect(within(memories).getAllByRole("listitem")).toHaveLength(2);
    // A memory's row opens its whole body, as every row of the list primitive does.
    await userEvent.click(within(memories).getByRole("button", { name: /Charter defects/ }));
    expect(await screen.findByText("File the issue.")).toBeInTheDocument();
  });

  it("asks no extension anything when it is opened, because reading a persona is not consent", async () => {
    // The adversarial review of #212 (finding 6): the card mounted every offered view and so
    // started the extension's program because the operator read some memories.
    const { opened } = core(() => PERSONA);
    draw(STEWARD, { offered: [STATISTICS] });

    await screen.findByText("It remembers 2 things.");

    expect(opened.filter((asked) => asked.from !== null)).toEqual([]);
  });

  it("offers Statistics beside itself when an extension offers a view about personas", async () => {
    core(() => PERSONA);
    const wanted: [ViewRef, string][] = [];
    draw(STEWARD, {
      offered: [STATISTICS],
      onOpenView: (view, title) => wanted.push([view, title]),
    });

    await userEvent.click(screen.getByRole("button", { name: "Statistics" }));

    // About this persona, in a tab of its own, and asked there — not here.
    expect(wanted).toEqual([
      [{ from: "persona-statistics", view: "statistics", key: "steward" }, "Statistics · steward"],
    ]);
  });

  it("offers no Statistics when no extension offers a view about personas", async () => {
    core(() => PERSONA);
    draw(STEWARD);

    await screen.findByText("It remembers 2 things.");

    expect(screen.queryByRole("button", { name: "Statistics" })).toBeNull();
  });

  it("edits the persona's own dispatch limits, which the project's file keeps", async () => {
    const saves: unknown[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "open_view") return PERSONA;
      if (cmd === "dispatch_limits")
        return {
          file: "purlis.toml",
          local_file: "purlis.local.toml",
          base: "schema = 1\n",
          local_base: null,
          limits: [
            {
              word: "depth",
              label: "Depth",
              help: "How many dispatches deep a chain may go.",
              default: 3,
              persona_only: false,
              most: 8,
              ceiling: null,
            },
            {
              word: "may-run-at-once",
              label: "May run at once",
              help: "The chats that may run as this persona at once.",
              default: null,
              persona_only: true,
              most: 10000,
              ceiling: null,
            },
          ],
          rows: [
            { scope: "project", name: "", values: [null, null], beneath: [3, null], ignored: [] },
          ],
          mine: [],
          locked_by: null,
          refused: [],
          local_left_out: null,
        };
      if (cmd === "save_project_settings") {
        saves.push(args);
        return { kind: "saved", file: {} };
      }
      return undefined;
    });
    draw(STEWARD);

    const limits = await screen.findByRole("region", { name: "Dispatch limits" });
    const most = await within(limits).findByLabelText("May run at once for this persona");
    await userEvent.type(most, "1{Enter}");

    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0]).toMatchObject({
      which: "shared",
      change: {
        kind: "edits",
        edits: [
          {
            path: [
              { key: "dispatch" },
              { key: "personas" },
              { key: "steward" },
              { key: "may-run-at-once" },
            ],
            value: { kind: "integer", value: 1 },
          },
        ],
      },
    });
  });

  it("draws no dispatch limits on a view that is not a persona's", async () => {
    core(() => CHARTED);
    draw(THE_PLANE_S_STATISTICS, { title: "Statistics" });

    await screen.findByText("4 memories across 2 personas");
    expect(screen.queryByRole("region", { name: "Dispatch limits" })).toBeNull();
  });

  it("says a persona the plane no longer has is gone, in the middle of the tab and not as an error", async () => {
    core(() => ({ kind: "gone", why: "This plane has no persona called steward any more." }));
    draw(STEWARD);

    const gone = await screen.findByTestId("view-gone");

    expect(gone).toHaveTextContent("steward is not here any more");
    expect(gone).toHaveTextContent("This plane has no persona called steward any more.");
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

/**
 * **Search is a side view only** (#1701, D-1701-2): nothing opens a Search view tab since the
 * left side's Search view took ⌘⇧F (#1676), so a pane draws none. A search's view reference is
 * the side view's own state (`PlaneView`'s `sideSearch`), never a tab's.
 */
describe("a search's view", () => {
  it("is not drawn as a tab of its own", async () => {
    const { opened } = core(() => ({ kind: "gone", why: "There is no view called search." }));
    draw(searchView(searchFromFocus(undefined, "alpha")), { title: "Search" });

    await screen.findByTestId("view-gone");
    expect(screen.queryByRole("searchbox")).toBeNull();
    expect(opened).toHaveLength(1);
  });
});

describe("an extension's view", () => {
  it("is asked of its program and drawn as a chart, a list to a screen reader", async () => {
    const { opened } = core(() => CHARTED);
    draw(THE_PLANE_S_STATISTICS, {
      title: "Statistics",
      offered: [STATISTICS],
      workspace: "alpha",
    });

    const chart = await screen.findByTestId("chart");

    // With the workspace whose strip it is on: its settings are a layer of the gate at the
    // press (charter-app#280).
    expect(opened).toEqual([
      {
        plane: PLANE,
        from: "persona-statistics",
        view: "statistics",
        key: "",
        workspace: "alpha",
      },
    ]);
    expect(within(chart).getByRole("list", { name: "Memories per persona" })).toBeInTheDocument();
    expect(within(chart).getAllByRole("listitem")[0]).toHaveTextContent("steward3 · default · 75%");
    // Whose program answered, after approval and not only at it (ADR 0041 item 5).
    expect(screen.getByRole("heading", { name: /Statistics/ })).toHaveTextContent(
      "persona-statistics",
    );
  });

  it("scales each bar to the largest, and hides the bar itself from a screen reader", async () => {
    core(() => CHARTED);
    draw(THE_PLANE_S_STATISTICS, { title: "Statistics" });

    const chart = await screen.findByTestId("chart");
    const bars = chart.querySelectorAll<HTMLElement>(".chart-bar");

    expect([...bars].map((bar) => bar.style.inlineSize)).toEqual(["100%", "33%"]);
    expect(bars[0].closest("[aria-hidden='true']")).not.toBeNull();
  });

  it("draws the executor's refusal in its own words rather than an empty chart", async () => {
    core(
      () =>
        new Error("'persona-statistics' has changed since you approved it — purlis will ask again"),
    );
    draw(THE_PLANE_S_STATISTICS, { title: "Statistics" });

    expect(await screen.findByRole("alert")).toHaveTextContent("has changed since you approved it");
    expect(screen.queryByTestId("chart")).toBeNull();
  });

  it("says an uninstalled extension's view is gone rather than refusing", async () => {
    core(() => ({ kind: "gone", why: "The extension 'persona-statistics' is not installed." }));
    draw(THE_PLANE_S_STATISTICS, { title: "Statistics" });

    expect(await screen.findByTestId("view-gone")).toHaveTextContent("is not installed");
  });

  it("is asked once when React mounts it twice, so it never meets its own question in flight", async () => {
    // The adversarial review of #212 (finding 7): development's double effect was a second
    // question to a program still answering the first, refused as "still answering".
    const { opened } = core(() => CHARTED);
    draw(THE_PLANE_S_STATISTICS, { title: "Statistics", strict: true });

    await screen.findByTestId("chart");

    expect(opened).toHaveLength(1);
  });

  describe("put back by a launch", () => {
    it("asks its program nothing until the operator presses for it", async () => {
      const { opened } = core(() => CHARTED);
      const onAsk = vi.fn();
      draw(THE_PLANE_S_STATISTICS, { title: "Statistics", waits: true, onAsk });

      const waiting = screen.getByTestId("view-waits");
      await userEvent.click(
        within(waiting).getByRole("button", { name: "Ask persona-statistics" }),
      );

      expect(opened).toEqual([]);
      expect(onAsk).toHaveBeenCalledTimes(1);
    });

    it("does not make purlis's own view wait, since it runs no program", async () => {
      const { opened } = core(() => PERSONA);
      draw(STEWARD, { waits: true });

      await waitFor(() => expect(opened).toHaveLength(1));
      expect(screen.queryByTestId("view-waits")).toBeNull();
    });
  });

  it("says what changed outside the paths it declares, above its answer, and still draws it", async () => {
    core(() => ({
      ...CHARTED,
      overreach: "While 'persona-statistics' was answering, purlis saw these change: a.md.",
    }));
    draw(THE_PLANE_S_STATISTICS, { title: "Statistics" });

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "While 'persona-statistics' was answering",
    );
    expect(screen.getByTestId("chart")).toBeInTheDocument();
  });
});

describe("an action on an extension's row (charter-app#341)", () => {
  const JOT: RowAction = { id: "jot", title: "Jot a note", asks_first: false, deletes: false };
  const CAREFUL: RowAction = { id: "careful", title: "Careful", asks_first: true, deletes: false };
  const FORGET: RowAction = { id: "forget", title: "Forget", asks_first: true, deletes: true };

  /** The probe's view: one row counting its notes, offering `actions`. */
  const counted = (notes: number, actions: RowAction[]): PanelBlock[] => [
    {
      kind: "list",
      rows: [
        {
          key: "notes",
          text: `${notes} notes`,
          note: null,
          mark: "note",
          tone: "plain",
          detail: null,
          runs: null,
          actions,
        },
      ],
      empty: { headline: "Nothing", body: null, offer: null },
    },
  ];

  const PROBE: ViewRef = { from: "extension-probe", view: "probe", key: "" };

  /** The core, answering the view with `actions` on its row and each action with `acted`. */
  function acting(actions: RowAction[], acted: (asked: Record<string, unknown>) => ActionAnswer) {
    const ran: Record<string, unknown>[] = [];
    mockIPC((cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      if (cmd === "open_view") {
        return { kind: "answered", blocks: counted(0, actions), took_ms: 1, overreach: null };
      }
      if (cmd === "run_action") {
        ran.push(given);
        return acted(given);
      }
      return undefined;
    });
    return { ran };
  }

  it("runs on the row it was pressed on, and the view draws what it answered", async () => {
    const { ran } = acting([JOT], () => ({
      blocks: counted(1, [JOT]),
      took_ms: 3,
      overreach: null,
    }));
    draw(PROBE, { title: "Probe", workspace: "alpha" });

    await userEvent.click(await screen.findByRole("button", { name: "Jot a note" }));

    expect(await screen.findByText("1 notes")).toBeInTheDocument();
    expect(ran).toEqual([
      {
        plane: PLANE,
        extension: "extension-probe",
        action: "jot",
        view: "probe",
        key: "",
        row: "notes",
        workspace: "alpha",
        confirmed: false,
      },
    ]);
  });

  it("asks first when the action says so, and runs only once said yes to", async () => {
    const { ran } = acting([CAREFUL], () => ({ blocks: null, took_ms: 1, overreach: null }));
    draw(PROBE, { title: "Probe" });

    await userEvent.click(await screen.findByRole("button", { name: "Careful" }));

    const asking = await screen.findByRole("alertdialog");
    expect(asking).toHaveTextContent("Run “Careful” from extension-probe?");
    expect(ran).toEqual([]);
    await userEvent.click(within(asking).getByRole("button", { name: "Run" }));
    await waitFor(() => expect(ran).toHaveLength(1));
    expect(ran[0].confirmed).toBe(true);
  });

  it("asks before an action that deletes, in words that say it deletes, and Cancel runs nothing", async () => {
    const { ran } = acting([FORGET], () => ({ blocks: null, took_ms: 1, overreach: null }));
    draw(PROBE, { title: "Probe" });

    await userEvent.click(await screen.findByRole("button", { name: "Forget" }));

    const asking = await screen.findByRole("alertdialog");
    expect(asking).toHaveTextContent("It deletes");
    expect(within(asking).getByRole("button", { name: "Delete" })).toBeInTheDocument();
    await userEvent.click(within(asking).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(ran).toEqual([]);
  });

  it("says only what the last action came to, not what the view's own question saw before it", async () => {
    mockIPC((cmd) => {
      if (cmd === "open_view")
        return {
          kind: "answered",
          blocks: counted(0, [JOT]),
          took_ms: 1,
          overreach: "While 'extension-probe' was answering, purlis saw these change: old.md.",
        };
      if (cmd === "run_action") return { blocks: counted(1, [JOT]), took_ms: 1, overreach: null };
      return undefined;
    });
    draw(PROBE, { title: "Probe" });
    expect(await screen.findByRole("alert")).toHaveTextContent("old.md");

    await userEvent.click(screen.getByRole("button", { name: "Jot a note" }));

    await screen.findByText("1 notes");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("says what the action changed outside the extension's declared paths", async () => {
    acting([JOT], () => ({
      blocks: null,
      took_ms: 1,
      overreach: "While 'extension-probe' was answering, purlis saw these change: stray.txt.",
    }));
    draw(PROBE, { title: "Probe" });

    await userEvent.click(await screen.findByRole("button", { name: "Jot a note" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("stray.txt");
    // Done with no blocks: the view stands as it was.
    expect(screen.getByText("0 notes")).toBeInTheDocument();
  });
});

describe("a workspace's changes", () => {
  const CHANGES: ViewRef = { from: null, view: "changes", key: "alpha" };

  /** What `change::view::blocks` answers, read at `read`. */
  function changes(read: string): ViewAnswer {
    return {
      kind: "answered",
      blocks: [
        {
          kind: "facts",
          facts: [
            { label: "workspace", value: "alpha" },
            { label: "read from the forge", value: read },
          ],
        },
        { kind: "note", text: "api-2 · 0 of 1 merged · bump the API", tone: "default" },
        {
          kind: "list",
          rows: [
            {
              key: "svc",
              text: "svc · change/api-2",
              note: "#7 open · head 9f3a1c2 · checks NOT RUN",
              mark: "repo",
              tone: "trouble",
              detail: null,
              runs: null,
              actions: [],
            },
          ],
          empty: { headline: "No members yet", body: null, offer: null },
        },
      ],
      took_ms: 40,
      overreach: null,
    } as ViewAnswer;
  }

  it("draws each member with its request and checks, and when they were read", async () => {
    core(() => changes("2026-09-26 10:00:00 UTC"));
    draw(CHANGES, { title: "Changes · alpha", workspace: "alpha" });

    expect(await screen.findByText("2026-09-26 10:00:00 UTC")).toBeInTheDocument();
    expect(screen.getByText("· #7 open · head 9f3a1c2 · checks NOT RUN")).toBeInTheDocument();
  });

  it("is asked once, and not again when its tab is drawn again after a workspace switch", async () => {
    const gamma: ViewRef = { ...CHANGES, key: "gamma" };
    const { opened } = core(() => changes("2026-09-26 10:00:00 UTC"));
    draw(gamma, { title: "Changes · gamma", workspace: "gamma" });
    await screen.findByText("2026-09-26 10:00:00 UTC");
    // Switching to another workspace takes this tab's pane away; switching back draws it again.
    cleanup();
    draw(gamma, { title: "Changes · gamma", workspace: "gamma" });
    await screen.findByText("2026-09-26 10:00:00 UTC");

    expect(opened).toHaveLength(1);
  });

  it("asks again only when Refresh is pressed", async () => {
    let read = "2026-09-26 10:00:00 UTC";
    const { opened } = core(() => changes(read));
    draw({ ...CHANGES, key: "beta" }, { title: "Changes · beta", workspace: "beta" });
    await screen.findByText(read);

    read = "2026-09-26 10:05:00 UTC";
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));

    expect(await screen.findByText(read)).toBeInTheDocument();
    expect(opened).toHaveLength(2);
  });
});

describe("a persona's tab heading (SI-3)", () => {
  const offers = catalogued(
    catalogue({
      tabs: noTabs(),
      workspaces: [],
      plane: PLANE,
      personas: ["steward"],
      needsYou: [],
      nameOf: String,
    }),
  );

  it("offers the persona's persona.md to the editor, and its deletion, as the catalogue's rows", async () => {
    core(() => PERSONA);
    const pressed: Offer[] = [];
    draw(STEWARD, { offerFor: (id) => offers.get(id), onPress: (offer) => pressed.push(offer) });

    const head = screen.getByRole("heading", { name: /steward/ }).closest("header");
    if (head === null) throw new Error("the view has no heading");
    await userEvent.click(within(head).getByRole("button", { name: "Edit steward's persona.md" }));
    // #1445: the profile its chats start on is set from the view that shows it.
    await userEvent.click(within(head).getByRole("button", { name: "Set steward's profile…" }));
    await userEvent.click(within(head).getByRole("button", { name: "Delete persona steward…" }));

    expect(pressed.map((offer) => offer.does)).toEqual([
      { verb: "editPersona", persona: "steward" },
      { verb: "setPersonaProfile", persona: "steward" },
      { verb: "removePersona", persona: "steward" },
    ]);
  });

  it("draws neither when the window hands it no catalogue", () => {
    core(() => PERSONA);
    draw(STEWARD);

    expect(screen.queryByRole("button", { name: /persona\.md/ })).toBeNull();
  });
});

describe("the + on a memory list's heading (SI-9c, ADR 0065 Q9)", () => {
  const offers = catalogued(
    catalogue({
      tabs: noTabs(),
      workspaces: [],
      plane: PLANE,
      personas: ["steward"],
      needsYou: [],
      nameOf: String,
    }),
  );
  const heading = (name: RegExp) => {
    const head = screen.getByRole("heading", { name }).closest("header");
    if (head === null) throw new Error("the view has no heading");
    return head;
  };

  it("makes a memory for the persona whose tab it is", async () => {
    core(() => PERSONA);
    const pressed: Offer[] = [];
    draw(STEWARD, { offerFor: (id) => offers.get(id), onPress: (offer) => pressed.push(offer) });

    await userEvent.click(
      within(heading(/steward/)).getByRole("button", { name: "New memory for steward…" }),
    );

    expect(pressed.map((offer) => offer.does)).toEqual([
      { verb: "newMemory", scope: { kind: "persona", name: "steward" } },
    ]);
  });

  it("opens the persona's archive, and the shared store's, beside the + (KN-4)", async () => {
    core(() => PERSONA);
    const pressed: Offer[] = [];
    draw(STEWARD, { offerFor: (id) => offers.get(id), onPress: (offer) => pressed.push(offer) });
    await userEvent.click(
      within(heading(/steward/)).getByRole("button", { name: "Open steward's archive" }),
    );
    cleanup();
    draw(
      { from: null, view: "shared-memory", key: "" },
      {
        title: "Shared memory",
        offerFor: (id) => offers.get(id),
        onPress: (offer) => pressed.push(offer),
      },
    );
    await userEvent.click(
      within(heading(/Shared memory/)).getByRole("button", { name: "Open the shared archive" }),
    );

    expect(pressed.map((offer) => offer.does)).toEqual([
      {
        verb: "openView",
        view: { from: null, view: "memory-archive", key: "persona/steward" },
        title: "Archived memory · steward",
      },
      {
        verb: "openView",
        view: { from: null, view: "memory-archive", key: "shared" },
        title: "Archived shared memory",
      },
    ]);
  });

  it("makes a shared memory from the shared list's tab, which lists the shared store", async () => {
    const { opened } = core(() => ({
      kind: "answered",
      blocks: [
        {
          kind: "list",
          rows: [
            {
              key: "the-plane-is-the-unit-of-work",
              text: "The plane is the unit of work",
              note: "2026-09-20 10:00",
              mark: "note",
              tone: "plain",
              detail: { kind: "text", text: "Yes." },
              runs: "memory.open:shared/the-plane-is-the-unit-of-work",
              actions: [],
            },
          ],
          empty: { headline: "Nothing shared yet", body: null, offer: null },
        },
      ],
      took_ms: 1,
      overreach: null,
    }));
    const pressed: Offer[] = [];
    draw(
      { from: null, view: "shared-memory", key: "" },
      {
        title: "Shared memory",
        offerFor: (id) => offers.get(id),
        onPress: (offer) => pressed.push(offer),
      },
    );

    // Asked of the core as charter's own view.
    await waitFor(() => expect(opened).toHaveLength(1));
    expect(opened[0]).toMatchObject({ from: null, view: "shared-memory", key: "" });
    await userEvent.click(
      await screen.findByRole("button", { name: /The plane is the unit of work/ }),
    );
    await userEvent.click(
      within(heading(/Shared memory/)).getByRole("button", { name: "New shared memory…" }),
    );

    expect(pressed.map((offer) => offer.does)).toEqual([
      {
        verb: "openMemory",
        ref: { scope: { kind: "shared" }, slug: "the-plane-is-the-unit-of-work" },
        title: "The plane is the unit of work",
        keep: false,
      },
      { verb: "newMemory", scope: { kind: "shared" } },
    ]);
  });
});

describe("a change's Push and Land (#474)", () => {
  const HEAD = "6dcb09b5b57875f334f61aebed695e2e4193db5e";

  /** A changes view of api-2 over widget and gadget, gadget needing widget. */
  function changes(): ViewAnswer {
    const member = (repo: string, note: string) => ({
      key: `api-2/${repo}`,
      text: `${repo} · change/api-2`,
      note,
      mark: "repo",
      tone: "plain",
      detail: null,
      runs: null,
      actions: [],
    });
    return {
      kind: "answered",
      blocks: [
        { kind: "note", text: "api-2 · 0 of 2 merged · bump the api", tone: "default" },
        {
          kind: "list",
          rows: [
            member("widget", "#7 open · head 6dcb09b · checks PASSED"),
            member("gadget", "#8 open · head 6dcb09b · checks PASSED"),
          ],
          empty: { headline: "No members yet", body: null, offer: null },
        },
      ],
      took_ms: 40,
      overreach: null,
      changes: [
        {
          at: 1,
          change: "api-2",
          members: [
            { key: "api-2/widget", repo: "widget" },
            { key: "api-2/gadget", repo: "gadget" },
          ],
        },
      ],
    } as ViewAnswer;
  }

  const QUESTION: LandQuestion = {
    repo: "widget",
    number: 7,
    url: "https://github.com/acme/widget/pull/7",
    head: HEAD,
    head_short: "6dcb09b5b578",
    through: "merge",
    forge: "github",
    request: "pull request",
    sigil: "#",
    queue: "merge queue",
    how: "charter merges it now, at 6dcb09b5b578 and no other.",
    squash: true,
    said: ["• widget: checks PASSED at 6dcb09b5b578 (1 check)"],
  };

  /** The core: the view, and `answers` for every other command, each call noted. */
  function forge(answers: Record<string, (args: Record<string, unknown>) => unknown>) {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    mockIPC((cmd, args) => {
      const given = (args ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args: given });
      if (cmd === "open_view") return changes();
      const answer = answers[cmd];
      if (answer === undefined) return undefined;
      const said = answer(given);
      // A command's refusal reaches the window as its string, as Tauri hands it over.
      if (said instanceof Error) throw said.message;
      return said;
    });
    return { calls, called: (cmd: string) => calls.filter((c) => c.cmd === cmd) };
  }

  function open(ws: string) {
    draw({ from: null, view: "changes", key: ws }, { title: `Changes · ${ws}`, workspace: ws });
  }

  it("shows a blocked member's refusal in the core's words beside its row, and merges nothing", async () => {
    const { called } = forge({
      change_land_question: () =>
        new Error("gadget: blocker widget has not landed (its request is open)."),
    });
    open("land-blocked");
    const gadget = (await screen.findByText("gadget · change/api-2")).closest("li");
    if (gadget === null) throw new Error("no gadget row");

    await userEvent.click(within(gadget).getByRole("button", { name: "Land…" }));

    expect(
      await screen.findByText("gadget: blocker widget has not landed (its request is open)."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(called("change_land_question")[0].args).toMatchObject({
      workspace: "land-blocked",
      change: "api-2",
      repo: "gadget",
    });
    expect(called("change_land")).toHaveLength(0);
  });

  it("asks first, naming the request, the head its checks passed at and that it merges now", async () => {
    const { called } = forge({
      change_land_question: () => QUESTION,
      change_land: () => ["✓ merged #7 as e5bd3914e2e5, trailer Charter-Change: api-2"],
    });
    open("land-asks");
    const widget = (await screen.findByText("widget · change/api-2")).closest("li");
    if (widget === null) throw new Error("no widget row");
    await userEvent.click(within(widget).getByRole("button", { name: "Land…" }));

    const asking = await screen.findByRole("alertdialog");
    expect(within(asking).getByText("Land widget's pull request #7?")).toBeInTheDocument();
    expect(within(asking).getByText(HEAD)).toBeInTheDocument();
    expect(within(asking).getByText("merged now, at 6dcb09b5b578")).toBeInTheDocument();
    expect(called("change_land")).toHaveLength(0);
    // Cancel has the focus: an Enter out of habit lands nothing.
    expect(within(asking).getByRole("button", { name: "Cancel" })).toHaveFocus();

    await userEvent.click(within(asking).getByRole("button", { name: "Land" }));

    expect(
      await within(asking).findByText("✓ merged #7 as e5bd3914e2e5, trailer Charter-Change: api-2"),
    ).toBeInTheDocument();
    expect(called("change_land")).toHaveLength(1);
    expect(called("change_land")[0].args).toEqual({
      plane: PLANE,
      workspace: "land-asks",
      change: "api-2",
      confirmed: QUESTION,
      squash: false,
    });
    // Closed after it ran, the view is read again.
    await userEvent.click(within(asking).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(called("open_view")).toHaveLength(2));
  });

  it("says a branch with a merge queue goes into it, and offers no squash it cannot do", async () => {
    const how =
      "purlis puts it in its merge queue at 6dcb09b5b578, and GitHub merges it once the merge queue's own checks pass.";
    forge({
      change_land_question: () => ({ ...QUESTION, through: "queue", how, squash: false }),
    });
    open("land-queue");
    const widget = (await screen.findByText("widget · change/api-2")).closest("li");
    if (widget === null) throw new Error("no widget row");
    await userEvent.click(within(widget).getByRole("button", { name: "Land…" }));

    const asking = await screen.findByRole("alertdialog");
    expect(within(asking).getByText("into the merge queue")).toBeInTheDocument();
    expect(within(asking).getByText(how)).toBeInTheDocument();
    expect(within(asking).queryByRole("checkbox")).toBeNull();
  });

  it("asks again after a landing is refused, and hands back only what it shows now", async () => {
    const MOVED = "0123456789abcdef0123456789abcdef01234567";
    const moved = { ...QUESTION, head: MOVED, head_short: "0123456789ab" };
    let asked = 0;
    const { called } = forge({
      change_land_question: () => (asked++ === 0 ? QUESTION : moved),
      change_land: (args) =>
        (args.confirmed as LandQuestion).head === HEAD
          ? new Error("widget: this is not the landing you confirmed. Nothing was merged.")
          : ["✓ merged #7 as e5bd3914e2e5"],
    });
    open("land-again");
    const widget = (await screen.findByText("widget · change/api-2")).closest("li");
    if (widget === null) throw new Error("no widget row");
    await userEvent.click(within(widget).getByRole("button", { name: "Land…" }));
    const asking = await screen.findByRole("alertdialog");
    await userEvent.click(within(asking).getByRole("button", { name: "Land" }));

    expect(
      await within(asking).findByText(
        "widget: this is not the landing you confirmed. Nothing was merged.",
      ),
    ).toBeInTheDocument();
    expect(await within(asking).findByText(MOVED)).toBeInTheDocument();
    await userEvent.click(within(asking).getByRole("button", { name: "Land" }));

    expect(await within(asking).findByText("✓ merged #7 as e5bd3914e2e5")).toBeInTheDocument();
    expect(called("change_land").map((c) => (c.args.confirmed as LandQuestion).head)).toEqual([
      HEAD,
      MOVED,
    ]);
  });

  it("asks first naming each repo, branch and destination, and a second click pushes nothing more", async () => {
    let finish: (lines: string[]) => void = () => {};
    const destinations = [
      {
        repo: "widget",
        branch: "change/api-2",
        head: HEAD,
        head_short: "6dcb09b5b578",
        to: "https://github.com/acme/widget.git",
        base: "main",
        forge: "github",
        request: "pull request",
      },
      {
        repo: "gadget",
        branch: "change/api-2",
        head: HEAD,
        head_short: "6dcb09b5b578",
        to: "https://gitlab.com/acme/gadget.git",
        base: "main",
        forge: "gitlab",
        request: "merge request",
      },
    ];
    const { called } = forge({
      change_push_question: () => ({ destinations, not_pushed: [] }),
      change_push: () =>
        new Promise<string[]>((done) => {
          finish = done;
        }),
    });
    open("push-asks");
    await userEvent.click(await screen.findByRole("button", { name: "Push api-2…" }));

    const asking = await screen.findByRole("alertdialog");
    expect(
      await within(asking).findByText(
        /branch change\/api-2 at 6dcb09b5b578 → https:\/\/github.com\/acme\/widget.git, its pull request into main/,
      ),
    ).toBeInTheDocument();
    expect(
      within(asking).getByText(
        /branch change\/api-2 at 6dcb09b5b578 → https:\/\/gitlab.com\/acme\/gadget.git, its merge request into main/,
      ),
    ).toBeInTheDocument();
    expect(called("change_push")).toHaveLength(0);

    const push = within(asking).getByRole("button", { name: "Push" });
    await userEvent.click(push);
    await userEvent.click(push);
    expect(push).toBeDisabled();
    finish(["✓ widget opened → #7"]);

    expect(await within(asking).findByText("✓ widget opened → #7")).toBeInTheDocument();
    expect(called("change_push")).toHaveLength(1);
    expect(called("change_push")[0].args).toMatchObject({
      change: "api-2",
      confirmed: destinations,
    });
  });

  it("asks again after a push is refused, so the next Push hands back what is shown now", async () => {
    const at = (head: string) => ({
      destinations: [
        {
          repo: "widget",
          branch: "change/api-2",
          head,
          head_short: head.slice(0, 12),
          to: "https://github.com/acme/widget.git",
          base: "main",
          forge: "github" as const,
          request: "pull request",
        },
      ],
      not_pushed: [],
    });
    let asked = 0;
    const { called } = forge({
      change_push_question: () => (asked++ === 0 ? at(HEAD) : at("a1b2c3d4e5f6a1b2c3d4")),
      change_push: () =>
        new Error(
          "what purlis would push is not what you confirmed: widget's branch change/api-2 is now at a1b2c3d4e5f6. Nothing was pushed.",
        ),
    });
    open("push-again");
    await userEvent.click(await screen.findByRole("button", { name: "Push api-2…" }));
    const asking = await screen.findByRole("alertdialog");
    await within(asking).findByText(/at 6dcb09b5b578 →/);
    await userEvent.click(within(asking).getByRole("button", { name: "Push" }));

    expect(await within(asking).findByText(/not what you confirmed/)).toBeInTheDocument();
    expect(await within(asking).findByText(/at a1b2c3d4e5f6 →/)).toBeInTheDocument();
    expect(called("change_push_question")).toHaveLength(2);
  });
});
