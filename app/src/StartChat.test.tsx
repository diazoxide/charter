import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import type { StartOptions } from "./bindings";
import { StartChat } from "./StartChat";

afterEach(cleanup);

const ONE: StartOptions = {
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
  personas: ["steward", "release"],
  persona: "steward",
  ignore_fix: null,
  declares_none: true,
};

const options = (over: Partial<StartOptions> = {}): StartOptions => ({ ...ONE, ...over });

function show(over: Partial<StartOptions> = {}) {
  const onStart = vi.fn();
  const onApprove = vi.fn();
  const onCancel = vi.fn();
  render(
    <StartChat
      options={options(over)}
      onStart={onStart}
      onApprove={onApprove}
      onCancel={onCancel}
    />,
  );
  return { onStart, onApprove, onCancel, user: userEvent.setup() };
}

describe("the picker a chat starts from", () => {
  it("shows even when one profile is available, because one profile costs one Enter", () => {
    // Skipping it would bring back the harness nobody picked on a one-harness machine,
    // which is one of the two failures ADR 0022 exists to prevent.
    show();

    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /claude/ })).toBeChecked();
  });

  it("starts nothing on Escape", async () => {
    const { onCancel, onStart, user } = show();

    await user.keyboard("{Escape}");

    expect(onCancel).toHaveBeenCalled();
    expect(onStart).not.toHaveBeenCalled();
  });

  it("starts on the profile and the persona that were picked", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("radio", { name: /release/ }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "release", false);
  });

  it("starts on the plane's own default persona when nobody picks another", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false);
  });

  it("can start a chat that adopts no persona at all", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("radio", { name: "none" }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", null, false);
  });

  it("leaves the pane's footer blank unless this chat asks for charter's", async () => {
    // The default, and ADR 0029 keeps it: nobody's pane moves on an upgrade. The
    // box is drawn unticked and the start carries `false`.
    const { onStart, user } = show();

    expect(screen.getByRole("checkbox", { name: /charter's footer/ })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false);
  });

  it("starts a chat that draws charter's footer when the box is ticked", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("checkbox", { name: /charter's footer/ }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", true);
  });

  it("says why charter blanks it, rather than leaving the box to be guessed at", () => {
    // The panels repeat most of it, and it says something about THIS chat that they say
    // only for the focused one. An operator deciding this is owed both halves on screen.
    show();

    expect(screen.getByText(/panels already draw the plane/)).toBeInTheDocument();
    expect(screen.getByText(/This chat only/)).toBeInTheDocument();
  });

  it("carries the footer choice through the approval, which is where a pick gets dropped", async () => {
    // The persona used to be dropped on exactly this path — a profile's first run. The
    // footer rides the same call and would be lost the same way.
    const { onApprove, user } = show({
      profiles: [
        {
          name: "work",
          kind: "claude",
          shown: "claude --model opus",
          source: "charter.local.toml",
          is_default: true,
          approval: "new",
        },
      ],
    });

    await user.click(screen.getByRole("checkbox", { name: /charter's footer/ }));
    await user.click(screen.getByRole("button", { name: "Approve and start" }));

    expect(onApprove).toHaveBeenCalledWith("work", "steward", true, "claude --model opus");
  });

  it("shows the command and asks, for a profile charter has not recorded running", async () => {
    // The file is gitignored, so an edit to it leaves no diff for a reviewer to catch —
    // which is why the ask is about the WORDS that are about to run, not the profile's name.
    const { onStart, onApprove, user } = show({
      profiles: [
        {
          name: "work",
          kind: "claude",
          shown: "CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus",
          source: "charter.local.toml",
          is_default: true,
          approval: "new",
        },
      ],
    });

    expect(screen.getByRole("alert")).toHaveTextContent(
      "CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus",
    );
    await user.click(screen.getByRole("button", { name: "Approve and start" }));

    // With the persona, not only the profile: the approve path used to drop the pick.
    expect(onApprove).toHaveBeenCalledWith(
      "work",
      "steward",
      false,
      "CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus",
    );
    expect(onStart).not.toHaveBeenCalled();
  });

  it("says a changed command is changed rather than new", () => {
    show({
      profiles: [
        {
          name: "work",
          kind: "claude",
          shown: "claude",
          source: "charter.local.toml",
          is_default: true,
          approval: "changed",
        },
      ],
    });

    expect(screen.getByRole("alert")).toHaveTextContent("as it now stands");
  });

  it("says a plane that declares nothing declares nothing, rather than looking broken", () => {
    show();

    expect(screen.getByText(/declares no profiles of its own/)).toBeInTheDocument();
  });

  it("says when git would carry the file, because then every declared profile is refused", () => {
    show({ declares_none: false, ignore_fix: "charter reinit" });

    expect(screen.getByRole("alert")).toHaveTextContent("charter reinit");
  });

  it("names the profiles it will not use, so a missing row is never merely missing", async () => {
    const { user } = show({
      refused: [["bad-kind", "profile 'bad-kind' has kind opencodex, which is not a harness"]],
    });

    await user.click(screen.getByText("1 refused"));

    expect(screen.getByText(/has kind opencodex/)).toBeInTheDocument();
  });

  it("puts Cancel under the key a stray Return finds, never the start", () => {
    // Starting a chat runs a command with nothing between the key and the exec.
    show();

    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });
  it("offers a chat with no persona at all when the plane declares none", () => {
    // A plane with no `personas/` at all is the ordinary first state, and `_shared` is the
    // store every persona reads rather than a persona anybody adopts — so a plane can
    // legitimately have nothing to list here. The picker still starts a chat.
    const { onStart, user } = show({ personas: [], persona: null });

    expect(screen.getByRole("radio", { name: "none" })).toBeChecked();

    return user.click(screen.getByRole("button", { name: "Start" })).then(() => {
      expect(onStart).toHaveBeenCalledWith("claude", null, false);
    });
  });

  describe("the markup underneath it", () => {
    // The dialog the operator opened read `claudeclaudeclaudebuilt-indefault`: five spans in
    // one `<label>`, with no rule laying them out and no association between the words and
    // the control. These pin what replaced it (`docs/ui-primitives.md`), and they are the
    // tests a hand-rolled version cannot pass.

    const WORK: StartOptions = options({
      profiles: [
        {
          name: "work",
          kind: "claude",
          shown: "CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus",
          source: "charter.local.toml",
          is_default: true,
          approval: null,
        },
        {
          name: "plain",
          kind: "claude",
          shown: "claude",
          source: "built-in",
          is_default: false,
          approval: null,
        },
      ],
    });

    it("calls a row by its profile's name and not by every column in it", () => {
      // `name: "work"` is exact. It was `workclaudeCLAUDE_CONFIG_DIR=…charter.local.tomldefault`
      // — the whole row run together, which is what both a screen reader and the screen got.
      show(WORK);

      expect(screen.getByRole("radio", { name: "work" })).toBeInTheDocument();
      expect(screen.getByRole("radio", { name: "plain" })).toBeInTheDocument();
    });

    it("keeps the command line on the row as its description, not folded into its name", () => {
      // The words that are about to run still have to be readable beside the row that runs
      // them — moving them out of the name must not move them off the screen.
      show(WORK);

      expect(screen.getByRole("radio", { name: "work" })).toHaveAccessibleDescription(
        /CLAUDE_CONFIG_DIR=~\/\.claude-work claude --model opus/,
      );
      expect(
        screen.getByText("CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus"),
      ).toBeInTheDocument();
    });

    it("picks the row the label was clicked on, because the label is tied to the control", () => {
      // The association is the whole point: the words were beside the control and named
      // nothing, so clicking them did nothing.
      const { onStart, user } = show(WORK);

      return user
        .click(screen.getByText("plain"))
        .then(() => user.click(screen.getByRole("button", { name: "Start" })))
        .then(() => {
          expect(onStart).toHaveBeenCalledWith("plain", "steward", false);
        });
    });

    it("moves between harnesses with the arrow keys, as one group and not five stops", async () => {
      const { onStart, user } = show(WORK);

      await user.click(screen.getByRole("radio", { name: "work" }));
      await user.keyboard("{ArrowDown}");
      await user.click(screen.getByRole("button", { name: "Start" }));

      expect(onStart).toHaveBeenCalledWith("plain", "steward", false);
    });

    it("hides the window behind it from the keyboard and from the accessibility tree", () => {
      // Modal was a word in an attribute and a grey overlay. Nothing enforced it: the tab
      // strip behind the picker was reachable by Tab and listed by every role query, and one
      // of this repo's own tests was ending a chat through it while a chat was starting.
      const onStart = vi.fn();
      render(
        <>
          <button>a tab behind it</button>
          <StartChat options={options()} onStart={onStart} onApprove={vi.fn()} onCancel={vi.fn()} />
        </>,
      );

      expect(screen.queryByRole("button", { name: "a tab behind it" })).not.toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    });
  });

  it("never preselects a persona it does not draw a row for", async () => {
    // The core filters `[persona] default` against the personas the plane has, so this
    // should be unreachable from the app. Pinned anyway: if it ever arrives, the picker
    // must not start a chat on a persona the operator cannot see or change.
    const { onStart, user } = show({ personas: ["release"], persona: "gone" });

    await user.click(screen.getByRole("button", { name: "Start" }));

    const [, persona] = onStart.mock.calls[0] as [string, string | null];
    expect(["release", null]).toContain(persona);
  });
});
