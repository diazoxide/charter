import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { VaultTab } from "./VaultTab";
import type { VaultContents, VaultSecret } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

/** What the tests type as values. None of them may ever be found in the document. */
const TYPED = "typed-value-9f3c1e";
const EDITED = "edited-value-4b7a20";

function secret(key: string, over: Partial<VaultSecret> = {}): VaultSecret {
  return { key, size: "16–31 bytes", updated: "2026-09-24T11:32:17Z", ...over };
}

function contents(secrets: VaultSecret[], over: Partial<VaultContents> = {}): VaultContents {
  return {
    name: "ops",
    provider: "keyring",
    count: secrets.length,
    health: { ok: true, detail: `${secrets.length} secret(s) in the system keyring` },
    secrets,
    ...over,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/**
 * The core: `vault_open` answers `opened`, and each write answers what `writes` says for it —
 * the vault as it now is, or an `Error` whose message is the core's refusal.
 */
function core(
  opened: VaultContents | Error,
  writes: Record<string, VaultContents | Error> = {},
): Asked[] {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    const answer = cmd === "vault_open" ? opened : writes[cmd];
    if (answer === undefined) return null;
    if (answer instanceof Error) throw answer.message;
    return answer;
  });
  return asked;
}

function draw(onChanged = vi.fn()) {
  render(<VaultTab plane={PLANE} vault="ops" onChanged={onChanged} />);
  return onChanged;
}

/** The table's rows, as the text of each cell. */
function rows(): string[][] {
  const table = screen.getByRole("table", { name: "Secrets in ops" });
  return within(table)
    .getAllByRole("row")
    .slice(1)
    .map((row) =>
      within(row)
        .getAllByRole("cell")
        .map((cell) => cell.textContent ?? ""),
    );
}

/** The document holds no value typed into it: not in the markup, and not in any field. */
function noValueAnywhere(...values: string[]) {
  for (const value of values) {
    expect(document.body.innerHTML).not.toContain(value);
    for (const field of document.querySelectorAll("input, textarea")) {
      expect((field as HTMLInputElement).value).not.toBe(value);
    }
  }
}

/** Opens the menu of the row for `key` and picks `item`. */
async function fromTheMenuOf(key: string, item: string) {
  await userEvent.click(await screen.findByRole("button", { name: key }));
  await userEvent.click(await screen.findByRole("menuitem", { name: item }));
}

