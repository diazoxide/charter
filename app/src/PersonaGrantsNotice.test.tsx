import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { PersonaGrantsNotice } from "./PersonaGrantsNotice";
import type { GrantsHeld } from "./bindings";

/**
 * A handed-off chat holding the asking chat's persona grants (#1362, D-1362-5): its tab says
 * so, and only the person's Allow gives it its own persona's hosts.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function core(held: GrantsHeld | null) {
  const asked: { cmd: string; args: unknown }[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args });
    if (cmd === "persona_grants_held") return held;
    if (cmd === "allow_persona_grants") return true;
    return null;
  });
  return asked;
}

describe("the persona grants Notice", () => {
  it("says which chat opened it and whose hosts it cannot reach yet", async () => {
    core({ from: "steward 3", persona: "devops", locked: null, waits_here: false });
    render(
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={false}
        owed={false}
        onRestart={() => {}}
      />,
    );

    const notice = await screen.findByRole("status", { name: "Persona's hosts held" });
    expect(notice).toHaveTextContent(
      "This chat was opened by steward 3 as devops. It can't reach devops's hosts until you allow it.",
    );
  });

  it("offers no Allow where policy forbids a persona's own hosts, and says who set it (#1343)", async () => {
    const locked =
      "Policy forbids a persona's own hosts. Locked by policy, set by Platform team in /etc/purlis/policy.json.";
    const asked = core({ from: "steward 3", persona: "devops", locked, waits_here: false });
    render(
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={false}
        owed={false}
        onRestart={() => {}}
      />,
    );

    const notice = await screen.findByRole("status", { name: "Persona's hosts held" });
    expect(notice).toHaveTextContent(`This chat can't reach devops's hosts. ${locked}`);
    expect(screen.queryByRole("button", { name: "Allow" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep" })).toBeInTheDocument();
    expect(asked.map((one) => one.cmd)).not.toContain("allow_persona_grants");
  });

  it("says nothing for a chat that holds its own", async () => {
    const asked = core(null);
    render(
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={false}
        owed={false}
        onRestart={() => {}}
      />,
    );

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("persona_grants_held"));
    expect(screen.queryByRole("status", { name: "Persona's hosts held" })).not.toBeInTheDocument();
  });

  it("says when a Resume, not a handoff, started it", async () => {
    core({ from: null, persona: "devops", locked: null, waits_here: false });
    render(
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={false}
        owed={false}
        onRestart={() => {}}
      />,
    );

    expect(await screen.findByRole("status", { name: "Persona's hosts held" })).toHaveTextContent(
      "This chat was resumed from a session record as devops.",
    );
  });

  it("asks for its restart on Restart now, and says a chat mid-turn waits for the turn", async () => {
    // The wait is the window's: one restart, whoever asked for it (`RestartChat.window.test`).
    core({ from: "steward 3", persona: "devops", locked: null, waits_here: false });
    const onRestart = vi.fn();
    const notice = (owed: boolean) => (
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={true}
        owed={owed}
        onRestart={onRestart}
      />
    );
    const { rerender } = render(notice(false));
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Allow" }));
    const allowed = screen.getByRole("status", { name: "Persona's hosts allowed" });
    expect(allowed).not.toHaveTextContent("when this turn ends");
    await user.click(await screen.findByRole("button", { name: "Restart now" }));
    expect(onRestart).toHaveBeenCalledOnce();

    // The window owes it the restart from here, and the Notice says when it happens.
    rerender(notice(true));
    expect(allowed).toHaveTextContent("when this turn ends");
  });

  it("asks again on Restart now after a restart that was refused", async () => {
    // S8: the Notice once remembered that it had asked, and its button went dead for good.
    core({ from: "steward 3", persona: "devops", locked: null, waits_here: false });
    const onRestart = vi.fn();
    const notice = (owed: boolean) => (
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={true}
        owed={owed}
        onRestart={onRestart}
      />
    );
    const { rerender } = render(notice(false));
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Allow" }));
    await user.click(await screen.findByRole("button", { name: "Restart now" }));
    rerender(notice(true));

    // Refused: the window owes it nothing any more, and says why on the chat's pane.
    rerender(notice(false));
    expect(screen.getByRole("status", { name: "Persona's hosts allowed" })).not.toHaveTextContent(
      "when this turn ends",
    );
    await user.click(screen.getByRole("button", { name: "Restart now" }));

    expect(onRestart).toHaveBeenCalledTimes(2);
  });

  it("allows them on Allow, from the chat's next start", async () => {
    const asked = core({ from: "steward 3", persona: "devops", locked: null, waits_here: false });
    render(
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={false}
        owed={false}
        onRestart={() => {}}
      />,
    );

    await userEvent.setup().click(await screen.findByRole("button", { name: "Allow" }));

    expect(
      await screen.findByRole("status", { name: "Persona's hosts allowed" }),
    ).toHaveTextContent("from its next start");
    expect(asked.find((one) => one.cmd === "allow_persona_grants")?.args).toMatchObject({
      plane: PLANE,
      session: 7,
    });
  });

  it("says the persona's hosts also wait for this machine's Allow, before and after Allow (#1362)", async () => {
    core({ from: "steward 3", persona: "devops", locked: null, waits_here: true });
    render(
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={false}
        owed={false}
        onRestart={() => {}}
      />,
    );

    expect(await screen.findByRole("status", { name: "Persona's hosts held" })).toHaveTextContent(
      "devops's hosts also wait for your Allow on this machine, on the project's notice or in Settings › Sandbox",
    );
    await userEvent.setup().click(await screen.findByRole("button", { name: "Allow" }));
    const after = await screen.findByRole("status", { name: "Persona's hosts allowed" });
    expect(after).toHaveTextContent("Allowed for this chat.");
    expect(after).toHaveTextContent(
      "until then this chat reaches none of them, even after it restarts",
    );
    expect(after).not.toHaveTextContent("from its next start");
  });

  it("keeps holding on Keep, and allows nothing", async () => {
    const asked = core({ from: "steward 3", persona: "devops", locked: null, waits_here: false });
    render(
      <PersonaGrantsNotice
        plane={PLANE}
        session={7}
        running={false}
        owed={false}
        onRestart={() => {}}
      />,
    );

    await userEvent.setup().click(await screen.findByRole("button", { name: "Keep" }));

    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "Persona's hosts held" }),
      ).not.toBeInTheDocument(),
    );
    expect(asked.map((one) => one.cmd)).not.toContain("allow_persona_grants");
  });
});
