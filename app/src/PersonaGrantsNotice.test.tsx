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
    core({ from: "steward 3", persona: "devops", locked: null });
    render(<PersonaGrantsNotice plane={PLANE} session={7} running={false} onRestart={() => {}} />);

    const notice = await screen.findByRole("status", { name: "Persona's hosts held" });
    expect(notice).toHaveTextContent(
      "This chat was opened by steward 3 as devops. It can't reach devops's hosts until you allow it.",
    );
  });

  it("offers no Allow where policy forbids a persona's own hosts, and says who set it (#1343)", async () => {
    const locked =
      "Policy forbids a persona's own hosts. Locked by policy, set by Platform team in /etc/purlis/policy.json.";
    const asked = core({ from: "steward 3", persona: "devops", locked });
    render(<PersonaGrantsNotice plane={PLANE} session={7} running={false} onRestart={() => {}} />);

    const notice = await screen.findByRole("status", { name: "Persona's hosts held" });
    expect(notice).toHaveTextContent(`This chat can't reach devops's hosts. ${locked}`);
    expect(screen.queryByRole("button", { name: "Allow" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep" })).toBeInTheDocument();
    expect(asked.map((one) => one.cmd)).not.toContain("allow_persona_grants");
  });

  it("says nothing for a chat that holds its own", async () => {
    const asked = core(null);
    render(<PersonaGrantsNotice plane={PLANE} session={7} running={false} onRestart={() => {}} />);

    await waitFor(() => expect(asked.map((one) => one.cmd)).toContain("persona_grants_held"));
    expect(screen.queryByRole("status", { name: "Persona's hosts held" })).not.toBeInTheDocument();
  });

  it("says when a Resume, not a handoff, started it", async () => {
    core({ from: null, persona: "devops", locked: null });
    render(<PersonaGrantsNotice plane={PLANE} session={7} running={false} onRestart={() => {}} />);

    expect(await screen.findByRole("status", { name: "Persona's hosts held" })).toHaveTextContent(
      "This chat was resumed from a session record as devops.",
    );
  });

  it("restarts it on Restart now only once its turn has ended", async () => {
    core({ from: "steward 3", persona: "devops", locked: null });
    const onRestart = vi.fn();
    const { rerender } = render(
      <PersonaGrantsNotice plane={PLANE} session={7} running={true} onRestart={onRestart} />,
    );
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Allow" }));
    await user.click(await screen.findByRole("button", { name: "Restart now" }));

    expect(onRestart).not.toHaveBeenCalled();
    expect(screen.getByRole("status", { name: "Persona's hosts allowed" })).toHaveTextContent(
      "when this turn ends",
    );

    rerender(
      <PersonaGrantsNotice plane={PLANE} session={7} running={false} onRestart={onRestart} />,
    );

    await waitFor(() => expect(onRestart).toHaveBeenCalledOnce());
  });

  it("allows them on Allow, from the chat's next start", async () => {
    const asked = core({ from: "steward 3", persona: "devops", locked: null });
    render(<PersonaGrantsNotice plane={PLANE} session={7} running={false} onRestart={() => {}} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Allow" }));

    expect(
      await screen.findByRole("status", { name: "Persona's hosts allowed" }),
    ).toHaveTextContent("from its next start");
    expect(asked.find((one) => one.cmd === "allow_persona_grants")?.args).toMatchObject({
      plane: PLANE,
      session: 7,
    });
  });

  it("keeps holding on Keep, and allows nothing", async () => {
    const asked = core({ from: "steward 3", persona: "devops", locked: null });
    render(<PersonaGrantsNotice plane={PLANE} session={7} running={false} onRestart={() => {}} />);

    await userEvent.setup().click(await screen.findByRole("button", { name: "Keep" }));

    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "Persona's hosts held" }),
      ).not.toBeInTheDocument(),
    );
    expect(asked.map((one) => one.cmd)).not.toContain("allow_persona_grants");
  });
});
