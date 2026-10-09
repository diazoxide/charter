import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { PersonaHostsNotice } from "./PersonaHostsNotice";
import { inTheWindow } from "./personaHostsAllow";
import type { PersonaHosts, SandboxState } from "./bindings";

/**
 * A persona's committed hosts wait for the person on each machine (#1362, D-1362-7): the
 * Notice shows the list, and Allow sends back the digest of exactly what it showed.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const DEVOPS: PersonaHosts = {
  persona: "devops",
  hosts: ["10.100.39.145:6443", "*.internal.example"],
  reached: ["10.100.39.145:6443", "*.internal.example"],
  digest: "d1",
  allowed: false,
  default: false,
  waiting: false,
};

const QUIET: SandboxState = {
  on: true,
  offer: false,
  said: null,
  never: [],
  hosts_changed: null,
  presets_changed: null,
  presets: [],
  persona_hosts: [],
  besides: { project_hosts: 0, your_hosts: 0, folders: 0 },
  policy: null,
};

const WAITING: SandboxState = { ...QUIET, persona_hosts: [DEVOPS] };
const ALLOWED: SandboxState = { ...QUIET, persona_hosts: [{ ...DEVOPS, allowed: true }] };

const NAME = "devops's hosts wait for you";

function core(state: SandboxState, allowed: SandboxState | string = ALLOWED) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "sandbox_state") return state;
    if (cmd === "allow_persona_hosts") {
      if (typeof allowed === "string") throw allowed;
      return allowed;
    }
    return null;
  });
  return asked;
}

describe("a persona's hosts Notice", () => {
  it("shows the hosts a chat as the persona would reach, and that none reach it until allowed", async () => {
    core(WAITING);
    render(<PersonaHostsNotice plane={PLANE} />);

    const notice = await screen.findByRole("status", { name: NAME });
    expect(notice).toHaveTextContent(
      "The project's settings let chats as devops reach 10.100.39.145:6443 and *.internal.example.",
    );
    expect(notice).toHaveTextContent("no chat here reaches these hosts until you allow them");
    expect(notice).toHaveTextContent(
      "Chats started after you allow them reach them; a chat already running takes them when it restarts.",
    );
  });

  it("says the default persona's hosts reach every chat that names no persona, and allows that", async () => {
    const asked = core({ ...QUIET, persona_hosts: [{ ...DEVOPS, default: true }] });
    render(<PersonaHostsNotice plane={PLANE} />);

    const notice = await screen.findByRole("status", { name: NAME });
    expect(notice).toHaveTextContent(
      "let chats as devops, and every chat that names no persona (devops is this project's default persona), reach",
    );
    await userEvent.setup().click(
      await screen.findByRole("button", {
        name: "Allow for devops chats and chats that name no persona",
      }),
    );
    await waitFor(() =>
      expect(asked).toContainEqual({
        cmd: "allow_persona_hosts",
        args: { plane: PLANE, persona: "devops", digest: "d1" },
      }),
    );
  });

  it("says an Allow whose list changed since reaches nothing until it is allowed again", async () => {
    core({ ...QUIET, persona_hosts: [{ ...DEVOPS, waiting: true }] });
    render(<PersonaHostsNotice plane={PLANE} />);

    const notice = await screen.findByRole("status", { name: NAME });
    expect(notice).toHaveTextContent(
      "You allowed devops's hosts on this machine, and they have changed since, so no chat reaches any of them until you allow them again.",
    );
  });

  it("offers no Allow on a link, whose client has no window-only call", async () => {
    core(WAITING);
    render(<PersonaHostsNotice plane={PLANE} canAllow={false} />);

    const notice = await screen.findByRole("status", { name: NAME });
    expect(notice).toHaveTextContent("Allow them from purlis's own window on this machine.");
    expect(screen.queryByRole("button", { name: /Allow/ })).toBeNull();
    expect(inTheWindow({ allowPersonaHosts: () => null }, "allowPersonaHosts")).toBe(true);
    expect(inTheWindow({}, "allowPersonaHosts")).toBe(false);
  });

  it("sends back the digest of the list it showed, and is gone once allowed", async () => {
    const asked = core(WAITING);
    render(<PersonaHostsNotice plane={PLANE} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Allow for devops chats" }));

    await waitFor(() =>
      expect(asked).toContainEqual({
        cmd: "allow_persona_hosts",
        args: { plane: PLANE, persona: "devops", digest: "d1" },
      }),
    );
    await waitFor(() => expect(screen.queryByRole("status", { name: NAME })).toBeNull());
  });

  it("says why when the list changed since it was shown, and stays", async () => {
    core(WAITING, "purlis allowed nothing: devops's hosts changed after they were shown.");
    render(<PersonaHostsNotice plane={PLANE} />);

    await userEvent
      .setup()
      .click(await screen.findByRole("button", { name: "Allow for devops chats" }));

    const notice = await screen.findByRole("status", { name: NAME });
    await waitFor(() => expect(notice).toHaveTextContent("changed after they were shown"));
  });

  it("asks nothing for hosts already allowed, none a chat would reach, or under a policy lock", async () => {
    const asked = core({
      ...QUIET,
      persona_hosts: [
        { ...DEVOPS, allowed: true },
        { ...DEVOPS, persona: "qa", reached: [] },
      ],
    });
    render(<PersonaHostsNotice plane={PLANE} />);
    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("sandbox_state"));
    expect(screen.queryByRole("status")).toBeNull();

    cleanup();
    clearMocks();
    const locked = core({
      ...WAITING,
      policy: {
        presets: null,
        hosts: null,
        persona_hosts: true,
        personal_hosts: false,
        write_grants: false,
        required: false,
        locked_by: "Locked by policy.",
      } as unknown as SandboxState["policy"],
    });
    render(<PersonaHostsNotice plane={PLANE} />);
    await waitFor(() => expect(locked.map((one) => one.cmd)).toContain("sandbox_state"));
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("hides on Not now and keeps nothing", async () => {
    const asked = core(WAITING);
    render(<PersonaHostsNotice plane={PLANE} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Not now" }));

    await waitFor(() => expect(screen.queryByRole("status", { name: NAME })).toBeNull());
    expect(asked.map((one) => one.cmd)).not.toContain("allow_persona_hosts");
  });
});
