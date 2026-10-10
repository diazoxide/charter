import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { userEvent } from "@testing-library/user-event";
import type { StartOptions } from "./bindings";
import { StartChat } from "./StartChat";
import { profileCommand, profilePage } from "./settings/profileAddress";

afterEach(cleanup);

const ONE: StartOptions = {
  profiles: [
    {
      name: "claude",
      kind: "claude",
      shown: "claude",
      source: "built-in",
      is_default: true,
      ready_to_type: true,
      harness: null,
      sandbox: null,
      approval: null,
    },
  ],
  refused: [],
  personas: ["steward", "release"],
  persona: "steward",
  persona_profiles: {},
  ignore_fix: null,
  ignore_fix_id: null,
  declares_none: true,
};

const options = (over: Partial<StartOptions> = {}): StartOptions => ({ ...ONE, ...over });

function show(over: Partial<StartOptions> = {}, repo?: string) {
  const onStart = vi.fn();
  const onApprove = vi.fn();
  const onCancel = vi.fn();
  render(
    <StartChat
      options={options(over)}
      repo={repo}
      onStart={onStart}
      onApprove={onApprove}
      onCancel={onCancel}
    />,
  );
  return { onStart, onApprove, onCancel, user: userEvent.setup() };
}

describe("a persona's one-line description (#1460)", () => {
  it("is said on its row, before the row's other facts", () => {
    show({
      personas: ["steward", "release", "ops"],
      persona_descriptions: { steward: "Scopes the work", ops: "Keeps the lights on" },
    });

    expect(screen.getByRole("radio", { name: /^steward/ })).toHaveAccessibleDescription(
      "Scopes the work · project default",
    );
    expect(screen.getByRole("radio", { name: /^ops/ })).toHaveAccessibleDescription(
      "Keeps the lights on",
    );
    expect(screen.getByRole("radio", { name: /^release/ })).not.toHaveAccessibleDescription();
  });

  it("is nothing for an answer that carries none", () => {
    show({ persona_descriptions: undefined });

    expect(screen.getByRole("radio", { name: /^release/ })).not.toHaveAccessibleDescription();
  });
});

