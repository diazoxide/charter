import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, renderHook, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { SandboxGrant } from "../bindings";
import { grantedGroup, grantSaid } from "./GrantedList";
import type { LiveSetting } from "./groups";
import { useSandboxCommands } from "../sandboxAsked";

/**
 * **Settings › Sandbox › Granted** (#1348): every grant at every level, who granted it and when,
 * each with Revoke, which the core takes out of every later start and audits.
 */

const PLANE = "/home/dev/plane";

const CHAT: SandboxGrant = {
  id: "chat\u001fc1\u001fhost\u001fapi.example.com",
  what: "host",
  target: "api.example.com",
  persona: null,
  level: "chat",
  by: null,
  at: 1_790_000_000,
  chat: "claude 4",
  locked: null,
  waiting: null,
  for_no_persona: false,
};
const MINE: SandboxGrant = {
  id: "you\u001fwrite\u001f/opt/cache",
  what: "write",
  target: "/opt/cache",
  persona: null,
  level: "you",
  by: null,
  at: null,
  chat: null,
  locked: null,
  waiting: null,
  for_no_persona: false,
};
const PROJECT: SandboxGrant = {
  id: "project\u001fhost\u001f10.0.0.5:6443",
  what: "host",
  target: "10.0.0.5:6443",
  persona: null,
  level: "project",
  by: "Dana",
  at: 1_790_000_000,
  chat: null,
  locked:
    "10.0.0.5:6443 is not a host policy allows. Locked by policy, set by IT in /etc/purlis/policy.json.",
  waiting: null,
  for_no_persona: false,
};

afterEach(() => {
  cleanup();
  clearMocks();
});

function Setting({ at }: { at: number }) {
  const setting = grantedGroup(PLANE, "charter.toml").settings[at] as LiveSetting;
  const { control } = setting.useControl();
  return <>{control({ id: `g${at}`, labelledBy: `g${at}-label` })}</>;
}

const Granted = () => <Setting at={0} />;

