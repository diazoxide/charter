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

    expect(onStart).toHaveBeenCalledWith("claude", "release");
  });

  it("starts on the plane's own default persona when nobody picks another", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", "steward");
  });

  it("can start a chat that adopts no persona at all", async () => {
    const { onStart, user } = show();

    await user.click(screen.getByRole("radio", { name: "none" }));
    await user.click(screen.getByRole("button", { name: "Start" }));

    expect(onStart).toHaveBeenCalledWith("claude", null);
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

    expect(onApprove).toHaveBeenCalledWith("work");
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
});
