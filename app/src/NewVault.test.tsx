import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { userEvent } from "@testing-library/user-event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { NewVault } from "./NewVault";
import type {
  SetupAccounts,
  SetupAlike,
  SetupBegun,
  SetupDone,
  SetupTested,
  VaultContents,
} from "./bindings";

afterEach(() => {
  cleanup();
  clearMocks();
});

const PLANE = "/home/dev/plane";

function draw(over: { trouble?: string; making?: boolean } = {}) {
  const create = vi.fn();
  const made = vi.fn();
  const cancel = vi.fn();
  const drawn = render(
    <NewVault
      plane={PLANE}
      trouble={over.trouble}
      making={over.making ?? false}
      onCreate={create}
      onMade={made}
      onCancel={cancel}
    />,
  );
  return {
    create,
    made,
    cancel,
    unmount: drawn.unmount,
    dialog: screen.getByRole("dialog", { name: "New vault" }),
  };
}

/** A made-up token. It may never be found in the document. */
const TOKEN = "made-up-word-for-the-box-1527";

type Asked = { cmd: string; args: Record<string, unknown> };

/** The core: each command answers what `answers` says for it, or an `Error`'s message as its
 *  refusal. */
function core(answers: Record<string, unknown>): Asked[] {
  const asked: Asked[] = [];
  mockIPC((cmd, args) => {
    asked.push({ cmd, args: args as Record<string, unknown> });
    const answer = answers[cmd];
    if (answer instanceof Error) throw answer.message;
    return answer ?? null;
  });
  return asked;
}

function begun(over: Partial<SetupBegun> = {}): SetupBegun {
  return { setup: 7, op_vaults: ["Engineering", "Ops"], listing: null, alike: [], ...over };
}

function passed(over: Partial<SetupTested> = {}): SetupTested {
  return { items: 2, item: "charter-team", item_there: false, failed: null, ...over };
}

function other(name: string, over: Partial<SetupAlike> = {}): SetupAlike {
  return {
    name,
    op_vault: "Edge",
    op_item: `charter-${name}`,
    account: null,
    persona: null,
    half: "local",
    held: "unset",
    ticked: true,
    digest: `digest-of-${name}`,
    ...over,
  };
}

function done(over: Partial<SetupDone> = {}): SetupDone {
  const contents: VaultContents = {
    name: "team",
    provider: "1password",
    count: 0,
    health: { ok: true, detail: "no secrets yet" },
    secrets: [],
    refused: null,
    identity_unset_elsewhere: [],
    identity: [{ variable: "service-account-token", held: "keyring", kept: true }],
    identity_in_app_env: [],
  };
  return {
    contents,
    marked: [],
    skipped: [],
    no_longer_read: [],
    checked_every_project: true,
    ...over,
  };
}

/** The document holds no token: not in the markup, and not in any field. */
function noTokenAnywhere() {
  expect(document.body.innerHTML).not.toContain(TOKEN);
  for (const field of document.querySelectorAll("input, textarea")) {
    expect((field as HTMLInputElement).value).not.toBe(TOKEN);
  }
}

/** Names the vault, chooses 1Password, and hands the token over. */
async function giveTheToken(dialog: HTMLElement) {
  await userEvent.type(within(dialog).getByLabelText("Name"), "team");
  await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));
  await userEvent.type(within(dialog).getByLabelText("Service-account token"), TOKEN);
  await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));
  await within(dialog).findByText(/purlis has the token for this set-up/);
}

