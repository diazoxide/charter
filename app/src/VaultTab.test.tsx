import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { VaultTab } from "./VaultTab";
import { saidOfTheOldExport } from "./VaultSignIn";
import type { UnreadFor, VaultContents, VaultSecret } from "./bindings";

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
    refused: null,
    identity_unset_elsewhere: [],
    identity: [],
    identity_in_app_env: [],
    ...over,
  };
}

type Asked = { cmd: string; args: Record<string, unknown> };

/**
 * The core: `vault_open` answers `opened`, and each write answers what `writes` says for it —
 * the vault as it now is, or an `Error` whose message is the core's refusal.
 */
function core(opened: VaultContents | Error, writes: Record<string, unknown> = {}): Asked[] {
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

function draw(onChanged = vi.fn(), onOpenVault?: (vault: string) => void) {
  render(<VaultTab plane={PLANE} vault="ops" onChanged={onChanged} onOpenVault={onOpenVault} />);
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

  it("reads again from the refusal, and from the health line, on Read again (#1296)", async () => {
    // Refused, then unhealthy, then read: each answer is the next `vault_open`'s.
    const answers: (VaultContents | Error)[] = [
      new Error("vault 'ops' could not be opened"),
      contents([], { provider: "1password", health: { ok: false, detail: "op not on PATH" } }),
      contents([secret("API_TOKEN")]),
    ];
    let opened = 0;
    mockIPC((cmd) => {
      if (cmd !== "vault_open") return null;
      const answer = answers[Math.min(opened++, answers.length - 1)];
      if (answer instanceof Error) throw answer.message;
      return answer;
    });
    draw();

    expect(await screen.findByRole("alert")).toHaveTextContent("vault 'ops' could not be opened");
    await userEvent.click(screen.getByRole("button", { name: "Read again" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("op not on PATH");
    await userEvent.click(screen.getByRole("button", { name: "Read again" }));

    await waitFor(() =>
      expect(rows()).toEqual([["API_TOKEN", "16–31 bytes", "2026-09-24 11:32 UTC", ""]]),
    );
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Read again" })).not.toBeInTheDocument();
    expect(opened).toBe(3);
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

  it("cancels a reveal still on its way when the eye is pressed again", async () => {
    const user = onAFakeClock();
    let answer: (value: string) => void = () => undefined;
    const asked: string[] = [];
    mockIPC((cmd, args) => {
      if (cmd === "vault_open") return contents([secret("API_TOKEN")]);
      if (cmd === "vault_secret_reveal") {
        asked.push((args as { key: string }).key);
        return new Promise<string>((resolve) => (answer = resolve));
      }
      return null;
    });
    draw();
    const eye = await screen.findByRole("button", { name: "Reveal API_TOKEN" });

    await user.click(eye);
    await user.click(eye);
    await act(async () => answer(REVEALED));

    expect(asked).toEqual(["API_TOKEN"]);
    expect(eye).toHaveAttribute("aria-pressed", "false");
    noValueAnywhere(REVEALED);
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
  // The clipboard itself is the core's: it reads the value, writes it, and clears it a minute
  // later only while the clipboard still holds it. That minute, and "something else was copied
  // since", are tested on tokio's paused clock in `vaults.rs`. Here: the tab asks, never
  // receives the value, and says so for as long as it is there.
  it("asks the core to copy it and never receives the value", async () => {
    const user = onAFakeClock();
    const asked = core(contents([secret("API_TOKEN")]));
    draw();

    await user.click(await screen.findByRole("button", { name: "API_TOKEN" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "Copied API_TOKEN. The clipboard clears in a minute",
    );
    expect(asked.filter((one) => one.cmd !== "vault_open")).toEqual([
      { cmd: "vault_secret_copy", args: { plane: PLANE, vault: "ops", key: "API_TOKEN" } },
    ]);
  });

  it("stops saying the value is on the clipboard when the core clears it, a minute on", async () => {
    const user = onAFakeClock();
    core(contents([secret("API_TOKEN")]));
    draw();
    await user.click(await screen.findByRole("button", { name: "API_TOKEN" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy" }));
    await screen.findByText(/Copied API_TOKEN/);

    await after(59_000);
    expect(screen.getByRole("status")).toHaveTextContent("Copied API_TOKEN");
    await after(1_000);
    expect(screen.getByRole("status")).toHaveTextContent("");
  });

  it("says why when the core refuses a copy", async () => {
    const user = onAFakeClock();
    core(contents([secret("API_TOKEN")]), {
      vault_secret_copy: new Error("the clipboard did not take the copy"),
    });
    draw();
    await user.click(await screen.findByRole("button", { name: "API_TOKEN" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("did not take the copy");
    expect(screen.getByRole("status")).toHaveTextContent("");
  });
});

describe("a 1Password vault's token", () => {
  const PUT = "put-token-9f21ab";

  /** `team`, a 1Password vault read through `$OP_TEAM_TOKEN`, held `held`, with `inAppEnv` the
   *  variables purlis's own environment still carries. */
  function team(
    held: "environment" | "keyring",
    inAppEnv: string[] = held === "environment" ? ["OP_TEAM_TOKEN"] : [],
  ): VaultContents {
    return contents([secret("DEPLOY", { size: null, updated: null })], {
      provider: "1password",
      identity: [{ variable: "OP_TEAM_TOKEN", held, kept: false }],
      identity_in_app_env: inAppEnv,
    });
  }

  it("puts a pasted token straight into the Keychain, and keeps it out of the page", async () => {
    // The token the operator pastes goes to `vault_identity_put`; the app's environment never has
    // it, so no chat can read it (#271 review, U3). The box is emptied at the press.
    const asked = core(team("environment"), { vault_identity_put: team("keyring") });
    const onChanged = draw();

    const box = await screen.findByLabelText("Token for $OP_TEAM_TOKEN");
    await userEvent.type(box, PUT);
    await userEvent.click(
      screen.getByRole("button", { name: "Put this vault's token in the Keychain" }),
    );

    expect(asked.at(-1)).toEqual({
      cmd: "vault_identity_put",
      args: { plane: PLANE, vault: "ops", token: PUT },
    });
    noValueAnywhere(PUT);
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Stored $OP_TEAM_TOKEN in the Keychain",
    );
    expect(screen.getByText(/reads \$OP_TEAM_TOKEN from the Keychain/)).toBeInTheDocument();
    expect(onChanged).toHaveBeenCalled();
  });

  it("offers to move the token purlis's environment already has, and warns to relaunch", async () => {
    // The move path leaves the export in the app's own process, so the note warns to relaunch
    // (#271 review, U3): the honest wording, not "no chat is given it".
    const asked = core(team("environment"), {
      vault_identity_move: team("keyring", ["OP_TEAM_TOKEN"]),
    });
    draw();

    await userEvent.click(
      await screen.findByRole("button", { name: "Move the token from purlis's environment" }),
    );

    expect(asked.at(-1)).toEqual({
      cmd: "vault_identity_move",
      args: { plane: PLANE, vault: "ops" },
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("relaunch purlis");
  });

  it("says where the token is, and offers nothing, once it is in the Keychain", async () => {
    core(team("keyring"));
    draw();

    expect(await screen.findByText(/reads \$OP_TEAM_TOKEN from the Keychain/)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Put this vault's token in the Keychain" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Move the token from purlis's environment" }),
    ).not.toBeInTheDocument();
  });

  it("still warns to relaunch when the token is in the Keychain but the shell still exports it", async () => {
    core(team("keyring", ["OP_TEAM_TOKEN"]));
    draw();

    expect(await screen.findByText(/relaunch purlis without it/)).toBeInTheDocument();
  });

  it("offers nothing for a vault read through no token", async () => {
    core(contents([secret("API_TOKEN")]));
    draw();

    await screen.findByRole("table", { name: "Secrets in ops" });
    expect(
      screen.queryByRole("button", { name: "Put this vault's token in the Keychain" }),
    ).not.toBeInTheDocument();
  });

  it("says why when the core refuses the put, and keeps offering it", async () => {
    core(team("environment"), {
      vault_identity_put: new Error(
        "purlis could not write 'charter/@identity/ab' in the system keyring",
      ),
    });
    draw();

    const box = await screen.findByLabelText("Token for $OP_TEAM_TOKEN");
    await userEvent.type(box, PUT);
    await userEvent.click(
      screen.getByRole("button", { name: "Put this vault's token in the Keychain" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent("could not write");
    expect(
      screen.getByRole("button", { name: "Put this vault's token in the Keychain" }),
    ).toBeInTheDocument();
    noValueAnywhere(PUT);
  });

  // --- #1526: the token is nowhere, so the contents cannot be read ------------------------ //

  const UNSET =
    "vault 'ops' is read through $OP_TEAM_TOKEN, which is unset. purlis will not fall back to an ambient $OP_SERVICE_ACCOUNT_TOKEN. Paste the token into the box below: it goes straight into the Keychain.";
  const PASTE_HERE =
    "Paste the service-account token for $OP_TEAM_TOKEN here. It goes straight into the Keychain; purlis reads it from there, and no chat is given it.";

  /** `team` as the core answers it when its contents could not be read: no secrets, why and what
   *  kind of failure, and the identity it declares all the same. */
  function unread(
    held: "unset" | "keyring",
    why = UNSET,
    kind: UnreadFor = held === "unset" ? "no-token" : "other",
  ): VaultContents {
    return contents([], {
      provider: "1password",
      health: { ok: false, detail: why },
      refused: { why, kind },
      identity: [{ variable: "OP_TEAM_TOKEN", held, kept: false }],
    });
  }

  const PUT_IT = "Put this vault's token in the Keychain";

  it("draws the refusal and the paste box when the token is nowhere, and no empty vault", async () => {
    core(unread("unset"));
    draw();

    expect(await screen.findByRole("alert")).toHaveTextContent(UNSET);
    expect(screen.getByText(PASTE_HERE)).toBeInTheDocument();
    expect(screen.getByLabelText("Token for $OP_TEAM_TOKEN")).toHaveAttribute("type", "password");
    expect(screen.getByRole("button", { name: PUT_IT })).toBeEnabled();
    // Nothing that would read as a vault with nothing in it, or offer to write to one.
    expect(screen.queryByTestId("vault-empty")).not.toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /ops/ })).toHaveTextContent("ops · 1password");
    expect(screen.getByRole("heading", { name: /ops/ })).not.toHaveTextContent("0 secrets");
  });

  it("stores a token pasted there, then lists the vault's secrets without being asked", async () => {
    const asked = core(unread("unset"), { vault_identity_put: team("keyring") });
    const onChanged = draw();

    const box = await screen.findByLabelText("Token for $OP_TEAM_TOKEN");
    await userEvent.type(box, PUT);
    await userEvent.click(screen.getByRole("button", { name: PUT_IT }));

    expect(asked.at(-1)).toEqual({
      cmd: "vault_identity_put",
      args: { plane: PLANE, vault: "ops", token: PUT },
    });
    expect(await screen.findByRole("table", { name: "Secrets in ops" })).toBeInTheDocument();
    expect(rows()).toEqual([["DEPLOY", "—", "—", ""]]);
    expect(screen.getByText(/reads \$OP_TEAM_TOKEN from the Keychain/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Stored $OP_TEAM_TOKEN in the Keychain. purlis reads it from there, and no chat is given the token.",
    );
    expect(screen.queryByText(/which is unset/)).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Token for $OP_TEAM_TOKEN")).not.toBeInTheDocument();
    noValueAnywhere(PUT);
    expect(onChanged).toHaveBeenCalled();
  });

  it("stores it on Enter too, and the box is empty from the press on", async () => {
    const asked = core(unread("unset"), { vault_identity_put: unread("keyring", "op said no") });
    draw();

    const box = await screen.findByLabelText("Token for $OP_TEAM_TOKEN");
    await userEvent.type(box, `${PUT}{Enter}`);

    expect(asked.at(-1)?.cmd).toBe("vault_identity_put");
    noValueAnywhere(PUT);
  });

  it("never says Stored beside a refusal: it says the token is kept and the read still failed", async () => {
    core(unread("unset"), {
      vault_identity_put: unread("keyring", "reading vault 'ops' failed (op exit 1).", "other"),
    });
    draw();

    await userEvent.type(await screen.findByLabelText("Token for $OP_TEAM_TOKEN"), PUT);
    await userEvent.click(screen.getByRole("button", { name: PUT_IT }));

    expect(
      await screen.findByText(
        "$OP_TEAM_TOKEN is stored in the Keychain, and the vault still could not be read: the reason is above.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^Stored /)).not.toBeInTheDocument();
    expect(screen.getByText("reading vault 'ops' failed (op exit 1).")).toBeInTheDocument();
    expect(screen.getByLabelText("Token for $OP_TEAM_TOKEN")).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    noValueAnywhere(PUT);
  });

  it("says what is known of a kept token that did not read the vault, and blames it only for a refused sign-in", async () => {
    const cases: [UnreadFor, RegExp, RegExp | null][] = [
      [
        "program",
        /could not run the program that reads this vault\. Nothing says the token is wrong\. Storing the token again here pins the program/,
        /Paste the right token/,
      ],
      [
        "try-again",
        /Nothing says the token is wrong: read again in a moment/,
        /Paste the right token|to replace it\.$/,
      ],
      [
        "sign-in",
        /the sign-in with it was refused\. Paste the right token here to replace it\./,
        null,
      ],
      [
        "other",
        /If it is the token that is wrong, paste the right one here to replace it\./,
        /Nothing says/,
      ],
    ];
    for (const [kind, says, never] of cases) {
      core(unread("keyring", `the provider's own reason (${kind})`, kind));
      draw();

      expect(await screen.findByRole("alert")).toHaveTextContent(
        `the provider's own reason (${kind})`,
      );
      expect(screen.getByText(says)).toBeInTheDocument();
      if (never) expect(screen.queryByText(never)).not.toBeInTheDocument();
      // The box in every case.
      expect(screen.getByLabelText("Token for $OP_TEAM_TOKEN")).toBeInTheDocument();
      expect(screen.queryByText(/could not read the vault with it/)).not.toBeInTheDocument();
      cleanup();
      clearMocks();
    }
  });

  it("reads the vault again on Read again", async () => {
    const asked = core(unread("keyring", "rate-limited", "try-again"));
    draw();

    await userEvent.click(await screen.findByRole("button", { name: "Read again" }));

    await waitFor(() => expect(asked.filter((one) => one.cmd === "vault_open")).toHaveLength(2));
  });

  it("says why when the core refuses to store it, beside the refusal and the box", async () => {
    core(unread("unset"), {
      vault_identity_put: new Error("purlis could not write 'purlis/@identity/ab'"),
    });
    draw();

    await userEvent.type(await screen.findByLabelText("Token for $OP_TEAM_TOKEN"), PUT);
    await userEvent.click(screen.getByRole("button", { name: PUT_IT }));

    expect(await screen.findByText(/could not write/)).toBeInTheDocument();
    expect(screen.getByText(UNSET)).toBeInTheDocument();
    expect(screen.getByLabelText("Token for $OP_TEAM_TOKEN")).toBeInTheDocument();
    noValueAnywhere(PUT);
  });

  it("draws no box for a vault read through several variables, and says why", async () => {
    core(
      contents([], {
        provider: "1password",
        refused: { why: UNSET, kind: "no-token" },
        identity: [
          { variable: "OP_TEAM_TOKEN", held: "unset", kept: false },
          { variable: "OP_CONNECT_TOKEN", held: "unset", kept: false },
        ],
      }),
    );
    draw();

    expect(
      await screen.findByText(
        /Read through \$OP_TEAM_TOKEN, \$OP_CONNECT_TOKEN\. A box stores one token and this vault needs 2/,
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: PUT_IT })).not.toBeInTheDocument();
    expect(document.querySelector("input[type=password]")).toBeNull();
  });

  // --- a token is stored per vault: the others are pointed at, never written -------------- //

  it("names the other vaults that still have no token, each a link to its own tab", async () => {
    const asked = core(unread("unset"), {
      vault_identity_put: {
        ...team("keyring"),
        identity_unset_elsewhere: ["authz-master", "edge"],
      },
    });
    const onOpenVault = vi.fn();
    draw(vi.fn(), onOpenVault);

    await userEvent.type(await screen.findByLabelText("Token for $OP_TEAM_TOKEN"), PUT);
    await userEvent.click(screen.getByRole("button", { name: PUT_IT }));

    expect(
      await screen.findAllByText(
        /also reads through \$OP_TEAM_TOKEN and has no token yet\. A token is stored per vault: put it in from that vault's tab\./,
      ),
    ).toHaveLength(2);
    await userEvent.click(screen.getByRole("button", { name: "authz-master" }));
    expect(onOpenVault).toHaveBeenCalledWith("authz-master");
    expect(screen.getByRole("button", { name: "edge" })).toBeInTheDocument();
    // A pointer: nothing was asked of the core for the other vaults.
    expect(asked.map((one) => one.cmd)).toEqual(["vault_open", "vault_identity_put"]);
    expect(asked.every((one) => one.args.vault === "ops")).toBe(true);
  });

  it("reads again when a token is stored in another vault's tab of the same project", async () => {
    const asked = core(team("keyring"));
    render(<VaultTab plane={PLANE} vault="ops" onChanged={vi.fn()} />);
    render(<VaultTab plane={PLANE} vault="edge" onChanged={vi.fn()} />);
    render(<VaultTab plane="/home/dev/other" vault="far" onChanged={vi.fn()} />);
    await waitFor(() => expect(asked).toHaveLength(3));

    act(() => {
      window.dispatchEvent(
        new CustomEvent("purlis:vault-token-stored", { detail: { plane: PLANE, vault: "edge" } }),
      );
    });

    await waitFor(() => expect(asked).toHaveLength(4));
    // The other tab of that project, and neither the one that stored nor another project's.
    expect(asked[3]).toEqual({ cmd: "vault_open", args: { plane: PLANE, vault: "ops" } });
  });

  it("does not read again for a store elsewhere when it is read through no token", async () => {
    const asked = core(contents([secret("API_TOKEN")]));
    draw();
    await screen.findByRole("table", { name: "Secrets in ops" });

    act(() => {
      window.dispatchEvent(
        new CustomEvent("purlis:vault-token-stored", { detail: { plane: PLANE, vault: "edge" } }),
      );
    });

    expect(asked).toHaveLength(1);
  });
});

describe("how a 1Password vault signs in, from its tab (#1527)", () => {
  /** A made-up token. It may never be found in the document. */
  const GIVEN = "made-up-word-for-the-tab-1527";

  function onePassword(over: Partial<VaultContents> = {}): VaultContents {
    return contents([secret("DEPLOY", { size: null, updated: null })], {
      provider: "1password",
      ...over,
    });
  }

  const kept = (held: "keyring" | "unset") => [
    { variable: "service-account-token", held, kept: true },
  ];

  it("names a token kept in the Keychain as the vault's, not as a variable", async () => {
    core(onePassword({ identity: kept("keyring") }));
    draw();
    expect(
      await screen.findByText("purlis reads this vault's token from the Keychain."),
    ).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("$service-account-token");
  });

  it("draws the box for a kept token this machine has none of, named for the vault", async () => {
    core(
      onePassword({
        secrets: [],
        count: 0,
        identity: kept("unset"),
        refused: { kind: "no-token", why: "vault 'ops' is read with a service-account token." },
      }),
    );
    draw();
    expect(await screen.findByLabelText("Token for this vault")).toHaveAttribute(
      "type",
      "password",
    );
    expect(
      screen.getByText(/Paste the service-account token for this vault here/),
    ).toBeInTheDocument();
    // Nothing to move: a kept token is in no environment.
    expect(screen.queryByRole("button", { name: /Move the token/ })).not.toBeInTheDocument();
  });

  it("offers the change for a 1Password vault only", async () => {
    core(contents([secret("K")]));
    draw();
    await screen.findByRole("table", { name: "Secrets in ops" });
    expect(
      screen.queryByRole("button", { name: "Change how this vault signs in" }),
    ).not.toBeInTheDocument();
  });

  it("converts a vault bound to a variable: token once, tested where it lives, stored", async () => {
    const bound = onePassword({
      secrets: [],
      count: 0,
      identity: [{ variable: "OP_TEAM_TOKEN", held: "unset", kept: false }],
      refused: { kind: "no-token", why: "vault 'ops' is read through $OP_TEAM_TOKEN." },
    });
    const converted = onePassword({ identity: kept("keyring") });
    const asked = core(bound, {
      vault_setup_begin: {
        setup: 3,
        op_vaults: ["Engineering"],
        listing: null,
        alike: [
          {
            name: "edge",
            op_vault: "Edge",
            op_item: "charter-edge",
            account: null,
            persona: null,
            half: "shared",
            held: "unset",
            ticked: false,
            digest: "digest-of-edge",
          },
        ],
      },
      vault_setup_test: { items: 4, item: "charter-ops", item_there: true, failed: null },
      vault_setup_change: {
        contents: converted,
        marked: [],
        skipped: [{ name: "edge", why: "changed", said: null }],
        no_longer_read: [],
        checked_every_project: true,
      },
    });
    const onChanged = draw();

    await userEvent.click(
      await screen.findByRole("button", { name: "Change how this vault signs in" }),
    );
    const dialog = screen.getByRole("dialog", { name: "How ops signs in" });
    await userEvent.type(within(dialog).getByLabelText("Service-account token"), GIVEN);
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));
    await within(dialog).findByText(/purlis has the token for this set-up/);

    expect(asked.at(-1)).toEqual({
      cmd: "vault_setup_begin",
      args: { plane: PLANE, token: GIVEN, account: null, vault: "ops" },
    });
    noValueAnywhere(GIVEN);
    // Where the items live is the vault's own: it is not asked again.
    expect(within(dialog).queryByLabelText("1Password vault")).not.toBeInTheDocument();
    expect(within(dialog).queryByLabelText("Name")).not.toBeInTheDocument();
    // The committed vault bound to the same variable is listed, and starts unticked.
    await userEvent.click(within(dialog).getByRole("checkbox", { name: "edge" }));

    await userEvent.click(within(dialog).getByRole("button", { name: "Test" }));
    expect(await within(dialog).findByRole("status")).toHaveTextContent(
      "Signed in. 4 items in that 1Password vault; the item charter-ops is there.",
    );
    expect(asked.at(-1)).toEqual({
      cmd: "vault_setup_test",
      args: { plane: PLANE, setup: 3, vault: "ops", opVault: null, opItem: null },
    });

    await userEvent.click(within(dialog).getByRole("button", { name: "Store" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(asked.at(-1)).toEqual({
      cmd: "vault_setup_change",
      args: {
        plane: PLANE,
        setup: 3,
        vault: "ops",
        also: [{ name: "edge", digest: "digest-of-edge" }],
      },
    });
    // The vault is read at once, with no restart, and the person is told what was skipped.
    expect(await screen.findByRole("table", { name: "Secrets in ops" })).toBeInTheDocument();
    expect(screen.getByText("purlis reads this vault's token from the Keychain.")).toBeVisible();
    expect(screen.getByRole("alert")).toHaveTextContent(
      "How this vault signs in is stored. purlis reads the vault with it from now on, with no restart. edge was not given the token: its settings changed after they were shown here.",
    );
    expect(onChanged).toHaveBeenCalled();
    noValueAnywhere(GIVEN);
    expect(asked.filter((one) => JSON.stringify(one.args).includes(GIVEN))).toHaveLength(1);
  });

  it("after a conversion, asks for the export no vault reads any more to be removed", async () => {
    const bound = onePassword({
      secrets: [],
      count: 0,
      identity: [{ variable: "OP_TEAM_TOKEN", held: "unset", kept: false }],
      refused: { kind: "no-token", why: "vault 'ops' is read through $OP_TEAM_TOKEN." },
    });
    core(bound, {
      vault_setup_begin: { setup: 4, op_vaults: [], listing: null, alike: [] },
      vault_setup_test: { items: 1, item: "charter-ops", item_there: true, failed: null },
      vault_setup_change: {
        contents: onePassword({ identity: kept("keyring") }),
        marked: [],
        skipped: [],
        no_longer_read: ["OP_TEAM_TOKEN"],
        checked_every_project: true,
      },
    });
    draw();

    await userEvent.click(
      await screen.findByRole("button", { name: "Change how this vault signs in" }),
    );
    const dialog = screen.getByRole("dialog", { name: "How ops signs in" });
    await userEvent.type(within(dialog).getByLabelText("Service-account token"), GIVEN);
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));
    await within(dialog).findByText(/purlis has the token for this set-up/);
    await userEvent.click(within(dialog).getByRole("button", { name: "Test" }));
    await within(dialog).findByRole("status");
    await userEvent.click(within(dialog).getByRole("button", { name: "Store" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "How this vault signs in is stored. purlis reads the vault with it from now on, with no restart. No vault of any project this machine opened reads $OP_TEAM_TOKEN any more. If your shell's startup files export it, remove that line: until then every shell started from them, and every program started from such a shell, still carries the token.",
    );
    noValueAnywhere(GIVEN);
  });

  it("speaks for this project alone where another project could not be checked", () => {
    const said = saidOfTheOldExport({
      contents: onePassword({ identity: kept("keyring") }),
      marked: [],
      skipped: [],
      no_longer_read: ["OP_TEAM_TOKEN"],
      checked_every_project: false,
    });
    expect(said).toMatch(
      /^No vault of this project reads \$OP_TEAM_TOKEN any more; purlis could not check/,
    );
    expect(said).not.toMatch(/Keychain/);
  });

  it("offers Store anyway beside the reason when the test did not pass", async () => {
    core(onePassword({ identity: kept("keyring") }), {
      vault_setup_begin: { setup: 4, op_vaults: [], listing: null, alike: [] },
      vault_setup_test: {
        items: 0,
        item: "",
        item_there: false,
        failed: { kind: "try-again", why: "purlis could not reach 1Password." },
      },
    });
    draw();
    await userEvent.click(
      await screen.findByRole("button", { name: "Change how this vault signs in" }),
    );
    const dialog = screen.getByRole("dialog", { name: "How ops signs in" });
    await userEvent.type(within(dialog).getByLabelText("Service-account token"), GIVEN);
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));
    await userEvent.click(await within(dialog).findByRole("button", { name: "Test" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "purlis could not reach 1Password.",
    );
    expect(within(dialog).getByRole("button", { name: "Store anyway" })).toBeEnabled();
    expect(within(dialog).queryByRole("button", { name: "Store" })).not.toBeInTheDocument();
  });

  it("lets go of the token when the change is cancelled", async () => {
    const asked = core(onePassword({ identity: kept("keyring") }), {
      vault_setup_begin: { setup: 5, op_vaults: [], listing: null, alike: [] },
    });
    draw();
    await userEvent.click(
      await screen.findByRole("button", { name: "Change how this vault signs in" }),
    );
    const dialog = screen.getByRole("dialog", { name: "How ops signs in" });
    await userEvent.type(within(dialog).getByLabelText("Service-account token"), GIVEN);
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));
    await within(dialog).findByText(/purlis has the token for this set-up/);

    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(asked.at(-1)).toEqual({ cmd: "vault_setup_cancel", args: { setup: 5 } });
    noValueAnywhere(GIVEN);
  });
});

describe("a vault's tab put back by a launch (#1660)", () => {
  /** The Vaults panel's row for `ops`: what the tab knows of the vault before reading it. */
  function listed(provider: string) {
    return [
      { name: "ops", provider, count: null, health: { ok: true, detail: "listed, not read" } },
    ];
  }

  function drawPutBack(onAsk = vi.fn()) {
    const view = render(
      <VaultTab plane={PLANE} vault="ops" onChanged={vi.fn()} waits onAsk={onAsk} />,
    );
    return { onAsk, view };
  }

  it("reads nothing of a 1Password vault until the person presses to read it", async () => {
    const asked = core(team1p(), { vault_list: listed("1password") });
    const { onAsk, view } = drawPutBack();

    const read = await screen.findByRole("button", { name: "Read ops" });
    expect(screen.getByTestId("vault-waits")).toHaveTextContent(
      "ops was open when purlis last quit",
    );
    // Only the listing, which reads no token and runs no provider.
    expect(asked.map((one) => one.cmd)).toEqual(["vault_list"]);

    await userEvent.click(read);
    expect(onAsk).toHaveBeenCalledOnce();
    view.rerender(<VaultTab plane={PLANE} vault="ops" onChanged={vi.fn()} onAsk={onAsk} />);
    await screen.findByRole("table", { name: "Secrets in ops" });
    expect(asked.map((one) => one.cmd)).toEqual(["vault_list", "vault_open"]);
  });

  it("reads a keyring vault at once: its table is its keys index, and no secret is read", async () => {
    const asked = core(contents([secret("API_TOKEN")]), { vault_list: listed("keyring") });
    drawPutBack();

    await screen.findByRole("table", { name: "Secrets in ops" });
    expect(screen.queryByTestId("vault-waits")).not.toBeInTheDocument();
    expect(asked.map((one) => one.cmd)).toEqual(["vault_list", "vault_open"]);
  });

  it("waits when the listing does not say what the vault is", async () => {
    const asked = core(contents([secret("API_TOKEN")]), { vault_list: [] });
    drawPutBack();

    await screen.findByRole("button", { name: "Read ops" });
    expect(asked.map((one) => one.cmd)).toEqual(["vault_list"]);
  });

  function team1p(): VaultContents {
    return contents([secret("DEPLOY", { size: null, updated: null })], {
      provider: "1password",
      identity: [{ variable: "1password-token", held: "keyring", kept: true }],
    });
  }
});
