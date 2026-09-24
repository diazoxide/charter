import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { VaultTab } from "./VaultTab";
import type { VaultContents, VaultSecret } from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
  vi.useRealTimers();
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
  writes: Record<string, VaultContents | string | boolean | Error> = {},
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
    ).toEqual(["Name", "Size", "Updated", ""]);
    expect(rows()).toEqual([
      ["API_TOKEN", "16–31 bytes", "2026-09-24 11:32 UTC", ""],
      ["DB_URL", "—", "—", ""],
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
    // The box was emptied at the press: the value went to the core and is not kept for a retry.
    expect(within(dialog).getByLabelText("Value")).toHaveValue("");
    expect(within(dialog).getByRole("button", { name: "Add secret" })).toBeDisabled();
    noValueAnywhere(TYPED);
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
      expect(rows()).toEqual([["API_TOKEN", "32–63 bytes", "2026-09-24 11:32 UTC", ""]]),
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
    ).toEqual(["Edit value", "Rename", "Copy", "Delete"]);
  });
});

/** What the core answers a reveal with. Never in the page once it is hidden. */
const REVEALED = "revealed-value-6d02b8";
const OTHER = "other-revealed-value-31fa";

/**
 * A fake clock for the 30 seconds and the minute, and a user whose waits run on it. It also moves
 * with real time — Testing Library's own waits are timeouts, and a clock that never moved would
 * leave them waiting — so the assertions below leave a second's slack either side of a deadline
 * rather than a millisecond's.
 */
function onAFakeClock() {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  return userEvent.setup({ advanceTimers: (ms) => vi.advanceTimersByTime(ms) });
}

