import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { OpenVault, Vaults, useVaults } from "./Vaults";
import { catalogue, catalogued, type Offer } from "./actions";
import { noTabs } from "./tabs";
import type { VaultSummary } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function vault(name: string, over: Partial<VaultSummary> = {}): VaultSummary {
  return {
    name,
    provider: "keyring",
    count: 2,
    health: { ok: true, detail: "2 secret(s) in the system keyring" },
    ...over,
  };
}

/**
 * The section as the window draws it: the window asks `vault_list` (`useVaults`), and the rows
 * run the catalogue the window builds from the answer. Answers what was pressed, and a way to
 * ask the list again.
 */
function draw() {
  const pressed: Offer[] = [];
  const reload = { now: () => {} };
  function Window() {
    const said = useVaults(PLANE);
    reload.now = said.reload;
    const offers = catalogued(
      catalogue({
        tabs: noTabs(),
        workspaces: [],
        needsYou: [],
        nameOf: String,
        plane: PLANE,
        vaults: said.vaults?.map((one) => one.name),
      }),
    );
    return <Vaults said={said} offers={offers} onPress={(offer) => pressed.push(offer)} />;
  }
  render(<Window />);
  return { pressed, reload: () => act(() => reload.now()) };
}

/** The core, answering `vault_list` with `answer` and recording what it was asked. */
function core(answer: unknown) {
  const asked: unknown[] = [];
  mockIPC((cmd, args) => {
    if (cmd === "vault_list") {
      asked.push(args);
      if (answer instanceof Error) throw answer.message;
      return answer;
    }
    return null;
  });
  return asked;
}

describe("the Vaults section", () => {
  it("lists each vault of the plane with its provider and how many secrets it holds", async () => {
    const asked = core([
      vault("ops"),
      vault("team", { provider: "1password", count: null }),
      vault("files", { provider: "plain-file", count: 1 }),
    ]);

    draw();

    const list = await screen.findByRole("list", { name: "Vaults" });
    const rows = within(list)
      .getAllByRole("listitem")
      .map((row) => row.textContent);
    expect(rows).toEqual([
      "ops · keyring · 2 secrets",
      "team · 1password",
      "files · plain-file · 1 secret",
    ]);
    expect(asked).toEqual([{ plane: PLANE }]);
  });

  it("says a plane with no vaults has none, and how to make one", async () => {
    core([]);
    draw();
    const empty = await screen.findByTestId("list-vaults-empty");
    expect(empty).toHaveTextContent("No vaults on this plane");
    expect(empty).toHaveTextContent("New vault…");
  });

  it("opens a vault's tab when its row is pressed, by the catalogue's own row", async () => {
    core([vault("ops")]);
    const { pressed } = draw();
    const list = await screen.findByRole("list", { name: "Vaults" });

    await userEvent.click(within(list).getByRole("button", { name: /ops/ }));

    expect(pressed.map((offer) => offer.id)).toEqual(["vault.open:ops"]);
    expect(pressed[0].does).toEqual({
      verb: "openView",
      view: { from: null, view: "vault", key: "ops" },
      title: "ops",
    });
  });

  it("reads the list again when the window asks, after a tab wrote to a vault", async () => {
    const asked = core([vault("ops")]);
    const { reload } = draw();
    await waitFor(() => expect(asked).toHaveLength(1));

    await reload();

    await waitFor(() => expect(asked).toHaveLength(2));
  });

  it("marks a vault charter cannot read, and leaves why to its tab", async () => {
    core([
      vault("team", {
        provider: "1password",
        count: null,
        health: { ok: false, detail: "op CLI not on PATH" },
      }),
    ]);
    draw();
    const list = await screen.findByRole("list", { name: "Vaults" });
    const row = within(list).getByRole("listitem");
    expect(row).toHaveClass("is-trouble");
    // No card: the tab says why, at the top, where the operator is looking when it matters.
    expect(screen.queryByText("op CLI not on PATH")).not.toBeInTheDocument();
  });

  it("draws the core's refusal rather than an empty list", async () => {
    core(new Error("vault registry vaults.json is corrupt: not a JSON object"));
    draw();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "vault registry vaults.json is corrupt",
    );
    expect(screen.queryByTestId("list-vaults-empty")).not.toBeInTheDocument();
  });

  it("is one Tab stop, and Up and Down move between vaults (charter-app#189)", async () => {
    core([vault("files", { provider: "plain-file", count: 1 }), vault("ops")]);
    draw();
    const list = await screen.findByRole("list", { name: "Vaults" });
    const [first, second] = within(list).getAllByRole("button");
    expect([first, second].map((one) => one.getAttribute("tabindex"))).toEqual(["0", "-1"]);

    first.focus();
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() => expect(second).toHaveFocus());
  });

  it("makes no rows out of an answer that is not a list", async () => {
    // Whole-window tests answer every command they do not care about with `null` or `[]`.
    const asked = core(null);
    draw();
    await waitFor(() => expect(asked).toHaveLength(1));
    // Let the answer land and React commit whatever it would draw from it.
    await act(async () => {
      await new Promise((settled) => setTimeout(settled, 0));
    });
    expect(screen.queryByRole("list", { name: "Vaults" })).not.toBeInTheDocument();
    expect(screen.queryByTestId("list-vaults-empty")).not.toBeInTheDocument();
  });
});

describe("the vault picker", () => {
  function pick(vaults: VaultSummary[]) {
    const pressed: Offer[] = [];
    const cancel = vi.fn();
    const offers = catalogued(
      catalogue({
        tabs: noTabs(),
        workspaces: [],
        needsYou: [],
        nameOf: String,
        plane: PLANE,
        vaults: vaults.map((one) => one.name),
      }),
    );
    render(
      <OpenVault
        vaults={vaults}
        offers={offers}
        onPress={(offer) => pressed.push(offer)}
        onCancel={cancel}
      />,
    );
    return { pressed, cancel };
  }

  it("lists the plane's vaults, and opens the one picked", async () => {
    const { pressed, cancel } = pick([vault("files", { provider: "plain-file" }), vault("ops")]);
    const dialog = await screen.findByRole("dialog", { name: "Open vault" });

    await userEvent.click(within(dialog).getByRole("button", { name: /ops/ }));

    expect(pressed.map((offer) => offer.id)).toEqual(["vault.open:ops"]);
    expect(cancel).toHaveBeenCalled();
  });

  it("puts the keyboard on the first vault, so Enter opens it", async () => {
    const { pressed } = pick([vault("files"), vault("ops")]);
    const first = await screen.findByRole("button", { name: /files/ });
    await waitFor(() => expect(first).toHaveFocus());

    await userEvent.keyboard("{ArrowDown}{Enter}");

    expect(pressed.map((offer) => offer.id)).toEqual(["vault.open:ops"]);
  });

  it("is closed by Escape and opens nothing", async () => {
    const { pressed, cancel } = pick([vault("ops")]);
    await screen.findByRole("dialog");
    await userEvent.keyboard("{Escape}");
    expect(cancel).toHaveBeenCalled();
    expect(pressed).toEqual([]);
  });
});
