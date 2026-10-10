import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { LandAsk } from "./ChangeActions";
import type { LandQuestion } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const HEAD = "6dcb09b5b57875f334f61aebed695e2e4193db5e";

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
  said: [],
};

/** The core, answering `change_land` with `landed` and noting what it was handed. */
function core(landed: Promise<string[]>) {
  const asked: Record<string, unknown>[] = [];
  mockIPC((cmd, args) => {
    if (cmd !== "change_land") return null;
    asked.push(args as Record<string, unknown>);
    return landed;
  });
  return asked;
}

function land() {
  render(
    <LandAsk
      plane={PLANE}
      workspace="ide"
      change="fix-login"
      question={QUESTION}
      onClose={() => {}}
    />,
  );
}

describe("Land's squash box (DS-3e follow-up, #1210)", () => {
  it("is named for what it does, and what is ticked is handed to the landing", async () => {
    const asked = core(Promise.resolve(["✓ widget #7 merged"]));
    land();

    const squash = await screen.findByRole("checkbox", { name: "Squash it into one commit" });
    expect(squash).not.toBeChecked();
    await userEvent.click(squash);
    await userEvent.click(screen.getByRole("button", { name: "Land" }));

    await waitFor(() => expect(asked).toHaveLength(1));
    expect(asked[0].squash).toBe(true);
  });

  it("is held while the landing runs", async () => {
    core(new Promise<string[]>(() => {}));
    land();

    await userEvent.click(await screen.findByRole("button", { name: "Land" }));

    await waitFor(() =>
      expect(screen.getByRole("checkbox", { name: "Squash it into one commit" })).toBeDisabled(),
    );
  });
});
