import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { Vaults } from "./Vaults";
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

    render(<Vaults plane={PLANE} />);

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
    render(<Vaults plane={PLANE} />);
    const empty = await screen.findByTestId("list-vaults-empty");
    expect(empty).toHaveTextContent("No vaults on this plane");
    expect(empty).toHaveTextContent("charter vault add <name>");
  });

  it("marks a vault charter cannot read, and says why in its card", async () => {
    core([vault("team", { provider: "1password", count: null, health: { ok: false, detail: "op CLI not on PATH" } })]);
    render(<Vaults plane={PLANE} />);
    const list = await screen.findByRole("list", { name: "Vaults" });
    const row = within(list).getByRole("listitem");
    expect(row).toHaveClass("is-trouble");
    within(row).getByRole("button").click();
    expect(await screen.findByTestId("row-detail-team")).toHaveTextContent("op CLI not on PATH");
  });

  it("draws the core's refusal rather than an empty list", async () => {
    core(new Error("vault registry vaults.json is corrupt: not a JSON object"));
    render(<Vaults plane={PLANE} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("vault registry vaults.json is corrupt");
    expect(screen.queryByTestId("list-vaults-empty")).not.toBeInTheDocument();
  });

  it("makes no rows out of an answer that is not a list", async () => {
    // Whole-window tests answer every command they do not care about with `null` or `[]`.
    const asked = core(null);
    render(<Vaults plane={PLANE} />);
    await waitFor(() => expect(asked).toHaveLength(1));
    expect(screen.queryByRole("list", { name: "Vaults" })).not.toBeInTheDocument();
  });
});