describe("the new-vault dialog", () => {
  it("makes a keyring vault unless another provider is chosen", async () => {
    const { create, dialog } = draw();
    expect(within(dialog).getByRole("radio", { name: /System keychain/ })).toBeChecked();

    await userEvent.type(within(dialog).getByLabelText("Name"), "ops");
    await userEvent.click(within(dialog).getByRole("button", { name: "Create vault" }));

    expect(create).toHaveBeenCalledWith("ops", "keyring", null);
  });

  it("asks a 1Password vault how it signs in before anything else about it", async () => {
    core({});
    const { create, dialog } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "team");
    expect(within(dialog).queryByLabelText("Service-account token")).not.toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));

    // The token is the choice it starts on, and nothing can be made or tested yet.
    expect(within(dialog).getByRole("radio", { name: "A service-account token" })).toBeChecked();
    expect(within(dialog).getByLabelText("Service-account token")).toHaveAttribute(
      "type",
      "password",
    );
    expect(within(dialog).queryByLabelText("1Password vault")).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /Create/ })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Test sign-in" })).not.toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });

  it("says the core's refusal where the name is typed", () => {
    const { dialog } = draw({ trouble: "'../x' is not a vault name purlis accepts" });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("not a vault name");
  });

  it("is drawn from the settings set, each answer tied to its line of help", async () => {
    // DS-3c (#1175): the dialog's form parts are the house set's rows, fields and choices, not
    // the hand-built `asks` / `choice` classes, so the help under a box is the box's own
    // description rather than a paragraph a screen reader cannot connect to it.
    const { dialog } = draw();
    const name = within(dialog).getByLabelText("Name");
    expect(name.closest(".ui-setting-row")).not.toBeNull();
    expect(name).toHaveAccessibleDescription("Letters, digits, ., _ and -.");

    const kept = within(dialog).getByRole("radiogroup", { name: "Kept in" });
    expect(kept.closest(".ui-setting-row")).not.toBeNull();
    expect(within(dialog).getByRole("radio", { name: "Plain file" })).toHaveAccessibleDescription(
      "A plaintext file under the project's state directory, which git never sees.",
    );

    core({});
    await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));
    const how = within(dialog).getByRole("radiogroup", { name: "Signs in with" });
    expect(how.closest(".ui-setting-row")).not.toBeNull();
    expect(within(dialog).getByLabelText("Service-account token")).toHaveAccessibleDescription(
      "It goes to purlis once and from there into your system keychain. It is not kept on this page.",
    );
  });

  it("puts the keyboard in the name box, and Escape makes nothing", async () => {
    const { create, cancel, dialog } = draw();
    expect(within(dialog).getByLabelText("Name")).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(cancel).toHaveBeenCalled();
    expect(create).not.toHaveBeenCalled();
  });
});