describe("a vault's tab", () => {
  it("shows the vault's name, provider and count, and a row per secret", async () => {
    const asked = core(
      contents([secret("API_TOKEN"), secret("DB_URL", { size: null, updated: null })]),
    );
    draw();

    expect(await screen.findByRole("heading", { name: /ops/ })).toHaveTextContent(
      "ops · keyring · 2 secrets",
    );
    const table = screen.getByRole("table", { name: "Secrets in ops" });
    expect(
      within(table)
        .getAllByRole("columnheader")
        .map((h) => h.textContent),
    ).toEqual(["Name", "Size", "Updated"]);
    expect(rows()).toEqual([
      ["API_TOKEN", "16–31 bytes", "2026-09-24 11:32 UTC"],
      ["DB_URL", "—", "—"],
    ]);
    expect(asked).toEqual([{ cmd: "vault_open", args: { plane: PLANE, vault: "ops" } }]);
  });

  it("says a vault with no secrets is empty, and offers to add one", async () => {
    core(contents([]));
    draw();
    const empty = await screen.findByTestId("vault-empty");
    expect(empty).toHaveTextContent("ops holds no secrets yet");
    expect(within(empty).getByRole("button", { name: "Add a secret" })).toBeInTheDocument();
  });

  it("draws the core's refusal when the vault cannot be opened", async () => {
    core(new Error("vault 'ops' is not registered"));
    draw();
    expect(await screen.findByRole("alert")).toHaveTextContent("vault 'ops' is not registered");
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
  });

  it("says why a vault charter cannot read is unhealthy", async () => {
    core(contents([], { provider: "1password", health: { ok: false, detail: "op not on PATH" } }));
    draw();
    expect(await screen.findByRole("alert")).toHaveTextContent("op not on PATH");
  });

  it("narrows the table to the secrets whose names hold what is searched for", async () => {
    core(contents([secret("API_TOKEN"), secret("DB_URL"), secret("DEPLOY_TOKEN")]));
    draw();
    const search = await screen.findByRole("searchbox", { name: "Search secrets in ops" });

    await userEvent.type(search, "token");
    expect(rows().map(([name]) => name)).toEqual(["API_TOKEN", "DEPLOY_TOKEN"]);

    await userEvent.clear(search);
    await userEvent.type(search, "nothing-like-it");
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.getByText(/Nothing in ops matches/)).toBeInTheDocument();
  });

  it("adds a secret, draws the vault the core answered, and keeps the value out of the page", async () => {
    const asked = core(contents([secret("DB_URL")]), {
      vault_secret_add: contents([secret("API_TOKEN"), secret("DB_URL")]),
    });
    const changed = draw();
    await userEvent.click(await screen.findByRole("button", { name: "Add" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a secret to ops" });

    await userEvent.type(within(dialog).getByLabelText("Name"), "API_TOKEN");
    await userEvent.type(within(dialog).getByLabelText("Value"), TYPED);
    // Typed, and in the box — but never in the markup, which is what a copy of the page holds.
    expect(document.body.innerHTML).not.toContain(TYPED);
    await userEvent.click(within(dialog).getByRole("button", { name: "Add secret" }));

    await waitFor(() => expect(rows().map(([name]) => name)).toEqual(["API_TOKEN", "DB_URL"]));
    expect(asked.find((one) => one.cmd === "vault_secret_add")?.args).toEqual({
      plane: PLANE,
      vault: "ops",
      key: "API_TOKEN",
      value: TYPED,
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(changed).toHaveBeenCalled();
    noValueAnywhere(TYPED);
  });

  it("keeps the dialog open with the core's sentence when an add is refused", async () => {
    core(contents([secret("API_TOKEN")]), {
      vault_secret_add: new Error("vault 'ops' already holds 'API_TOKEN'."),
    });
    const changed = draw();
    await userEvent.click(await screen.findByRole("button", { name: "Add" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(within(dialog).getByLabelText("Name"), "API_TOKEN");
    await userEvent.type(within(dialog).getByLabelText("Value"), TYPED);
    await userEvent.click(within(dialog).getByRole("button", { name: "Add secret" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("already holds");
    expect(changed).not.toHaveBeenCalled();
    expect(document.body.innerHTML).not.toContain(TYPED);
  });

  it("edits a value from the row's menu, and clears the box once it is written", async () => {
    const asked = core(contents([secret("API_TOKEN")]), {
      vault_secret_set: contents([secret("API_TOKEN", { size: "32–63 bytes" })]),
    });
    draw();
    await fromTheMenuOf("API_TOKEN", "Edit value");
    const dialog = await screen.findByRole("dialog", { name: "Edit the value of API_TOKEN" });
    const box = within(dialog).getByLabelText("New value");
    expect(box).toHaveAttribute("type", "password");
    expect(box).toHaveValue("");

    await userEvent.type(box, EDITED);
    await userEvent.click(within(dialog).getByRole("button", { name: "Save value" }));

    await waitFor(() =>
      expect(rows()).toEqual([["API_TOKEN", "32–63 bytes", "2026-09-24 11:32 UTC"]]),
    );
    expect(asked.find((one) => one.cmd === "vault_secret_set")?.args).toEqual({
      plane: PLANE,
      vault: "ops",
      key: "API_TOKEN",
      value: EDITED,
    });
    expect(box).toHaveValue("");
    noValueAnywhere(EDITED);
  });

  it("renames a secret from the row's menu", async () => {
    const asked = core(contents([secret("OLD")]), {
      vault_secret_rename: contents([secret("NEW")]),
    });
    draw();
    await fromTheMenuOf("OLD", "Rename");
    const dialog = await screen.findByRole("dialog", { name: "Rename OLD" });
    const box = within(dialog).getByLabelText("New name");
    expect(box).toHaveValue("OLD");

    await userEvent.clear(box);
    await userEvent.type(box, "NEW");
    await userEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

    await waitFor(() => expect(rows().map(([name]) => name)).toEqual(["NEW"]));
    expect(asked.find((one) => one.cmd === "vault_secret_rename")?.args).toEqual({
      plane: PLANE,
      vault: "ops",
      from: "OLD",
      to: "NEW",
    });
  });

  it("asks before deleting, and deletes nothing when the answer is Cancel", async () => {
    const asked = core(contents([secret("GONE"), secret("KEPT")]), {
      vault_secret_delete: contents([secret("KEPT")]),
    });
    draw();

    await fromTheMenuOf("GONE", "Delete");
    const asking = await screen.findByRole("alertdialog", { name: "Delete GONE from ops?" });
    expect(within(asking).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await userEvent.click(within(asking).getByRole("button", { name: "Cancel" }));
    expect(asked.some((one) => one.cmd === "vault_secret_delete")).toBe(false);

    await fromTheMenuOf("GONE", "Delete");
    const again = await screen.findByRole("alertdialog");
    await userEvent.click(within(again).getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(rows().map(([name]) => name)).toEqual(["KEPT"]));
    expect(asked.find((one) => one.cmd === "vault_secret_delete")?.args).toEqual({
      plane: PLANE,
      vault: "ops",
      key: "GONE",
    });
  });

  it("is one Tab stop, Up and Down move along the rows, and Enter opens a row's menu", async () => {
    core(contents([secret("A"), secret("B")]));
    draw();
    const a = await screen.findByRole("button", { name: "A" });
    const b = screen.getByRole("button", { name: "B" });
    expect([a, b].map((one) => one.getAttribute("tabindex"))).toEqual(["0", "-1"]);

    a.focus();
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() => expect(b).toHaveFocus());
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await userEvent.keyboard("{Enter}");
    const menu = await screen.findByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((item) => item.textContent),
    ).toEqual(["Edit value", "Rename", "Delete"]);
  });
});
