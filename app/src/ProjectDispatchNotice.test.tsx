import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { ProjectDispatchNotice } from "./ProjectDispatchNotice";
import type { DispatchGrants } from "./bindings";

/**
 * The one-time Notice of a project's dispatch grants changing (#1437): each teammate is told
 * who may now dispatch to whom, and who no longer may, once.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";
const LABEL = "The project's dispatch grants changed";

const QUIET: DispatchGrants = {
  grants: [],
  all_locked: null,
  locked_pairs: [],
  locked_by: null,
  changed: null,
};

const CHANGED: DispatchGrants = {
  ...QUIET,
  changed: {
    added: ["steward -> devops", "qa -> devops"],
    removed: ["steward -> billing"],
    now: ["steward -> devops", "qa -> devops"],
  },
};

function core(state: DispatchGrants) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "dispatch_grants") return state;
    if (cmd === "acknowledge_dispatch_grants") return null;
    return null;
  });
  return asked;
}

describe("the project's dispatch grants Notice", () => {
  it("names who may now dispatch to whom, and who no longer may", async () => {
    core(CHANGED);
    render(<ProjectDispatchNotice plane={PLANE} onReview={() => {}} />);

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).toHaveTextContent("Added: steward to devops, qa to devops.");
    expect(notice).toHaveTextContent("Taken away: steward to billing.");
    // What a pull added covers nothing here until someone at this machine allows it.
    expect(notice).toHaveTextContent(
      "What was added covers no chat on this machine until you allow it here.",
    );
  });

  it("allows one pair alone, and the others still wait", async () => {
    const asked = core(CHANGED);
    render(<ProjectDispatchNotice plane={PLANE} onReview={() => {}} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Allow steward to devops" }));

    const acknowledged = asked.filter((one) => one.cmd === "acknowledge_dispatch_grants");
    expect(acknowledged.map((one) => one.args)).toMatchObject([
      { plane: PLANE, shown: ["steward -> devops"] },
    ]);
  });

  it("only says so when grants were taken away, with nothing to allow", async () => {
    const asked = core({
      ...QUIET,
      changed: { added: [], removed: ["steward -> billing"], now: [] },
    });
    render(<ProjectDispatchNotice plane={PLANE} onReview={() => {}} />);

    const notice = await screen.findByRole("status", { name: LABEL });
    expect(notice).not.toHaveTextContent("until you allow it here");
    expect(screen.queryByRole("button", { name: /^Allow/ })).toBeNull();
    await userEvent.setup().click(screen.getByRole("button", { name: "Got it" }));
    await waitFor(() =>
      expect(asked.map((one) => one.cmd)).toContain("acknowledge_dispatch_grants"),
    );
  });

  it("says nothing when they did not change", async () => {
    const asked = core(QUIET);
    render(<ProjectDispatchNotice plane={PLANE} onReview={() => {}} />);

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("dispatch_grants"));
    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();
  });

  it("allows every pair it showed on Allow all, and is gone", async () => {
    const asked = core(CHANGED);
    render(<ProjectDispatchNotice plane={PLANE} onReview={() => {}} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Allow all 2" }));

    await waitFor(() =>
      expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument(),
    );
    const acknowledged = asked.filter((one) => one.cmd === "acknowledge_dispatch_grants");
    expect(acknowledged).toHaveLength(1);
    expect(acknowledged[0]?.args).toMatchObject({
      plane: PLANE,
      shown: ["steward -> devops", "qa -> devops"],
    });
  });

  it("opens Settings to review them, and records nothing", async () => {
    const asked = core(CHANGED);
    const onReview = vi.fn();
    render(<ProjectDispatchNotice plane={PLANE} onReview={onReview} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Review the grants" }));

    expect(onReview).toHaveBeenCalledOnce();
    expect(asked.map((one) => one.cmd)).not.toContain("acknowledge_dispatch_grants");
  });
});
