import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import type { HarnessGlance, ProfileRow, StartOptions } from "./bindings";
import { wordsOutsideTheFirstHour } from "./firstHour";
import { HarnessChip } from "./HarnessCard";
import { StartChat } from "./StartChat";

/**
 * The harness capability card at a glance (HP-19, W10, ADR 0072 §3): in the picker, under the
 * row that is picked, and in a chat's header, where it opens the card's own tab. What a card
 * says is the core's (`charter_core::harness_card`'s tests); here the core's answer is written
 * out, and what is held is where the window says it.
 */

afterEach(cleanup);

const CLAUDE: HarnessGlance = {
  name: "claude",
  title: "Claude Code",
  label: "What Claude Code can do here",
  lines: [],
  cannot_type: null,
};

const OPENCODE_LINES = [
  "opencode says nothing until your first prompt, so a new chat looks idle until then.",
  "opencode cannot have a prompt typed in for you, because charter cannot tell when it has finished starting.",
];

const OPENCODE: HarnessGlance = {
  name: "opencode",
  title: "opencode",
  label: "What opencode can do here",
  lines: OPENCODE_LINES,
  cannot_type: `${OPENCODE_LINES[1]} See What opencode can do here.`,
};

const row = (name: string, harness: HarnessGlance, isDefault: boolean): ProfileRow => ({
  name,
  kind: harness.name,
  shown: name,
  source: "built-in",
  is_default: isDefault,
  approval: null,
  ready_to_type: harness.cannot_type === null,
  harness,
});

const OPTIONS: StartOptions = {
  profiles: [row("claude", CLAUDE, true), row("opencode", OPENCODE, false)],
  refused: [],
  personas: [],
  persona: null,
  ignore_fix: null,
  declares_none: true,
};

function picker(options: StartOptions = OPTIONS) {
  render(<StartChat options={options} onStart={vi.fn()} onApprove={vi.fn()} onCancel={vi.fn()} />);
}

describe("the picker", () => {
  it("shows the picked harness's card, labelled and in a line for each thing it lacks", async () => {
    picker();

    const card = screen.getByRole("region", { name: "What Claude Code can do here" });
    expect(card).toHaveTextContent("Everything charter asks of it.");

    await userEvent.setup().click(screen.getByRole("radio", { name: "opencode" }));

    const other = screen.getByRole("region", { name: "What opencode can do here" });
    expect(
      within(other)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(OPENCODE_LINES);
    // In the first hour's words: the picker is one of its surfaces (ADR 0072 §3).
    expect(wordsOutsideTheFirstHour(other.textContent ?? "")).toEqual([]);
  });

  it("shows no card for a profile whose harness the project has no declaration of", () => {
    const bare = { ...row("custom", CLAUDE, true), harness: null };
    picker({ ...OPTIONS, profiles: [bare] });

    expect(screen.queryByTestId("harness-glance")).toBeNull();
  });
});

describe("a chat's header", () => {
  it("names the harness by its product, says what it lacks, and opens its card", async () => {
    const onOpen = vi.fn();
    render(<HarnessChip glance={OPENCODE} onOpen={onOpen} />);

    const chip = screen.getByRole("button", { name: "What opencode can do here" });
    expect(chip).toHaveTextContent("opencode");
    expect(chip).toHaveAttribute("title", OPENCODE_LINES.join(" "));

    await userEvent.setup().click(chip);

    expect(onOpen).toHaveBeenCalledOnce();
  });

  it("says a harness that lacks nothing lacks nothing", () => {
    render(<HarnessChip glance={CLAUDE} onOpen={vi.fn()} />);

    expect(screen.getByRole("button", { name: "What Claude Code can do here" })).toHaveAttribute(
      "title",
      "Everything charter asks of it.",
    );
  });
});