describe("a persona's own profile (#1445)", () => {
  const codex = { ...ONE.profiles[0], name: "codex", kind: "codex", shown: "codex" };
  const work = { ...ONE.profiles[0], name: "work", shown: "claude --work", is_default: false };
  const TWO: Partial<StartOptions> = {
    profiles: [ONE.profiles[0], { ...codex, is_default: false }, work],
    personas: ["steward", "release", "ops"],
    persona_profiles: { release: "codex" },
  };

  it("moves the harness to a persona's own profile when that persona is picked", async () => {
    const { onStart, user } = show(TWO);
    expect(screen.getByRole("radio", { name: /^claude/ })).toBeChecked();

    await user.click(screen.getByRole("radio", { name: /^release/ }));

    expect(screen.getByRole("radio", { name: /^codex/ })).toBeChecked();
    await user.click(screen.getByRole("button", { name: "Start" }));
    expect(onStart).toHaveBeenCalledWith("codex", "release", false, null, false, null);
  });

  it("says which profile is a persona's own on its row", () => {
    show(TWO);

    expect(screen.getByRole("radio", { name: /^release/ })).toHaveAccessibleDescription(
      "its profile is codex",
    );
    expect(screen.getByRole("radio", { name: /^ops/ })).not.toHaveAccessibleDescription();
  });

  it("leaves the harness the person's to change after a persona is picked", async () => {
    const { onStart, user } = show(TWO);

    await user.click(screen.getByRole("radio", { name: /^release/ }));
    await user.click(screen.getByRole("radio", { name: /^work/ }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("work", "release", false, null, false, null);
  });

  it("starts on the default persona's own profile", () => {
    show({ ...TWO, persona_profiles: { steward: "codex" } });

    expect(screen.getByRole("radio", { name: /^codex/ })).toBeChecked();
  });

  it("keeps a harness the person picked when a persona with its own profile is picked after", async () => {
    // Harness first, then persona: the Harness row is drawn above the Persona row.
    const { onStart, user } = show(TWO);

    await user.click(screen.getByRole("radio", { name: /^work/ }));
    await user.click(screen.getByRole("radio", { name: /^release/ }));

    expect(screen.getByRole("radio", { name: /^work/ })).toBeChecked();
    await user.click(screen.getByRole("button", { name: "Start" }));
    expect(onStart).toHaveBeenCalledWith("work", "release", false, null, false, null);
  });

  it("keeps a harness the person picked for a persona that names no profile", async () => {
    const { user } = show(TWO);

    await user.click(screen.getByRole("radio", { name: /^work/ }));
    await user.click(screen.getByRole("radio", { name: /^ops/ }));

    expect(screen.getByRole("radio", { name: /^work/ })).toBeChecked();
  });

  it("goes back to the default when the persona picked next names no profile", async () => {
    // Persona with a profile, then one without: codex must not be left over from `release`.
    const { onStart, user } = show(TWO);

    await user.click(screen.getByRole("radio", { name: /^release/ }));
    expect(screen.getByRole("radio", { name: /^codex/ })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: /^ops/ }));

    expect(screen.getByRole("radio", { name: /^claude/ })).toBeChecked();
    await user.click(screen.getByRole("button", { name: "Start" }));
    expect(onStart).toHaveBeenCalledWith("claude", "ops", false, null, false, null);
  });

  it("goes back to the default when no persona is picked after one with a profile", async () => {
    const { user } = show(TWO);

    await user.click(screen.getByRole("radio", { name: /^release/ }));
    await user.click(screen.getByRole("radio", { name: "none" }));

    expect(screen.getByRole("radio", { name: /^claude/ })).toBeChecked();
  });

  it("leaves a harness something asked for where it is", async () => {
    const onStart = vi.fn();
    render(
      <StartChat
        options={options(TWO)}
        prefer="claude"
        onStart={onStart}
        onApprove={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByRole("radio", { name: /^release/ }));

    expect(screen.getByRole("radio", { name: /^claude/ })).toBeChecked();
  });

  it("moves nowhere for a profile the picker has no row for", async () => {
    const { user } = show({ ...TWO, persona_profiles: { release: "gone" } });

    await user.click(screen.getByRole("radio", { name: /^release/ }));

    expect(screen.getByRole("radio", { name: /^claude/ })).toBeChecked();
  });
});

describe("a chat that starts in a repo (GL-1)", () => {
  it("starts on a new branch in that repo unless told otherwise", async () => {
    const { onStart, user } = show({}, "api");

    expect(screen.getByRole("checkbox", { name: /new branch in api/ })).toBeChecked();
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, true, null);
  });

  it("works on the branch the repo has checked out when the box is cleared", async () => {
    const { onStart, user } = show({}, "api");

    await user.click(screen.getByRole("checkbox", { name: /new branch in api/ }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, null);
  });

  it("asks nothing about a branch for a chat that does not start in a repo", () => {
    show();

    expect(screen.queryByRole("checkbox", { name: /new branch/ })).not.toBeInTheDocument();
  });

  it("draws its form from the settings set, every control a row", () => {
    // DS-3d (#1176): the picker's harness, persona, boxes and fields are the settings set's
    // rows, not the dialogs' own `choices` and `asks`.
    show({}, "api");

    const dialog = screen.getByRole("dialog");
    expect(dialog.querySelectorAll(".ui-setting-row")).toHaveLength(5);
    expect(screen.getByRole("radiogroup", { name: "Harness" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Persona" })).toBeInTheDocument();
  });

  it("says nothing the first hour does not say", () => {
    // ADR 0072 §3: the picker is a first-hour surface, so it says branch and never the
    // words charter keeps for its own internals.
    show({}, "api");

    const said = screen.getByRole("dialog").textContent ?? "";
    expect(said).not.toMatch(/worktree|piece/i);
  });
});

describe("a start already running (GL-1)", () => {
  it("says so on Start, which cannot be pressed again", async () => {
    const onStart = vi.fn();
    render(
      <StartChat
        options={options()}
        starting
        onStart={onStart}
        onApprove={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    const start = screen.getByRole("button", { name: "Starting…" });
    expect(start).toBeDisabled();
    await userEvent.setup().click(start);
    expect(onStart).not.toHaveBeenCalled();
  });
});

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

  describe("the keyboard once it closes (#1600)", () => {
    // The window draws the dialog once the options are read, and closes it by no longer
    // drawing it: the whole dialog goes at once, as `PlaneView` does it.
    function Window() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button type="button" onClick={() => setOpen(true)}>
            New tab
          </button>
          {open && (
            <StartChat
              options={options()}
              onStart={() => setOpen(false)}
              onApprove={vi.fn()}
              onCancel={() => setOpen(false)}
            />
          )}
        </>
      );
    }

    async function opened() {
      render(<Window />);
      const user = userEvent.setup();
      const plus = screen.getByRole("button", { name: "New tab" });
      plus.focus();
      await user.keyboard("{Enter}");
      await screen.findByRole("dialog", { name: "Start a chat" });
      return { plus, user };
    }

    it("goes back to the + that opened it on Escape", async () => {
      const { plus, user } = await opened();

      await user.keyboard("{Escape}");

      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(plus).toHaveFocus());
    });

    it("goes back to the + on Cancel", async () => {
      const { plus, user } = await opened();

      await user.click(screen.getByRole("button", { name: "Cancel" }));

      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(plus).toHaveFocus());
    });

    it("is left for the new chat on a start", async () => {
      const { plus, user } = await opened();

      await user.click(screen.getByRole("button", { name: "Start" }));

      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await new Promise((resolve) => setTimeout(resolve, 10));
      expect(plus).not.toHaveFocus();
    });
  });

  it("starts on the profile and the persona that were picked", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("radio", { name: /release/ }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "release", false, null, false, null);
  });

  it("starts on the plane's own default persona when nobody picks another", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, null);
  });

  it("can start a chat that adopts no persona at all", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("radio", { name: "none" }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", null, false, null, false, null);
  });

  it("leaves the pane's footer blank unless this chat asks for charter's", async () => {
    // The default, and ADR 0029 keeps it: nobody's pane moves on an upgrade. The
    // box is drawn unticked and the start carries `false`.
    const { onStart, user } = show();

    expect(screen.getByRole("checkbox", { name: /purlis's footer/ })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, null);
  });

  it("starts a chat that draws purlis's footer when the box is ticked", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("checkbox", { name: /purlis's footer/ }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", true, null, false, null);
  });

  it("starts under the name typed in the Name field (charter-app#254)", async () => {
    const { onStart, user } = show();

    await user.type(screen.getByRole("textbox", { name: /^Name/ }), "billing bug");
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, "billing bug", false, null);
  });

  it("starts under the default name when the Name field is only spaces", async () => {
    const { onStart, user } = show();

    await user.type(screen.getByRole("textbox", { name: /^Name/ }), "   ");
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, null);
  });

  it("says the Name field is optional and what an empty one means", () => {
    show();

    expect(screen.getByRole("textbox", { name: /^Name/ })).toHaveAccessibleDescription(
      /empty.*persona.*number/i,
    );
  });

  it("says why charter blanks it, rather than leaving the box to be guessed at", () => {
    // The panels repeat most of it, and it says something about THIS chat that they say
    // only for the focused one. An operator deciding this is owed both halves on screen.
    show();

    expect(screen.getByText(/panels already draw the project/)).toBeInTheDocument();
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
          ready_to_type: true,
          harness: null,
          sandbox: null,
          approval: "new",
        },
      ],
    });

    await user.click(screen.getByRole("checkbox", { name: /purlis's footer/ }));
    await user.click(screen.getByRole("button", { name: "Approve and start" }));

    expect(onApprove).toHaveBeenCalledWith(
      "work",
      "steward",
      true,
      "claude --model opus",
      null,
      false,
      null,
    );
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
          ready_to_type: true,
          harness: null,
          sandbox: null,
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
      null,
      false,
      null,
    );
    expect(onStart).not.toHaveBeenCalled();
  });

  it("shows a command longer than the display limit whole, and approves that line (#1014)", async () => {
    const long = `claude --note ${"x".repeat(200)} --and-then the-last-word (kind claude)`;
    const { onApprove, user } = show({
      profiles: [
        {
          name: "work",
          kind: "claude",
          shown: long,
          source: "charter.local.toml",
          is_default: true,
          ready_to_type: true,
          harness: null,
          sandbox: null,
          approval: "new",
        },
      ],
    });

    expect(screen.getByRole("alert").querySelector("code")?.textContent).toBe(long);
    await user.click(screen.getByRole("button", { name: "Approve and start" }));

    expect(onApprove).toHaveBeenCalledWith("work", "steward", false, long, null, false, null);
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
          ready_to_type: true,
          harness: null,
          sandbox: null,
          approval: "changed",
        },
      ],
    });

    expect(screen.getByRole("alert")).toHaveTextContent("as it now stands");
  });

  it("says a project that declares nothing declares nothing, rather than looking broken", () => {
    show();

    // "project", the window's word (#602), never the retired "plane".
    expect(screen.getByText(/This project declares no profiles of its own/)).toBeInTheDocument();
    expect(screen.queryByText(/\bplane\b/)).not.toBeInTheDocument();
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

  describe("its refusals' ways out (NO-8, #1233)", () => {
    function withWays(over: Partial<StartOptions>, fixing = false) {
      const onFix = vi.fn();
      const onOpenSettings = vi.fn();
      render(
        <StartChat
          options={options(over)}
          fixing={fixing}
          onStart={vi.fn()}
          onApprove={vi.fn()}
          onFix={onFix}
          onOpenSettings={onOpenSettings}
          onCancel={vi.fn()}
        />,
      );
      return { onFix, onOpenSettings, user: userEvent.setup() };
    }

    it("links each refused profile to its own page in Settings, its command focused (#1296)", async () => {
      const { onOpenSettings, user } = withWays({
        refused: [
          ["bad-kind", "profile 'bad-kind' has kind opencodex, which is not a harness"],
          ["team", "profile 'team' has no command"],
        ],
      });

      await user.click(screen.getByText("2 refused"));
      await user.click(screen.getByRole("button", { name: "Open team in Settings" }));

      // A profile with no page of its own (a committed one) lands on Harness & profiles: the
      // Settings tab takes an unknown sub-page to its parent group.
      expect(onOpenSettings).toHaveBeenCalledWith(profilePage("team"), profileCommand("team"));
      expect(screen.getByRole("button", { name: "Open bad-kind in Settings" })).toBeVisible();
    });

    it("fixes the file git would carry with the doctor's own fix, where one line cures it", async () => {
      const { onFix, user } = withWays({
        declares_none: false,
        persona_profiles: {},
        ignore_fix: "add /charter.local.toml to .gitignore",
        ignore_fix_id: "local-ignore",
      });

      await user.click(screen.getByRole("button", { name: "Add the ignore line" }));

      expect(onFix).toHaveBeenCalledWith("local-ignore");
    });

    it("says the fix is running and cannot be pressed twice", () => {
      withWays({ declares_none: false, ignore_fix: "x", ignore_fix_id: "local-ignore" }, true);

      expect(screen.getByRole("button", { name: "Adding the ignore line…" })).toBeDisabled();
    });

    it("offers no button where one ignore line would not cure it", () => {
      // A file git already tracks needs the operator's own `git rm --cached` first (FX-2).
      withWays({ declares_none: false, ignore_fix: "git rm --cached", ignore_fix_id: null });

      expect(screen.queryByRole("button", { name: /ignore line/ })).not.toBeInTheDocument();
      expect(screen.getByRole("alert")).toHaveTextContent("git rm --cached");
    });
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
      expect(onStart).toHaveBeenCalledWith("claude", null, false, null, false, null);
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
          ready_to_type: true,
          harness: null,
          sandbox: null,
          approval: null,
        },
        {
          name: "plain",
          kind: "claude",
          shown: "claude",
          source: "built-in",
          is_default: false,
          ready_to_type: true,
          harness: null,
          sandbox: null,
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
          expect(onStart).toHaveBeenCalledWith("plain", "steward", false, null, false, null);
        });
    });

    it("moves between harnesses with the arrow keys, as one group and not five stops", async () => {
      const { onStart, user } = show(WORK);

      await user.click(screen.getByRole("radio", { name: "work" }));
      await user.keyboard("{ArrowDown}");
      await user.click(screen.getByRole("button", { name: "Start" }));

      expect(onStart).toHaveBeenCalledWith("plain", "steward", false, null, false, null);
    });

    it("starts nothing while an arrow key is held on the harnesses", async () => {
      // A held key repeats, and a radio's pick follows it: the pick is only ever held, and
      // the chat starts on Start (DS-3d).
      const { onStart, onApprove, user } = show(WORK);

      await user.click(screen.getByRole("radio", { name: "work" }));
      await user.keyboard("{ArrowDown>}");
      await new Promise((done) => setTimeout(done, 80));
      await user.keyboard("{/ArrowDown}");

      expect(screen.getByRole("radio", { name: "plain" })).toBeChecked();
      expect(onStart).not.toHaveBeenCalled();
      expect(onApprove).not.toHaveBeenCalled();
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

describe("a chat in a project that runs every chat sandboxed (ADR 0067 §7, V78 a)", () => {
  const sandboxed = (sandbox: StartOptions["profiles"][number]["sandbox"]) =>
    options({ profiles: [{ ...ONE.profiles[0], sandbox }] });

  function showWith(sandbox: StartOptions["profiles"][number]["sandbox"], onInstall?: () => void) {
    const onStart = vi.fn();
    const onApprove = vi.fn();
    render(
      <StartChat
        options={sandboxed(sandbox)}
        onStart={onStart}
        onApprove={onApprove}
        onInstall={onInstall}
        onCancel={vi.fn()}
      />,
    );
    return { onStart, onApprove, user: userEvent.setup() };
  }

  it("says nothing of a sandbox the project has not turned on", () => {
    show();
    expect(screen.queryByRole("checkbox", { name: /without the sandbox/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/sandbox/i)).not.toBeInTheDocument();
  });

  it("heads what it says about the sandbox, in every state of it", () => {
    for (const sandbox of [
      {
        state: "unsandboxed",
        said: "This system has no sandbox backend.",
        install: null,
        locked: null,
      },
      { state: "refused", said: "bwrap is not installed.", install: null, locked: null },
    ] as const) {
      showWith(sandbox);
      const region = screen.getByRole("region", { name: "Sandbox" });
      expect(region).toHaveTextContent(sandbox.said);
      cleanup();
    }
  });

  it("starts sandboxed unless the person ticks the box", async () => {
    const { onStart, user } = showWith({
      state: "sandboxed",
      said: "",
      install: null,
      locked: null,
    });

    expect(screen.getByRole("checkbox", { name: "start without the sandbox" })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, null);
  });

  it("starts this one chat without the sandbox, with the reason typed, and says so on the button", async () => {
    const { onStart, user } = showWith({
      state: "sandboxed",
      said: "",
      install: null,
      locked: null,
    });

    await user.click(screen.getByRole("checkbox", { name: "start without the sandbox" }));
    await user.type(
      screen.getByRole("textbox", { name: "Why, if you want it recorded" }),
      "the build needs the network",
    );
    await user.click(screen.getByRole("button", { name: "Start without the sandbox" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, {
      reason: "the build needs the network",
    });
  });

  it("sends no reason when none was typed", async () => {
    const { onStart, user } = showWith({
      state: "sandboxed",
      said: "",
      install: null,
      locked: null,
    });

    await user.click(screen.getByRole("checkbox", { name: "start without the sandbox" }));
    await user.click(screen.getByRole("button", { name: "Start without the sandbox" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, {
      reason: null,
    });
  });

  it("shows why the sandbox cannot be applied, and the one way on is said on the button", async () => {
    const why =
      "this project runs every chat sandboxed, and this machine cannot apply the sandbox: socat is not installed";
    const { onStart, user } = showWith({
      state: "refused",
      said: why,
      install: null,
      locked: null,
    });

    expect(screen.getByRole("alert")).toHaveTextContent(why);
    expect(screen.queryByRole("button", { name: "Start" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Start without the sandbox" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, {
      reason: null,
    });
  });

  it("shows the distribution's install command and types it in a shell tab only when asked", async () => {
    const onInstall = vi.fn();
    const { onStart, user } = showWith(
      {
        state: "refused",
        said: "socat is not installed",
        install: "sudo apt install socat",
        locked: null,
      },
      onInstall,
    );

    expect(screen.getByText("sudo apt install socat").tagName).toBe("CODE");
    await user.click(screen.getByRole("button", { name: "Type it in a shell tab" }));

    expect(onInstall).toHaveBeenCalledTimes(1);
    expect(onStart).not.toHaveBeenCalled();
  });

  it("says why a chat on a system with no backend starts without the sandbox", () => {
    const said =
      "This chat runs without the sandbox: purlis has no sandbox backend on Windows yet, so every chat here starts without it until one exists.";
    showWith({ state: "unsandboxed", said, install: null, locked: null });

    expect(screen.getByText(said)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start" })).toBeInTheDocument();
  });

  it("offers no opt-out where policy forbids it, and says who locked it (#1343)", async () => {
    const locked =
      "Policy forbids starting a chat without the sandbox. Locked by policy, set by Platform team in /etc/purlis/policy.json.";
    const { onStart, user } = showWith({ state: "sandboxed", said: "", install: null, locked });

    expect(
      screen.queryByRole("checkbox", { name: "start without the sandbox" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Sandbox" })).toHaveTextContent(locked);
    await user.click(screen.getByRole("button", { name: "Start" }));
    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, null);
  });

  it("starts nothing where the sandbox cannot be applied and policy forbids the opt-out (#1343)", () => {
    const locked =
      "Policy forbids starting a chat without the sandbox. Locked by policy, set by Platform team in /etc/purlis/policy.json.";
    const { onStart } = showWith({
      state: "refused",
      said: "socat is not installed",
      install: null,
      locked,
    });

    expect(
      screen.queryByRole("button", { name: "Start without the sandbox" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start" })).toBeDisabled();
    expect(screen.getByRole("region", { name: "Sandbox" })).toHaveTextContent(locked);
    expect(onStart).not.toHaveBeenCalled();
  });

  it("starts nothing on a system with no backend where policy requires the sandbox, and names no opt-out (#1423)", () => {
    const said =
      "purlis has no sandbox backend on Windows yet, and policy requires the sandbox for every chat on this machine, so nothing was started.";
    const locked =
      "Policy forbids starting a chat without the sandbox. Locked by policy, set by Platform team in /etc/purlis/policy.json.";
    const { onStart } = showWith({ state: "refused", said, install: null, locked });

    const sandbox = screen.getByRole("region", { name: "Sandbox" });
    expect(sandbox).toHaveTextContent(said);
    // Who to ask, once: the policy and its owner.
    expect(sandbox).toHaveTextContent(locked);
    expect(sandbox.textContent?.split("Locked by policy")).toHaveLength(2);
    expect(sandbox).not.toHaveTextContent("new-chat picker");
    expect(screen.queryByRole("checkbox", { name: "start without the sandbox" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Start without the sandbox" })).toBeNull();
    expect(screen.getByRole("button", { name: "Start" })).toBeDisabled();
    expect(onStart).not.toHaveBeenCalled();
  });

  it("carries the opt-out through the approval, and says so on its button", async () => {
    const onApprove = vi.fn();
    render(
      <StartChat
        options={options({
          profiles: [
            {
              ...ONE.profiles[0],
              approval: "new",
              sandbox: {
                state: "refused",
                said: "socat is not installed",
                install: null,
                locked: null,
              },
            },
          ],
        })}
        onStart={vi.fn()}
        onApprove={onApprove}
        onCancel={vi.fn()}
      />,
    );

    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Approve and start without the sandbox" }));

    expect(onApprove).toHaveBeenCalledWith("claude", "steward", false, "claude", null, false, {
      reason: null,
    });
  });
});

describe("a harness charter cannot wrap on this system in a sandboxed project", () => {
  it("shows Codex's refusal and starts it only without the sandbox", async () => {
    const said =
      "this project runs every chat sandboxed, and purlis runs Codex inside a sandbox of its own, which it can apply on macOS but not yet on Linux, so it was not started. Start this chat on a Claude Code profile.";
    const onStart = vi.fn();
    render(
      <StartChat
        options={options({
          profiles: [
            {
              ...ONE.profiles[0],
              name: "codex",
              kind: "codex",
              shown: "codex",
              sandbox: { state: "refused", said, install: null, locked: null },
            },
          ],
        })}
        onStart={onStart}
        onApprove={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(said);
    expect(screen.queryByRole("button", { name: "Start" })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Type it in a shell tab" }),
    ).not.toBeInTheDocument();
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Start without the sandbox" }));

    expect(onStart).toHaveBeenCalledWith("codex", "steward", false, null, false, { reason: null });
  });
});

describe("a program the sandbox will not bind (V87g)", () => {
  it.each([
    [
      "a program where the chat can write",
      "this project runs every chat sandboxed, and the program lives where this chat can write: /work/acme/bin/claude, so it was not started sandboxed. Keep the program outside the project and outside what a chat may write.",
    ],
    [
      "a command naming a file where the chat can write",
      "this project runs every chat sandboxed, and this profile's command names /tmp/run.sh, which lies where this chat can write, so it was not started sandboxed. Keep every file the command names outside the project and outside what a chat may write.",
    ],
    [
      "a command word too long to check",
      "this project runs every chat sandboxed, and a word of this profile's command is longer than 4 KiB, which purlis does not check, so it was not started sandboxed. Keep what it says in a file outside the project and name that file instead.",
    ],
    [
      "a program named by a relative path",
      "this project runs every chat sandboxed, and this profile's program is a relative path, which would be found in a folder the chat can write, so it was not started sandboxed. Name the program by its full path.",
    ],
    [
      "a program that does not answer as Claude Code",
      "this project runs every chat sandboxed, and this profile's program does not answer as Claude Code, whose sandbox it was given, so it was not started sandboxed.",
    ],
  ])("shows %s, and starts it only without the sandbox", async (_, said) => {
    const onStart = vi.fn();
    render(
      <StartChat
        options={options({
          profiles: [
            {
              ...ONE.profiles[0],
              sandbox: { state: "refused", said, install: null, locked: null },
            },
          ],
        })}
        onStart={onStart}
        onApprove={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(said);
    expect(screen.queryByRole("button", { name: "Start" })).not.toBeInTheDocument();
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Start without the sandbox" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward", false, null, false, { reason: null });
  });
});

describe("a profile nobody has approved, in a sandboxed project", () => {
  it("says its program is checked once it is approved, and still asks for the approval", () => {
    const said =
      "purlis checks this profile's program before it starts sandboxed, once the profile may start: approved, and declared in a file git does not carry.";
    render(
      <StartChat
        options={options({
          profiles: [
            {
              ...ONE.profiles[0],
              approval: "new",
              sandbox: { state: "sandboxed", said, install: null, locked: null },
            },
          ],
        })}
        onStart={vi.fn()}
        onApprove={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText(said)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Approve and start" })).toBeInTheDocument();
  });
});