describe("the Granted list", () => {
  it("lists every level with who granted it, and Revoke takes one out", async () => {
    const asked: Record<string, unknown>[] = [];
    let held = [CHAT, MINE, PROJECT];
    mockIPC((cmd, args) => {
      if (cmd === "sandbox_grants") return held;
      if (cmd === "revoke_sandbox_grant") {
        asked.push(args as Record<string, unknown>);
        held = held.filter((one) => one.id !== (args as { id: string }).id);
        return held;
      }
      return null;
    });
    render(<Granted />);

    const list = await screen.findByRole("list", { name: "Granted" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    expect(rows[0]).toHaveTextContent("Reach api.example.com · One chat · granted by you");
    expect(rows[0]).toHaveTextContent("from claude 4");
    expect(rows[1]).toHaveTextContent(
      "Write /opt/cache and everything in it · Me on this machine · granted by you",
    );
    expect(rows[2]).toHaveTextContent(
      "Reach 10.0.0.5:6443 · Everyone in this project · committed by Dana",
    );
    // Locked by policy: said, and still yours to take away (#1431); a project host's Revoke
    // edits the committed file.
    expect(rows[2]).toHaveTextContent(
      "Not in force. 10.0.0.5:6443 is not a host policy allows. Locked by policy, set by IT in /etc/purlis/policy.json.",
    );
    expect(rows[2]).toHaveTextContent("Revoking it edits the committed charter.toml.");
    expect(
      within(rows[2]).getByRole("button", { name: "Revoke reaching 10.0.0.5:6443" }),
    ).toBeVisible();

    // What reads the chats left on an older sandbox hears that a sandbox command returned
    // (#1428): the revoked folder is kept where no watcher reports a write.
    const heard = renderHook(() => useSandboxCommands());
    const before = heard.result.current;
    await userEvent.click(screen.getByRole("button", { name: "Revoke writing /opt/cache" }));
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(2));
    expect(asked).toEqual([{ plane: PLANE, id: MINE.id }]);
    await waitFor(() => expect(heard.result.current).toBeGreaterThan(before));

    // The locked one goes the same way.
    await userEvent.click(screen.getByRole("button", { name: "Revoke reaching 10.0.0.5:6443" }));
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(1));
    expect(asked).toEqual([
      { plane: PLANE, id: MINE.id },
      { plane: PLANE, id: PROJECT.id },
    ]);
  });

  it("says when nothing is granted", async () => {
    mockIPC((cmd) => (cmd === "sandbox_grants" ? [] : null));
    render(<Granted />);
    expect(
      await screen.findByText("Nothing is granted past this project's sandbox."),
    ).toBeInTheDocument();
  });

  it("says a refused revoke through a Notice", async () => {
    mockIPC((cmd) => {
      if (cmd === "sandbox_grants") return [MINE];
      if (cmd === "revoke_sandbox_grant")
        throw new Error("purlis did not revoke it: that grant is no longer there.");
      return null;
    });
    render(<Granted />);
    await userEvent.click(await screen.findByRole("button", { name: "Revoke writing /opt/cache" }));
    expect(await screen.findByRole("status")).toHaveTextContent("that grant is no longer there");
  });

  it("lists the folders chats may be granted, with Add and Remove", async () => {
    const asked: { cmd: string; args: unknown }[] = [];
    let held = ["/opt/tools"];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "grantable_folders") return { folders: held, dropped: [] };
      if (cmd === "list_grantable_folder") {
        held = [...held, (args as { folder: string }).folder];
        return { folders: held, holds: ["/Users/dev/Library/LaunchAgents"] };
      }
      if (cmd === "unlist_grantable_folder") {
        held = held.filter((one) => one !== (args as { folder: string }).folder);
        return held;
      }
      return null;
    });
    render(<Setting at={1} />);
    const list = await screen.findByRole("list", { name: "Folders chats may be granted" });
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(1));
    await userEvent.type(screen.getByRole("textbox"), "/opt/more");
    await userEvent.click(screen.getByRole("button", { name: "Add folder" }));
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(2));
    // Listed, and warned of what it holds that other programs load code from.
    expect(await screen.findByRole("status")).toHaveTextContent(
      "holds folders other programs load code from",
    );
    await userEvent.click(screen.getByRole("button", { name: "Remove /opt/tools" }));
    await waitFor(() => expect(within(list).getAllByRole("listitem")).toHaveLength(1));
    expect(asked.map((one) => one.cmd)).toContain("list_grantable_folder");
  });

  it("says a project host not committed yet as that, and that revoking edits the file", async () => {
    mockIPC((cmd) => (cmd === "sandbox_grants" ? [{ ...PROJECT, by: null, locked: null }] : null));
    render(<Granted />);
    const row = (await screen.findAllByRole("listitem"))[0];
    expect(row).toHaveTextContent("Everyone in this project · not committed yet");
    expect(row).toHaveTextContent("Revoking it edits the committed charter.toml.");
  });

  it("says a listed folder the core dropped because it leads elsewhere now", async () => {
    mockIPC((cmd) =>
      cmd === "grantable_folders" ? { folders: [], dropped: ["/Users/dev/tools"] } : null,
    );
    render(<Setting at={1} />);
    expect(await screen.findByRole("status")).toHaveTextContent(
      "off this list: it now leads somewhere else",
    );
  });

  it("lists a vault you let a persona's chats use, and revokes it by both names (#1430)", async () => {
    const VAULT: SandboxGrant = {
      id: "you\u001fvault\u001fdevops\u001fsteward",
      what: "vault",
      target: "devops",
      persona: "steward",
      level: "you",
      by: null,
      at: null,
      chat: "steward 1",
      locked: null,
      waiting: null,
      for_no_persona: false,
    };
    expect(grantSaid(VAULT)).toBe(
      "Use vault devops as steward · Me on this machine · granted by you, from steward 1",
    );
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "sandbox_grants") return [VAULT];
      if (cmd === "revoke_sandbox_grant") return [];
      return null;
    });
    render(<Granted />);
    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Revoke using vault devops as steward" }));
    await waitFor(() =>
      expect(asked).toContainEqual({
        cmd: "revoke_sandbox_grant",
        args: { plane: PLANE, id: VAULT.id },
      }),
    );
    expect(
      await screen.findByText("Nothing is granted past this project's sandbox."),
    ).toBeVisible();
  });

  it("lists a persona's hosts you allowed, credited to the persona, and revokes them (#1362)", async () => {
    const DEVOPS: SandboxGrant = {
      id: "you\u001fpersona-hosts\u001fdevops",
      what: "persona-hosts",
      target: "10.100.39.145:6443, *.internal.example",
      persona: "devops",
      level: "you",
      by: null,
      at: null,
      chat: null,
      locked: null,
      waiting: null,
      for_no_persona: false,
    };
    expect(grantSaid({ ...DEVOPS, for_no_persona: true })).toBe(
      "Chats as devops and every chat that names no persona reach 10.100.39.145:6443, *.internal.example · Me on this machine · granted by you",
    );
    expect(grantSaid(DEVOPS)).toBe(
      "Chats as devops reach 10.100.39.145:6443, *.internal.example · Me on this machine · granted by you",
    );
    const asked: { cmd: string; args: unknown }[] = [];
    mockIPC((cmd, args) => {
      asked.push({ cmd, args });
      if (cmd === "sandbox_grants") return [DEVOPS];
      if (cmd === "revoke_sandbox_grant") return [];
      return null;
    });
    render(<Granted />);
    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Revoke devops's hosts" }));
    await waitFor(() =>
      expect(asked).toContainEqual({
        cmd: "revoke_sandbox_grant",
        args: { plane: PLANE, id: DEVOPS.id },
      }),
    );
  });

  it("lists an Allow whose list changed as not in force, with Revoke (#1362)", async () => {
    const WAITING: SandboxGrant = {
      id: "you\u001fpersona-hosts\u001fdevops",
      what: "persona-hosts",
      target: "10.100.39.145:6443",
      persona: "devops",
      level: "you",
      by: null,
      at: null,
      chat: null,
      locked: null,
      waiting:
        "waiting: the list changed since you allowed it, so it reaches nothing until you allow it again",
      for_no_persona: false,
    };
    mockIPC((cmd) => (cmd === "sandbox_grants" ? [WAITING] : null));
    render(<Granted />);
    expect(
      await screen.findByText(/Not in force: waiting: the list changed since you allowed it/),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Revoke devops's hosts" })).toBeVisible();
  });

  it("names a grant with no time as it is", () => {
    expect(grantSaid(MINE)).toBe(
      "Write /opt/cache and everything in it · Me on this machine · granted by you",
    );
  });
});
