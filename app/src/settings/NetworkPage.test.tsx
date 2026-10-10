import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { BlockedLately, ChatNetwork, SandboxGrant, SandboxNetwork } from "../bindings";
import { ChatNetworkTab } from "../ChatNetworkTab";
import { networkGroup } from "./GrantedList";
import type { LiveSetting } from "./groups";

/**
 * **Settings › Sandbox › Network and a chat's Network view** (#1662, spec #1661): Open hosts host
 * by host, Allowed with Remove, Blocked lately with Allow, and what one chat reaches and was
 * refused — all as the core says them, in the four words and none of the retired ones.
 */

const PLANE = "/home/dev/plane";

const NETWORK: SandboxNetwork = {
  on: true,
  open: [
    { title: "AI providers", hosts: ["api.anthropic.com", "api.openai.com"] },
    { title: "This project's hosts", hosts: ["registry.internal"] },
  ],
  blocked: [
    {
      target: "db.example.com:5432",
      looked_up: null,
      said: "a connection to an internet host this project does not allow",
      chat: "fix the build",
      at: 1_790_000_000,
      times: 3,
      reached: false,
      levels: ["you", "project"],
    },
    {
      target: "registry.internal:443",
      looked_up: null,
      said: "a connection to an internet host this project does not allow",
      chat: "claude 2",
      at: 1_789_000_000,
      times: 1,
      reached: true,
      levels: [],
    },
    {
      target: null,
      looked_up: "db.prod.example.com",
      said: "a lookup of an internet host by a program that does not go through the sandbox's proxy",
      chat: "claude 2",
      at: 1_788_000_000,
      times: 1,
      reached: false,
      levels: [],
    },
  ],
};

const ALLOWED: SandboxGrant = {
  id: "you\u001fhost\u001fapi.example.com:443",
  what: "host",
  target: "api.example.com:443",
  persona: null,
  level: "you",
  by: null,
  at: 1_790_000_000,
  chat: null,
  locked: null,
  waiting: null,
  for_no_persona: false,
};

afterEach(() => {
  cleanup();
  clearMocks();
});

/** The Network page's row `id`, drawn on its own. */
function Row({ id }: { id: string }) {
  const setting = networkGroup(PLANE, "charter.toml").settings.find(
    (one) => one.id === `project.sandbox.network.${id}`,
  ) as LiveSetting;
  const { control } = setting.useControl();
  return <>{control({ id: `r-${id}`, labelledBy: `r-${id}-label` })}</>;
}

function core(network: SandboxNetwork = NETWORK) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "sandbox_network") return network;
    if (cmd === "sandbox_grants") return [ALLOWED];
    if (cmd === "allow_blocked_host")
      return {
        said: "Allowed db.example.com:5432 for me on this machine. A chat that is running reaches it from its next start.",
      };
    return null;
  });
  return asked;
}

