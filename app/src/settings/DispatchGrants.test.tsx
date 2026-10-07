import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { DispatchGrant, DispatchGrants } from "../bindings";
import { DispatchGrantsList, dispatchGrantSaid } from "./DispatchGrants";
import { grantedGroup } from "./GrantedList";
import type { LiveSetting } from "./groups";

/**
 * **The dispatch grants in Settings** (#1437): each pair, its level, who granted it and when,
 * with Revoke; and what an administrator's policy locks, drawn locked.
 */

const PLANE = "/home/dev/plane";

const CHAT: DispatchGrant = {
  id: "chat\u001fc1\u001fsteward\u001fqa",
  asking: "steward",
  target: "qa",
  level: "chat",
  by: null,
  at: 1_790_000_000,
  chat: "steward 3",
  locked: null,
};
const MINE: DispatchGrant = {
  id: "you\u001fsteward\u001fdevops",
  asking: "steward",
  target: "devops",
  level: "you",
  by: null,
  at: null,
  chat: null,
  locked: null,
};
const PROJECT: DispatchGrant = {
  id: "project\u001fqa\u001fdevops",
  asking: "qa",
  target: "devops",
  level: "project",
  by: "Dana",
  at: 1_790_000_000,
  chat: null,
  locked: null,
};

const state = (over: Partial<DispatchGrants> = {}): DispatchGrants => ({
  grants: [],
  all_locked: null,
  locked_pairs: [],
  locked_by: null,
  changed: null,
  ...over,
});

afterEach(() => {
  cleanup();
  clearMocks();
});

const List = () => (
  <DispatchGrantsList plane={PLANE} file="purlis.toml" ids={{ id: "d", labelledBy: "d-label" }} />
);

describe("the dispatch grants list", () => {
  it("lists every level with who granted it and when, and Revoke takes one out", async () => {
    const asked: Record<string, unknown>[] = [];
    let held = [CHAT, MINE, PROJECT];
    mockIPC((cmd, args) => {
      if (cmd === "dispatch_grants") return state({ grants: held });
      if (cmd === "revoke_dispatch_grant") {
        asked.push(args as Record<string, unknown>);
        held = held.filter((one) => one.id !== (args as { id: string }).id);
        return state({ grants: held });
      }
      return null;
    });
    render(<List />);

    const list = await screen.findByRole("list", { name: "Dispatch grants" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent(
      "steward chats may dispatch to qa · One chat · granted by you",
    );
    expect(rows[0]).toHaveTextContent("from steward 3");
    expect(rows[1]).toHaveTextContent(
      "steward chats may dispatch to devops · Me on this machine · granted by you",
    );
    expect(rows[2]).toHaveTextContent(
      "qa chats may dispatch to devops · Everyone in this project · committed by Dana",
    );
    expect(rows[2]).toHaveTextContent("Revoking it edits the committed purlis.toml.");

    await userEvent.click(
      screen.getByRole("button", { name: "Revoke steward dispatching to devops" }),
    );
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(2));
    expect(asked).toEqual([{ plane: PLANE, id: MINE.id }]);
  });

  it("says a grant as a sentence, with what is not known left out", () => {
    expect(dispatchGrantSaid(MINE)).toBe(
      "steward chats may dispatch to devops · Me on this machine · granted by you",
    );
    expect(dispatchGrantSaid({ ...PROJECT, by: null, at: null })).toBe(
      "qa chats may dispatch to devops · Everyone in this project · not committed yet",
    );
    expect(dispatchGrantSaid({ ...CHAT, asking: null, at: null })).toBe(
      "This chat may dispatch to qa · One chat · granted by you, from steward 3",
    );
  });

  it("draws a grant policy locks as locked, with who locked it and no Revoke", async () => {
    const locked =
      "Policy forbids steward chats dispatching to devops. Locked by policy, set by IT in /etc/purlis/policy.json.";
    mockIPC((cmd) =>
      cmd === "dispatch_grants"
        ? state({
            grants: [{ ...MINE, locked }],
            locked_pairs: [{ asking: "steward", target: "devops" }],
            locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
          })
        : null,
    );
    render(<List />);

    const list = await screen.findByRole("list", { name: "Dispatch grants" });
    const row = within(list).getByRole("listitem");
    expect(row).toHaveTextContent(locked);
    expect(within(row).queryByRole("button")).toBeNull();
    const locks = screen.getByRole("list", { name: "Locked by policy" });
    expect(within(locks).getByRole("listitem")).toHaveTextContent("steward to devops");
    expect(
      screen.getByText("Locked by policy, set by IT in /etc/purlis/policy.json."),
    ).toBeInTheDocument();
  });

  it("says a lock on all dispatch, and who set it", async () => {
    const all =
      "Policy forbids one chat dispatching to another. Locked by policy, set by IT in /etc/purlis/policy.json.";
    mockIPC((cmd) =>
      cmd === "dispatch_grants"
        ? state({
            all_locked: all,
            locked_by: "Locked by policy, set by IT in /etc/purlis/policy.json.",
          })
        : null,
    );
    render(<List />);

    expect(await screen.findByText(all)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Locked by policy" })).toBeNull();
  });

  it("says when no persona may dispatch to another", async () => {
    mockIPC((cmd) => (cmd === "dispatch_grants" ? state() : null));
    render(<List />);
    expect(
      await screen.findByText("No persona's chats may dispatch to another persona yet."),
    ).toBeInTheDocument();
  });

  it("says what the core said when a revoke is refused", async () => {
    mockIPC((cmd) => {
      if (cmd === "dispatch_grants") return state({ grants: [MINE] });
      if (cmd === "revoke_dispatch_grant")
        throw "purlis did not revoke it: that grant is no longer there.";
      return null;
    });
    render(<List />);

    await userEvent.click(
      await screen.findByRole("button", { name: "Revoke steward dispatching to devops" }),
    );
    expect(
      await screen.findByText("purlis did not revoke it: that grant is no longer there."),
    ).toBeInTheDocument();
  });

  it("is on Settings' Granted page today", async () => {
    mockIPC((cmd) => (cmd === "dispatch_grants" ? state({ grants: [MINE] }) : null));
    const setting = grantedGroup(PLANE, "purlis.toml").settings.find(
      (one) => one.id === "project.sandbox.granted.dispatch",
    ) as LiveSetting;
    expect(setting.label).toBe("Who may dispatch to whom");
    function Row() {
      const { control } = setting.useControl();
      return <>{control({ id: "g", labelledBy: "g-label" })}</>;
    }
    render(<Row />);

    const list = await screen.findByRole("list", { name: "Dispatch grants" });
    expect(within(list).getByRole("listitem")).toHaveTextContent(
      "steward chats may dispatch to devops",
    );
  });
});
