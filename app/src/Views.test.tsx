import { StrictMode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ViewPane } from "./Views";
import type { ExtensionView, ViewAnswer } from "./bindings";
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
        { label: "Delegate to it for", value: "routing" },
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
        },
        {
          key: "b",
          text: "Grill back hard",
          note: "2026-09-21",
          mark: "note",
          tone: "plain",
          detail: { kind: "text", text: "Recommend, don't offer menus." },
          runs: null,
        },
      ],
      empty: { headline: "Nothing remembered yet", body: null, offer: null },
    },
  ],
  took_ms: 2,
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
    expect(labels).toEqual(["Delegate to it for", "Vault"]);
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

  it("says a persona the plane no longer has is gone, in the middle of the tab and not as an error", async () => {
    core(() => ({ kind: "gone", why: "This plane has no persona called steward any more." }));
    draw(STEWARD);

    const gone = await screen.findByTestId("view-gone");

    expect(gone).toHaveTextContent("steward is not here any more");
    expect(gone).toHaveTextContent("This plane has no persona called steward any more.");
    expect(screen.queryByRole("alert")).toBeNull();
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
        new Error(
          "'persona-statistics' has changed since you approved it — charter will ask again",
        ),
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

    it("does not make charter's own view wait, since it runs no program", async () => {
      const { opened } = core(() => PERSONA);
      draw(STEWARD, { waits: true });

      await waitFor(() => expect(opened).toHaveLength(1));
      expect(screen.queryByTestId("view-waits")).toBeNull();
    });
  });
});