describe("the Network page", () => {
  it("is Open hosts, Allowed and Blocked lately, in that order, then the folders", () => {
    const group = networkGroup(PLANE, "charter.toml");
    expect(group.label).toBe("Network");
    expect(group.settings.map((one) => one.label)).toEqual([
      "Open hosts",
      "Allowed",
      "Blocked lately",
      "Folders a block's Allow may name",
      "Who may dispatch to whom",
    ]);
  });

  it("names every host of each preset in force, then the project's own", async () => {
    core();
    render(<Row id="open" />);
    const ai = await screen.findByRole("list", { name: "AI providers" });
    expect(
      within(ai)
        .getAllByRole("listitem")
        .map((one) => one.textContent),
    ).toEqual(["api.anthropic.com", "api.openai.com"]);
    expect(screen.getByRole("list", { name: "This project's hosts" })).toHaveTextContent(
      "registry.internal",
    );
  });

  it("says a project without the sandbox reaches any host", async () => {
    core({ on: false, open: [], blocked: [] });
    render(<Row id="open" />);
    expect(
      await screen.findByText("Chats here run without a sandbox, so they can reach any host."),
    ).toBeVisible();
  });

  it("lists what was blocked lately with its host, chat and time, and Allow for a host", async () => {
    const asked = core();
    render(<Row id="blocked" />);
    const list = await screen.findByRole("list", { name: "Blocked lately" });
    const [db, reached, lookup] = within(list).getAllByRole("listitem");
    expect(db).toHaveTextContent("db.example.com:5432");
    expect(db).toHaveTextContent(
      "A connection to an internet host this project does not allow · fix the build · 3 times, last",
    );
    // Reached since, or nothing a host's Allow would mend: no Allow.
    expect(reached).toHaveTextContent("Reached now.");
    expect(within(reached).queryByRole("button")).toBeNull();
    expect(within(lookup).queryByRole("button")).toBeNull();
    // A refused lookup's host is a program's printed words: shown, never offered (#1663).
    expect(lookup).toHaveTextContent("db.prod.example.com");
    expect(lookup).toHaveTextContent("Looked up by a program, not offered.");

    await userEvent.click(within(db).getByRole("button", { name: "Allow: db.example.com:5432" }));
    await waitFor(() =>
      expect(asked).toContainEqual({
        cmd: "allow_blocked_host",
        args: { plane: PLANE, host: "db.example.com:5432", level: "you" },
      }),
    );
    expect(
      await screen.findByText(/Allowed db\.example\.com:5432 for me on this machine/),
    ).toBeVisible();
    expect(
      within(db).getByRole("button", {
        name: "Allow for everyone in the project: db.example.com:5432",
      }),
    ).toBeVisible();
  });

  it("never draws Allow on a looked-up host, whatever scopes the row carries", async () => {
    core({
      ...NETWORK,
      blocked: [
        {
          ...NETWORK.blocked[2],
          target: "db.prod.example.com",
          levels: ["you", "project"],
        },
      ],
    });
    render(<Row id="blocked" />);
    const list = await screen.findByRole("list", { name: "Blocked lately" });
    expect(list).toHaveTextContent("Looked up by a program, not offered.");
    expect(within(list).queryByRole("button")).toBeNull();
  });

  it("says when nothing was blocked lately", async () => {
    core({ ...NETWORK, blocked: [] });
    render(<Row id="blocked" />);
    expect(await screen.findByText("Nothing was blocked here in the last 30 days.")).toBeVisible();
  });

  it("lists Allowed with its scope, who and when, and Remove", async () => {
    core();
    render(<Row id="allowed" />);
    const list = await screen.findByRole("list", { name: "Allowed" });
    const row = within(list).getByRole("listitem");
    expect(row).toHaveTextContent(
      "Reach api.example.com:443 · This project on this machine · allowed by you",
    );
    expect(
      within(row).getByRole("button", { name: "Remove reaching api.example.com:443" }),
    ).toBeVisible();
  });

  it("says none of the retired words: egress, grant, allowlist", async () => {
    core();
    const group = networkGroup(PLANE, "charter.toml");
    const { container } = render(
      <>
        {["open", "allowed", "blocked", "folders"].map((id) => (
          <Row key={id} id={id} />
        ))}
      </>,
    );
    await screen.findByRole("list", { name: "Blocked lately" });
    await screen.findByRole("list", { name: "Allowed" });
    const said = [
      container.textContent ?? "",
      group.label,
      group.help,
      ...group.settings.slice(0, 4).flatMap((one) => [one.label, one.help]),
      ...[...container.querySelectorAll("[aria-label]")].map(
        (one) => one.getAttribute("aria-label") ?? "",
      ),
    ].join("\n");
    expect(said).not.toMatch(/\begress\b|\bgrant(s|ed|ing)?\b|\ballowlist\b/i);
  });
});

const REFUSED: BlockedLately = {
  ...NETWORK.blocked[0],
  chat: null,
};

describe("a chat's Network view", () => {
  function chatCore(network: ChatNetwork) {
    mockIPC((cmd) => (cmd === "chat_network" ? network : null));
  }

  it("says a chat started without the sandbox can reach anything, and nothing more", async () => {
    chatCore({ open: true, sandboxed: false, reach: [], refused: [] });
    render(<ChatNetworkTab plane={PLANE} session={3} />);
    expect(await screen.findByText("Not sandboxed: can reach anything.")).toBeVisible();
    expect(screen.queryByText("Refused")).toBeNull();
  });

  it("lists what it reaches by Open, Persona and Allowed hosts, and what it was refused", async () => {
    chatCore({
      open: true,
      sandboxed: true,
      reach: [
        { host: "api.anthropic.com", by: "open" },
        { host: "db.internal:5432", by: "persona" },
        { host: "extra.example:443", by: "allowed" },
      ],
      refused: [REFUSED, { ...NETWORK.blocked[2], chat: null }],
    });
    render(<ChatNetworkTab plane={PLANE} session={3} />);
    expect(await screen.findByRole("list", { name: "Open hosts" })).toHaveTextContent(
      "api.anthropic.com",
    );
    expect(screen.getByRole("list", { name: "Persona hosts" })).toHaveTextContent(
      "db.internal:5432",
    );
    expect(screen.getByRole("list", { name: "Allowed hosts" })).toHaveTextContent(
      "extra.example:443",
    );
    const refused = screen.getByRole("list", { name: "Refused" });
    expect(refused).toHaveTextContent("db.example.com:5432");
    expect(
      within(refused).getByRole("button", { name: "Allow: db.example.com:5432" }),
    ).toBeVisible();
    const lookup = within(refused).getAllByRole("listitem")[1];
    expect(lookup).toHaveTextContent("db.prod.example.com");
    expect(lookup).toHaveTextContent("Looked up by a program, not offered.");
    expect(within(lookup).queryByRole("button")).toBeNull();
  });

  it("says a chat that is not open has nothing to show", async () => {
    chatCore({ open: false, sandboxed: false, reach: [], refused: [] });
    render(<ChatNetworkTab plane={PLANE} session={9} />);
    expect(await screen.findByText("This chat is not open")).toBeVisible();
  });
});