/** Moves the fake clock on by `ms`, and lets what it set off settle. */
async function after(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

describe("revealing a value", () => {
  it("fetches that one value, shows it for 30 seconds, then drops it from the page", async () => {
    const user = onAFakeClock();
    const asked = core(contents([secret("API_TOKEN"), secret("DB_URL")]), {
      vault_secret_reveal: REVEALED,
    });
    draw();
    const eye = await screen.findByRole("button", { name: "Reveal API_TOKEN" });
    expect(eye).toHaveAttribute("aria-pressed", "false");

    await user.click(eye);

    expect(await screen.findByText(REVEALED)).toBeInTheDocument();
    expect(eye).toHaveAttribute("aria-pressed", "true");
    expect(asked.filter((one) => one.cmd === "vault_secret_reveal")).toEqual([
      { cmd: "vault_secret_reveal", args: { plane: PLANE, vault: "ops", key: "API_TOKEN" } },
    ]);

    await after(29_000);
    expect(screen.getByText(REVEALED)).toBeInTheDocument();
    await after(1_000);
    expect(eye).toHaveAttribute("aria-pressed", "false");
    noValueAnywhere(REVEALED);
  });

  it("hides the value early when the eye is pressed again, or on Escape", async () => {
    const user = onAFakeClock();
    core(contents([secret("API_TOKEN")]), { vault_secret_reveal: REVEALED });
    draw();
    const eye = await screen.findByRole("button", { name: "Reveal API_TOKEN" });

    await user.click(eye);
    await screen.findByText(REVEALED);
    await user.click(eye);
    noValueAnywhere(REVEALED);

    await user.click(eye);
    await screen.findByText(REVEALED);
    await user.keyboard("{Escape}");
    noValueAnywhere(REVEALED);
    expect(eye).toHaveAttribute("aria-pressed", "false");
  });

  it("shows one value at a time", async () => {
    const user = onAFakeClock();
    let answer = REVEALED;
    const asked: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "vault_open") return contents([secret("A"), secret("B")]);
      if (cmd === "vault_secret_reveal") {
        asked.push((args as { key: string }).key);
        return answer;
      }
      return null;
    });
    draw();

    await user.click(await screen.findByRole("button", { name: "Reveal A" }));
    await screen.findByText(REVEALED);
    answer = OTHER;
    await user.click(screen.getByRole("button", { name: "Reveal B" }));

    await screen.findByText(OTHER);
    noValueAnywhere(REVEALED);
    expect(screen.getByRole("button", { name: "Reveal A" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(asked).toEqual(["A", "B"]);
  });

  it("says why when the core refuses a reveal, and shows nothing", async () => {
    const user = onAFakeClock();
    core(contents([secret("API_TOKEN")]), {
      vault_secret_reveal: new Error("the keychain refused to hand over 'API_TOKEN'"),
    });
    draw();

    await user.click(await screen.findByRole("button", { name: "Reveal API_TOKEN" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("the keychain refused");
    expect(screen.getByRole("button", { name: "Reveal API_TOKEN" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("drops a revealed value when the vault is written to", async () => {
    const user = onAFakeClock();
    core(contents([secret("OLD")]), {
      vault_secret_reveal: REVEALED,
      vault_secret_rename: contents([secret("NEW")]),
    });
    draw();
    await user.click(await screen.findByRole("button", { name: "Reveal OLD" }));
    await screen.findByText(REVEALED);

    await user.click(screen.getByRole("button", { name: "OLD" }));
    await user.click(await screen.findByRole("menuitem", { name: "Rename" }));
    const box = within(await screen.findByRole("dialog")).getByLabelText("New name");
    await user.clear(box);
    await user.type(box, "NEW");
    await user.keyboard("{Enter}");

    await screen.findByRole("button", { name: "Reveal NEW" });
    noValueAnywhere(REVEALED);
  });

  it("reaches a row's eye with Right from its name, and comes back with Left", async () => {
    core(contents([secret("A"), secret("B")]));
    draw();
    const a = await screen.findByRole("button", { name: "A" });
    const eye = screen.getByRole("button", { name: "Reveal A" });
    // The eyes are not Tab stops of their own: the list stays one.
    expect(eye).toHaveAttribute("tabindex", "-1");

    a.focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(eye).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(a).toHaveFocus();
  });
});

describe("copying a value", () => {
  it("asks the core to copy it, never receives it, and asks for the clear a minute later", async () => {
    const user = onAFakeClock();
    const asked = core(contents([secret("API_TOKEN")]), { vault_clipboard_clear: true });
    draw();

    await user.click(await screen.findByRole("button", { name: "API_TOKEN" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Copied API_TOKEN. The clipboard clears in a minute",
    );
    expect(asked.filter((one) => one.cmd === "vault_secret_copy")).toEqual([
      { cmd: "vault_secret_copy", args: { plane: PLANE, vault: "ops", key: "API_TOKEN" } },
    ]);
    expect(asked.some((one) => one.cmd === "vault_secret_reveal")).toBe(false);

    await after(59_000);
    expect(asked.some((one) => one.cmd === "vault_clipboard_clear")).toBe(false);
    await after(1_000);
    expect(asked.filter((one) => one.cmd === "vault_clipboard_clear")).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent("");
  });

  it("leaves the clipboard to the core when something else was copied since", async () => {
    // The core keeps a digest of what it copied and clears only while the clipboard still
    // holds it (`vaults.rs`); here it answers that it did not, and the tab claims nothing.
    const user = onAFakeClock();
    const asked = core(contents([secret("API_TOKEN")]), { vault_clipboard_clear: false });
    draw();

    await user.click(await screen.findByRole("button", { name: "API_TOKEN" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy" }));
    await screen.findByText(/Copied API_TOKEN/);
    await after(60_000);

    expect(asked.filter((one) => one.cmd === "vault_clipboard_clear")).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent("");
  });

  it("counts the minute from the last copy, and clears once", async () => {
    const user = onAFakeClock();
    const asked = core(contents([secret("A"), secret("B")]), { vault_clipboard_clear: true });
    draw();
    const copy = async (key: string) => {
      await user.click(await screen.findByRole("button", { name: key }));
      await user.click(await screen.findByRole("menuitem", { name: "Copy" }));
      await screen.findByText(new RegExp(`Copied ${key}`));
    };

    await copy("A");
    await after(30_000);
    await copy("B");
    await after(59_000);
    expect(asked.some((one) => one.cmd === "vault_clipboard_clear")).toBe(false);
    await after(1_000);
    expect(asked.filter((one) => one.cmd === "vault_clipboard_clear")).toHaveLength(1);
  });

  it("still clears the clipboard when the tab is closed within the minute", async () => {
    const user = onAFakeClock();
    const asked = core(contents([secret("API_TOKEN")]), { vault_clipboard_clear: true });
    draw();
    await user.click(await screen.findByRole("button", { name: "API_TOKEN" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy" }));
    await screen.findByText(/Copied API_TOKEN/);

    cleanup();
    await after(60_000);

    expect(asked.filter((one) => one.cmd === "vault_clipboard_clear")).toHaveLength(1);
  });

  it("says why when the core refuses a copy", async () => {
    const user = onAFakeClock();
    const asked = core(contents([secret("API_TOKEN")]), {
      vault_secret_copy: new Error("the clipboard did not take the copy"),
    });
    draw();
    await user.click(await screen.findByRole("button", { name: "API_TOKEN" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("did not take the copy");
    await after(60_000);
    expect(asked.some((one) => one.cmd === "vault_clipboard_clear")).toBe(false);
  });
});
