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
  });

  it("says nothing when they did not change", async () => {
    const asked = core(QUIET);
    render(<ProjectDispatchNotice plane={PLANE} onReview={() => {}} />);

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("dispatch_grants"));
    expect(screen.queryByRole("status", { name: LABEL })).not.toBeInTheDocument();
  });

  it("records the list it showed once read, and is gone", async () => {
    const asked = core(CHANGED);
    render(<ProjectDispatchNotice plane={PLANE} onReview={() => {}} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Got it" }));

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