describe("setting up how a 1Password vault signs in", () => {
  it("hands the token to the core once, and the page never holds it", async () => {
    const asked = core({ vault_setup_begin: begun() });
    const { dialog } = draw();

    await giveTheToken(dialog);

    expect(asked.at(-1)).toEqual({
      cmd: "vault_setup_begin",
      args: { plane: PLANE, token: TOKEN, account: null, vault: null },
    });
    noTokenAnywhere();
    // The box is gone: what the page holds from here on is the set-up's number.
    expect(within(dialog).queryByLabelText("Service-account token")).not.toBeInTheDocument();
    expect(JSON.stringify(asked.slice(0, -1))).not.toContain(TOKEN);
  });

  it("gives the token on Enter too, and makes nothing by it", async () => {
    const asked = core({ vault_setup_begin: begun() });
    const { create, dialog } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "team");
    await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));

    await userEvent.type(within(dialog).getByLabelText("Service-account token"), `${TOKEN}{Enter}`);

    await within(dialog).findByText(/purlis has the token for this set-up/);
    expect(asked.map((one) => one.cmd)).toEqual(["vault_setup_begin"]);
    expect(create).not.toHaveBeenCalled();
    noTokenAnywhere();
  });

  it("says the core's refusal of a token, with the box emptied", async () => {
    core({ vault_setup_begin: new Error("what was given holds a space or a line break") });
    const { dialog } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "team");
    await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));
    await userEvent.type(within(dialog).getByLabelText("Service-account token"), TOKEN);
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("holds a space");
    noTokenAnywhere();
    expect(within(dialog).getByLabelText("Service-account token")).toHaveValue("");
  });

  it("offers the 1Password vaults the sign-in can see, and the item's default", async () => {
    core({ vault_setup_begin: begun() });
    const { dialog } = draw();
    await giveTheToken(dialog);

    const pick = within(dialog).getByLabelText("1Password vault");
    expect(
      within(pick)
        .getAllByRole("option")
        .map((one) => one.textContent),
    ).toEqual(["Choose a vault", "Engineering", "Ops"]);
    expect(within(dialog).getByLabelText("Item")).toHaveAccessibleDescription(
      "The one item whose fields are this vault's secrets. Empty for charter-team.",
    );
    // No vault chosen yet: there is nothing to test.
    expect(within(dialog).getByRole("button", { name: "Test sign-in" })).toBeDisabled();
  });

  it("has the vault typed where the sign-in may not list them, and says why", async () => {
    core({
      vault_setup_begin: begun({
        op_vaults: [],
        listing: { kind: "try-again", why: "purlis could not reach 1Password." },
      }),
    });
    const { dialog } = draw();
    await giveTheToken(dialog);

    const typed = within(dialog).getByLabelText("1Password vault");
    expect(typed.tagName).toBe("INPUT");
    expect(typed).toHaveAccessibleDescription(
      "purlis could not reach 1Password. Type the vault's name to go on.",
    );
  });

  it("tests before anything is made, and offers Create only once a test passed", async () => {
    const asked = core({
      vault_setup_begin: begun(),
      vault_setup_test: passed(),
      vault_setup_create: done(),
    });
    const { made, dialog } = draw();
    await giveTheToken(dialog);
    await userEvent.selectOptions(within(dialog).getByLabelText("1Password vault"), "Engineering");
    expect(within(dialog).queryByRole("button", { name: /Create/ })).not.toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("button", { name: "Test sign-in" }));

    expect(await within(dialog).findByRole("status")).toHaveTextContent(
      "Signed in. 2 items in that 1Password vault; the item charter-team will be made with the first secret. No value was read.",
    );
    expect(asked.at(-1)).toEqual({
      cmd: "vault_setup_test",
      args: { plane: PLANE, setup: 7, vault: "team", opVault: "Engineering", opItem: null },
    });
    expect(asked.map((one) => one.cmd)).not.toContain("vault_setup_create");

    await userEvent.click(within(dialog).getByRole("button", { name: "Create vault" }));

    await waitFor(() => expect(made).toHaveBeenCalledWith(done()));
    expect(asked.at(-1)).toEqual({
      cmd: "vault_setup_create",
      args: {
        plane: PLANE,
        setup: 7,
        place: { vault: "team", op_vault: "Engineering", op_item: null },
        also: [],
      },
    });
    // The token crossed once, at the first step.
    expect(asked.filter((one) => JSON.stringify(one.args).includes(TOKEN))).toHaveLength(1);
    noTokenAnywhere();
  });

  it.each([
    ["program", "purlis could not find the 1Password CLI ('op')."],
    ["try-again", "purlis could not reach 1Password: this machine has no network."],
    ["sign-in", "1Password refused the sign-in with this token."],
    ["other", "1Password answered, and has no vault of that name for this sign-in."],
  ] as const)(
    "says why a test did not pass (%s) and offers Create anyway beside it",
    async (kind, why) => {
      const asked = core({
        vault_setup_begin: begun(),
        vault_setup_test: passed({ items: 0, item: "", failed: { kind, why } }),
        vault_setup_create: done(),
      });
      const { made, dialog } = draw();
      await giveTheToken(dialog);
      await userEvent.selectOptions(within(dialog).getByLabelText("1Password vault"), "Ops");
      await userEvent.click(within(dialog).getByRole("button", { name: "Test sign-in" }));

      expect(await within(dialog).findByRole("alert")).toHaveTextContent(why);
      expect(within(dialog).queryByRole("status")).not.toBeInTheDocument();
      expect(
        within(dialog).queryByRole("button", { name: "Create vault" }),
      ).not.toBeInTheDocument();
      expect(within(dialog).getByRole("button", { name: "Test sign-in again" })).toBeEnabled();

      await userEvent.click(within(dialog).getByRole("button", { name: "Create anyway" }));
      await waitFor(() => expect(made).toHaveBeenCalled());
      expect(asked.at(-1)?.cmd).toBe("vault_setup_create");
    },
  );

  it("asks for the test again when what was tested is changed", async () => {
    core({ vault_setup_begin: begun(), vault_setup_test: passed() });
    const { dialog } = draw();
    await giveTheToken(dialog);
    await userEvent.selectOptions(within(dialog).getByLabelText("1Password vault"), "Engineering");
    await userEvent.click(within(dialog).getByRole("button", { name: "Test sign-in" }));
    await within(dialog).findByRole("button", { name: "Create vault" });

    await userEvent.selectOptions(within(dialog).getByLabelText("1Password vault"), "Ops");

    expect(within(dialog).queryByRole("button", { name: /Create/ })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("status")).not.toBeInTheDocument();
  });

  it("says the core's refusal of the create and keeps the set-up for another try", async () => {
    const asked = core({
      vault_setup_begin: begun(),
      vault_setup_test: passed(),
      vault_setup_create: new Error("vault 'team' is already registered."),
    });
    const { made, dialog } = draw();
    await giveTheToken(dialog);
    await userEvent.selectOptions(within(dialog).getByLabelText("1Password vault"), "Engineering");
    await userEvent.click(within(dialog).getByRole("button", { name: "Test sign-in" }));
    await userEvent.click(await within(dialog).findByRole("button", { name: "Create vault" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("already registered");
    expect(made).not.toHaveBeenCalled();
    expect(asked.map((one) => one.cmd)).not.toContain("vault_setup_cancel");
    expect(within(dialog).getByRole("button", { name: "Create vault" })).toBeEnabled();
  });

  it("lists the other vaults with what would be pinned, a committed one unticked", async () => {
    const asked = core({
      vault_setup_begin: begun({
        alike: [
          other("edge"),
          other("pulled", {
            op_vault: "Pulled",
            account: "acme.1password.eu",
            persona: "devops",
            half: "shared",
            ticked: false,
          }),
          other("kept", { half: "both", held: "keyring", ticked: false }),
        ],
      }),
      vault_setup_test: passed(),
      vault_setup_create: done({ marked: ["edge", "pulled"] }),
    });
    const { dialog } = draw();
    await giveTheToken(dialog);

    const list = within(dialog).getByRole("group", { name: "Also use this token for" });
    expect(within(list).getByRole("checkbox", { name: "edge" })).toBeChecked();
    expect(within(list).getByRole("checkbox", { name: "pulled" })).not.toBeChecked();
    expect(within(list).getByRole("checkbox", { name: "kept" })).not.toBeChecked();
    expect(within(list).getByRole("checkbox", { name: "pulled" })).toHaveAccessibleDescription(
      "1Password vault Pulled, item charter-pulled, account acme.1password.eu, for persona devops; named for the whole project; has no token yet.",
    );
    expect(within(list).getByRole("checkbox", { name: "kept" })).toHaveAccessibleDescription(
      "1Password vault Edge, item charter-kept; named for the whole project and on this machine; has a token in your system keychain now, which this one would replace.",
    );

    // The person ticks the committed one. What goes to the core is each ticked name with the
    // digest of what was shown for it, and nothing for the one left unticked.
    await userEvent.click(within(list).getByRole("checkbox", { name: "pulled" }));
    await userEvent.selectOptions(within(dialog).getByLabelText("1Password vault"), "Engineering");
    await userEvent.click(within(dialog).getByRole("button", { name: "Test sign-in" }));
    await userEvent.click(await within(dialog).findByRole("button", { name: "Create vault" }));

    await waitFor(() => expect(asked.at(-1)?.cmd).toBe("vault_setup_create"));
    expect(asked.at(-1)?.args.also).toEqual([
      { name: "edge", digest: "digest-of-edge" },
      { name: "pulled", digest: "digest-of-pulled" },
    ]);
  });

  it("signs in through the 1Password app with an account it lists", async () => {
    const accounts: SetupAccounts = {
      accounts: [
        { address: "my.1password.com", email: "a@example.test", pin: "my.1password.com" },
        { address: "acme.1password.eu", email: "b@example.test", pin: "acme.1password.eu" },
      ],
      failed: null,
    };
    const asked = core({ vault_setup_accounts: accounts, vault_setup_begin: begun() });
    const { dialog } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "mine");
    await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));

    await userEvent.click(
      within(dialog).getByRole("radio", { name: "The 1Password app on this machine" }),
    );

    // No box for a token: purlis is given no credential this way.
    expect(within(dialog).queryByLabelText("Service-account token")).not.toBeInTheDocument();
    const pick = await within(dialog).findByLabelText("Account");
    expect(
      within(pick)
        .getAllByRole("option")
        .map((one) => one.textContent),
    ).toEqual([
      "Type a sign-in address instead",
      "my.1password.com (a@example.test)",
      "acme.1password.eu (b@example.test)",
    ]);
    await userEvent.selectOptions(pick, "acme.1password.eu");
    expect(within(dialog).queryByLabelText("Sign-in address")).not.toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this account" }));

    await within(dialog).findByText(/signs in through the 1Password app/);
    expect(asked.at(-1)).toEqual({
      cmd: "vault_setup_begin",
      args: { plane: PLANE, token: null, account: "acme.1password.eu", vault: null },
    });
  });

  it("takes a typed sign-in address where the app's accounts cannot be listed", async () => {
    const asked = core({
      vault_setup_accounts: {
        accounts: [],
        failed: { kind: "sign-in", why: "1Password refused the sign-in through its app." },
      } satisfies SetupAccounts,
      vault_setup_begin: begun(),
    });
    const { dialog } = draw();
    await userEvent.type(within(dialog).getByLabelText("Name"), "mine");
    await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));
    await userEvent.click(
      within(dialog).getByRole("radio", { name: "The 1Password app on this machine" }),
    );

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "1Password refused the sign-in through its app.",
    );
    await userEvent.type(within(dialog).getByLabelText("Sign-in address"), "team.1password.ca");
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this account" }));

    await within(dialog).findByText(/signs in through the 1Password app/);
    expect(asked.at(-1)?.args).toEqual({
      plane: PLANE,
      token: null,
      account: "team.1password.ca",
      vault: null,
    });
  });

  it("tells the core to let go of the token when the dialog goes or another is given", async () => {
    const asked = core({ vault_setup_begin: begun() });
    const { dialog, unmount } = draw();
    await giveTheToken(dialog);

    await userEvent.click(within(dialog).getByRole("button", { name: "Give another token" }));
    expect(asked.at(-1)).toEqual({ cmd: "vault_setup_cancel", args: { setup: 7 } });
    expect(within(dialog).getByLabelText("Service-account token")).toHaveValue("");

    await userEvent.type(within(dialog).getByLabelText("Service-account token"), TOKEN);
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));
    await within(dialog).findByText(/purlis has the token for this set-up/);
    unmount();
    expect(asked.at(-1)).toEqual({ cmd: "vault_setup_cancel", args: { setup: 7 } });
  });

  it("makes nothing while the vault has no name", async () => {
    core({ vault_setup_begin: begun(), vault_setup_test: passed() });
    const { dialog } = draw();
    await userEvent.click(within(dialog).getByRole("radio", { name: "1Password" }));
    await userEvent.type(within(dialog).getByLabelText("Service-account token"), TOKEN);
    await userEvent.click(within(dialog).getByRole("button", { name: "Use this token" }));
    await userEvent.selectOptions(
      await within(dialog).findByLabelText("1Password vault"),
      "Engineering",
    );
    expect(within(dialog).getByRole("button", { name: "Test sign-in" })).toBeDisabled();
  });

  it("asks for the test again when the new vault's name changes after it passed", async () => {
    // The test checked the item the name gives by default, `charter-<name>`.
    const asked = core({ vault_setup_begin: begun(), vault_setup_test: passed() });
    const { dialog } = draw();
    await giveTheToken(dialog);
    await userEvent.selectOptions(within(dialog).getByLabelText("1Password vault"), "Engineering");
    await userEvent.click(within(dialog).getByRole("button", { name: "Test sign-in" }));
    await within(dialog).findByRole("button", { name: "Create vault" });

    await userEvent.type(within(dialog).getByLabelText("Name"), "-2");

    expect(within(dialog).queryByRole("button", { name: /Create/ })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("status")).not.toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Test sign-in" })).toBeEnabled();
    await userEvent.click(within(dialog).getByRole("button", { name: "Test sign-in" }));
    await waitFor(() => expect(asked.at(-1)?.args.vault).toBe("team-2"));
  });

  it("says a sign-in refused while its vaults were listed, and asks for no vault", async () => {
    const asked = core({
      vault_setup_begin: begun({
        op_vaults: [],
        listing: { kind: "sign-in", why: "1Password refused the sign-in with this token." },
      }),
    });
    const { dialog } = draw();
    await giveTheToken(dialog);

    expect(within(dialog).getByRole("alert")).toHaveTextContent(
      "1Password refused the sign-in with this token.",
    );
    expect(within(dialog).queryByLabelText("1Password vault")).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /Test/ })).not.toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: /Create/ })).not.toBeInTheDocument();
    // The way on is another token.
    await userEvent.click(within(dialog).getByRole("button", { name: "Give another token" }));
    expect(asked.at(-1)?.cmd).toBe("vault_setup_cancel");
    expect(within(dialog).getByLabelText("Service-account token")).toBeInTheDocument();
  });

  it("says a listing that failed otherwise in its own words, and has the name typed", async () => {
    const why =
      "purlis could not list the 1Password vaults this sign-in can see (op exit 1), and did not recognise why.";
    core({ vault_setup_begin: begun({ op_vaults: [], listing: { kind: "other", why } }) });
    const { dialog } = draw();
    await giveTheToken(dialog);
    expect(within(dialog).getByLabelText("1Password vault")).toHaveAccessibleDescription(why);
  });
});
