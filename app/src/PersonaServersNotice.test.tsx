import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import type { PersonaServerWaiting } from "./bindings";
import { PersonaServersNotice } from "./PersonaServersNotice";

/**
 * **A persona's server that waits for an approval** (#1460): the line `purlis persona
 * approve-mcp` asks about, shown in the persona's view, with Approve where the page is the
 * window's.
 */

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

const GRAFANA: PersonaServerWaiting = {
  server: "grafana",
  line: "run npx -y grafana-mcp@1.2.0 with GRAFANA_TOKEN from vault ops",
  fingerprint: "f00d",
};
const GSC: PersonaServerWaiting = {
  server: "gsc",
  line: "run uvx gsc-mcp==0.3.0 with GOOGLE_SA from vault ops as a file",
  fingerprint: "beef",
};

/** The core: what waits, and what an approval answers (or its refusal). */
function core(waits: PersonaServerWaiting[], refusal?: string) {
  const asked: { cmd: string; args: Record<string, unknown> }[] = [];
  let now = waits;
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    if (cmd === "persona_servers_waiting") return now;
    if (cmd === "approve_persona_server") {
      if (refusal) throw refusal;
      const { server } = args as { server: string };
      now = now.filter((one) => one.server !== server);
      return now;
    }
    return null;
  });
  return asked;
}

describe("a persona's servers that wait for this machine's approval", () => {
  it("shows each server's line with Approve, and approves the line it showed", async () => {
    const asked = core([GRAFANA, GSC]);
    render(<PersonaServersNotice plane={PLANE} persona="ops" canApprove />);

    const notice = await screen.findByRole("status", {
      name: "ops's grafana server waits for you",
    });
    expect(notice).toHaveTextContent(GRAFANA.line);
    expect(screen.getByRole("status", { name: "ops's gsc server waits for you" })).toBeVisible();

    await userEvent.click(screen.getAllByRole("button", { name: "Approve" })[0]);

    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "ops's grafana server waits for you" }),
      ).not.toBeInTheDocument(),
    );
    expect(
      asked.filter((one) => one.cmd === "approve_persona_server").map((one) => one.args),
    ).toEqual([{ plane: PLANE, name: "ops", server: "grafana", shown: "f00d" }]);
    expect(screen.getByRole("status", { name: "ops's gsc server waits for you" })).toBeVisible();
  });

  it("says the core's refusal, and keeps the server waiting", async () => {
    const refusal =
      "ops/grafana changed since it was shown, so nothing was approved. Read what it runs now, then approve it again.";
    core([GRAFANA], refusal);
    render(<PersonaServersNotice plane={PLANE} persona="ops" canApprove />);

    await userEvent.click(await screen.findByRole("button", { name: "Approve" }));

    expect(await screen.findByText(refusal)).toBeInTheDocument();
    expect(
      screen.getByRole("status", { name: "ops's grafana server waits for you" }),
    ).toBeVisible();
  });

  it("offers no Approve on a link, and says where to approve it", async () => {
    core([GRAFANA]);
    render(<PersonaServersNotice plane={PLANE} persona="ops" canApprove={false} />);

    const notice = await screen.findByRole("status", {
      name: "ops's grafana server waits for you",
    });
    expect(notice).toHaveTextContent("Approve it from purlis's own window on this machine.");
    expect(screen.queryByRole("button", { name: "Approve" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Not now" })).toBeVisible();
  });

  it("offers no Approve for an entry purlis cannot show in full", async () => {
    core([{ ...GRAFANA, fingerprint: null }]);
    render(<PersonaServersNotice plane={PLANE} persona="ops" canApprove />);

    const notice = await screen.findByRole("status", {
      name: "ops's grafana server waits for you",
    });
    expect(notice).toHaveTextContent("cannot show this server's entry in full");
    expect(screen.queryByRole("button", { name: "Approve" })).not.toBeInTheDocument();
  });

  it("hides a server on Not now, and says nothing where nothing waits", async () => {
    core([GRAFANA]);
    render(<PersonaServersNotice plane={PLANE} persona="ops" canApprove />);

    await userEvent.click(await screen.findByRole("button", { name: "Not now" }));

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
