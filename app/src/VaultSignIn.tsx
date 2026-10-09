import { useEffect, useRef, useState } from "react";
import {
  commands,
  type PlaneId,
  type SetupAccounts,
  type SetupAlike,
  type SetupBegun,
  type SetupDone,
  type SetupTested,
} from "./bindings";
import { Choice, Field, SettingActions, SettingRow } from "./settings/components";

/** A command's answer, with a call that never reached the core answered as its refusal. */
function settled<T>(
  call: Promise<{ status: "ok"; data: T } | { status: "error"; error: string }>,
): Promise<{ status: "ok"; data: T } | { status: "error"; error: string }> {
  return call.catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
}

/** Which half of the registry names a vault, as the list says it. */
const NAMED_BY: Record<string, string> = {
  local: "named by this machine's registry",
  shared: "named by the committed vaults.json",
  both: "named by the committed vaults.json and this machine's registry",
};

/** Where another vault's token is now, as the list says it. */
const HELD: Record<SetupAlike["held"], string> = {
  keyring: "has a token in the Keychain now, which this one would replace",
  environment: "its token is in purlis's environment now",
  unset: "has no token yet",
};

/**
 * **How a 1Password vault signs in, asked, tested and stored** (#1527): the guided part of *New
 * vault*, and the whole of *Change how this vault signs in* on a vault's tab.
 *
 * 1. **How purlis signs in.** A service-account token, in a password box, or the 1Password app
 *    on this machine with the account chosen from what `op` lists or its sign-in address typed.
 * 2. **Where the items live** (a new vault only): the 1Password vault, picked from the ones that
 *    sign-in can see, or typed where it may not list them; and the item.
 * 3. **Test**: the core signs in and reads item names. What it says is the core's own sentence,
 *    by kind, never the provider program's words.
 * 4. **Create** (or **Store**, for a vault that exists) is offered after a test passed, and as
 *    "anyway" beside the reason after one that did not.
 *
 * **The token is handed over once and is never on the page.** The box is uncontrolled, read
 * from the element at the press and emptied there, exactly as the vault tab's is; the core then
 * holds the token for this set-up (`vault_setup_begin`) and the page holds a number. Nothing in
 * React state, no attribute and no answer carries it. Leaving the set-up tells the core to let
 * go of it.
 *
 * **Other vaults the token may be used for are listed, never assumed.** Each row says what
 * would be pinned for that vault and which half of the registry names it; a vault the committed
 * half names starts unticked. The ticked names go to the core with the digest of what was shown,
 * and the core gives the token only to those that still match.
 */
export function VaultSignIn({
  plane,
  vault,
  name,
  nameTrouble,
  onBusy,
  onDone,
  onCancel,
}: {
  plane: PlaneId;
  /** The registered vault whose sign-in is being changed. Not set for a new vault. */
  vault?: string;
  /** The new vault's name, as typed so far. */
  name: string;
  /** Why the name will not do yet: nothing is tested or made while it is said. */
  nameTrouble?: string;
  /** Whether the core is at work, so the dialog around this does not close under it. */
  onBusy?: (busy: boolean) => void;
  onDone: (done: SetupDone) => void;
  onCancel: () => void;
}) {
  const existing = vault !== undefined;
  const [how, setHow] = useState<"token" | "app">("token");
  const [accounts, setAccounts] = useState<SetupAccounts>();
  const [account, setAccount] = useState("");
  const [address, setAddress] = useState("");
  const [begun, setBegun] = useState<SetupBegun>();
  const [opVault, setOpVault] = useState("");
  const [opItem, setOpItem] = useState("");
  const [ticked, setTicked] = useState<ReadonlySet<string>>(new Set());
  /** The last test's answer, and the name it was for: a test of a new vault checked its
   *  default item, `charter-<name>`, so it says nothing once the name has changed. */
  const [testedAs, setTestedAs] = useState<{ name: string; said: SetupTested }>();
  const tested =
    testedAs !== undefined && (existing || testedAs.name === name.trim())
      ? testedAs.said
      : undefined;
  const setTested = (said: SetupTested | undefined) =>
    setTestedAs(said === undefined ? undefined : { name: name.trim(), said });
  const [trouble, setTrouble] = useState<string>();
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLInputElement>(null);
  /** The set-up the core holds for this panel, to let go of when the panel goes. */
  const held = useRef<number | undefined>(undefined);

  const working = (on: boolean) => {
    setBusy(on);
    onBusy?.(on);
  };

  useEffect(
    () => () => {
      if (held.current !== undefined) void commands.vaultSetupCancel(held.current).catch(() => {});
    },
    [],
  );

  /** Start over from the first step: the core lets go of what it was given. */
  const again = () => {
    if (held.current !== undefined) void commands.vaultSetupCancel(held.current).catch(() => {});
    held.current = undefined;
    setBegun(undefined);
    setTested(undefined);
    setTrouble(undefined);
    setOpVault("");
    setTicked(new Set());
  };

  const choose = (to: string) => {
    again();
    setHow(to === "app" ? "app" : "token");
    if (to === "app" && accounts === undefined) {
      void settled(commands.vaultSetupAccounts(plane)).then((answer) => {
        if (answer.status === "ok") setAccounts(answer.data);
        else setAccounts({ accounts: [], failed: { kind: "other", why: answer.error } });
      });
    }
  };

  const begin = async (token: string | null, pinned: string | null) => {
    working(true);
    setTrouble(undefined);
    const answer = await settled(commands.vaultSetupBegin(plane, token, pinned, vault ?? null));
    working(false);
    if (answer.status === "error") {
      setTrouble(answer.error);
      return;
    }
    held.current = answer.data.setup;
    setBegun(answer.data);
    setTested(undefined);
    setOpVault("");
    setTicked(new Set(answer.data.alike.filter((one) => one.ticked).map((one) => one.name)));
  };

  /** Read the token from the box once, empty the box, and hand the token to the core. */
  const give = () => {
    const token = box.current?.value ?? "";
    if (box.current) box.current.value = "";
    if (token.trim() !== "") void begin(token, null);
  };

  /** The item typed, or null for the vault's own default (and for a vault that exists). */
  const item = () => (existing || opItem.trim() === "" ? null : opItem.trim());

  const test = async () => {
    if (begun === undefined) return;
    working(true);
    setTrouble(undefined);
    const answer = await settled(
      commands.vaultSetupTest(
        plane,
        begun.setup,
        vault ?? name.trim(),
        existing ? null : opVault.trim(),
        item(),
      ),
    );
    working(false);
    if (answer.status === "error") {
      setTrouble(answer.error);
      setTested(undefined);
      return;
    }
    setTested(answer.data);
  };

  const store = async () => {
    if (begun === undefined) return;
    working(true);
    setTrouble(undefined);
    const also = begun.alike
      .filter((one) => ticked.has(one.name))
      .map((one) => ({ name: one.name, digest: one.digest }));
    const answer = await settled(
      existing
        ? commands.vaultSetupChange(plane, begun.setup, vault, also)
        : commands.vaultSetupCreate(
            plane,
            begun.setup,
            { vault: name.trim(), op_vault: opVault.trim(), op_item: item() },
            also,
          ),
    );
    working(false);
    if (answer.status === "error") {
      setTrouble(answer.error);
      return;
    }
    held.current = undefined;
    onDone(answer.data);
  };

  /** The sign-in was refused already, while its vaults were listed: there is nothing to test
   *  or store with it, and the person is told so then, not after typing a vault's name. */
  const refusedAtSignIn = begun?.listing?.kind === "sign-in";
  const named = existing || (name.trim() !== "" && nameTrouble === undefined);
  const placed = existing || opVault.trim() !== "";
  const ready = begun !== undefined && !refusedAtSignIn && named && placed && !busy;
  const pinned = account !== "" ? account : address.trim();

  return (
    <>
      <SettingRow
        label="Signs in with"
        grouped
        control={(ids) => (
          <Choice
            kind="radio"
            ids={ids}
            disabled={busy}
            options={[
              {
                value: "token",
                label: "A service-account token",
                says: "For a vault agents use. Pasted once, kept in the Keychain, and never in a shell or a file.",
              },
              {
                value: "app",
                label: "The 1Password app on this machine",
                says: "Through the app's command-line integration and its own unlock. purlis is given no credential.",
              },
            ]}
            value={how}
            onValueChange={choose}
          />
        )}
      />

      {begun === undefined && how === "token" && (
        <SettingRow
          label="Service-account token"
          help="It goes to purlis once and from there into the Keychain. It is not kept on this page."
          control={(ids) => (
            <>
              <input
                ref={box}
                id={ids.id}
                className="ui-field"
                type="password"
                placeholder="Paste the token"
                autoComplete="off"
                spellCheck={false}
                tabIndex={0}
                disabled={busy}
                aria-describedby={ids.describedBy}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    give();
                  }
                }}
              />
              <button type="button" tabIndex={0} disabled={busy} onClick={give}>
                Use this token
              </button>
            </>
          )}
        />
      )}

      {begun === undefined && how === "app" && (
        <>
          {accounts === undefined ? (
            <p className="pending" aria-busy="true">
              Asking the 1Password app for its accounts…
            </p>
          ) : accounts.accounts.length > 0 ? (
            <SettingRow
              label="Account"
              help="The accounts the 1Password app is signed in to. Pinned for this machine only."
              control={(ids) => (
                <Choice
                  kind="select"
                  ids={ids}
                  unset="Type a sign-in address instead"
                  options={accounts.accounts.map((one) => ({
                    value: one.pin,
                    label: one.email === "" ? one.address : `${one.address} (${one.email})`,
                  }))}
                  value={account}
                  onValueChange={setAccount}
                />
              )}
            />
          ) : (
            accounts.failed && (
              <p className="trouble" role="alert">
                {accounts.failed.why}
              </p>
            )
          )}
          {account === "" && (
            <SettingRow
              label="Sign-in address"
              help={
                <>
                  As in <code>my.1password.com</code>, a regional address such as{" "}
                  <code>acme.1password.eu</code>, or your company's own. Leave it empty to use the
                  app's only account.
                </>
              }
              control={(ids) => (
                <Field kind="text" ids={ids} value={address} onChange={setAddress} />
              )}
            />
          )}
          <SettingActions>
            <button
              type="button"
              tabIndex={0}
              disabled={busy}
              onClick={() => void begin(null, pinned === "" ? null : pinned)}
            >
              Use this account
            </button>
          </SettingActions>
        </>
      )}

      {begun !== undefined && (
        <p className="vault-signin-given">
          {how === "token"
            ? "purlis has the token for this set-up, and it is not on this page."
            : "purlis signs in through the 1Password app for this set-up."}{" "}
          <button type="button" className="panel-view" tabIndex={0} disabled={busy} onClick={again}>
            {how === "token" ? "Give another token" : "Choose another account"}
          </button>
        </p>
      )}

      {begun?.listing && refusedAtSignIn && (
        <p className="trouble" role="alert">
          {begun.listing.why}
        </p>
      )}

      {begun !== undefined && !existing && !refusedAtSignIn && (
        <>
          {begun.op_vaults.length > 0 ? (
            <SettingRow
              label="1Password vault"
              help="Where purlis creates this vault's items: the vaults this sign-in can see."
              control={(ids) => (
                <Choice
                  kind="select"
                  ids={ids}
                  unset="Choose a vault"
                  options={begun.op_vaults.map((one) => ({ value: one, label: one }))}
                  value={opVault}
                  onValueChange={(to) => {
                    setOpVault(to);
                    setTested(undefined);
                  }}
                />
              )}
            />
          ) : (
            <SettingRow
              label="1Password vault"
              help={
                begun.listing
                  ? begun.listing.kind === "other"
                    ? begun.listing.why
                    : `${begun.listing.why} Type the vault's name to go on.`
                  : "This sign-in lists no vault, so type the name."
              }
              control={(ids) => (
                <Field
                  kind="text"
                  ids={ids}
                  value={opVault}
                  onChange={(to) => {
                    setOpVault(to);
                    setTested(undefined);
                  }}
                />
              )}
            />
          )}
          <SettingRow
            label="Item"
            help={
              <>
                The one item whose fields are this vault's secrets. Empty for{" "}
                <code>charter-{name.trim() === "" ? "<name>" : name.trim()}</code>.
              </>
            }
            control={(ids) => (
              <Field
                kind="text"
                ids={ids}
                value={opItem}
                onChange={(to) => {
                  setOpItem(to);
                  setTested(undefined);
                }}
              />
            )}
          />
        </>
      )}

      {begun !== undefined && !refusedAtSignIn && begun.alike.length > 0 && (
        <SettingRow
          label="Also use this token for"
          grouped
          help="Each ticked vault gets its own Keychain item, pinned to the settings shown here. One whose settings change before the token is stored is skipped."
          control={(ids) => (
            <Choice
              kind="checks"
              ids={ids}
              options={begun.alike.map((one) => ({
                value: one.name,
                label: one.name,
                says: `1Password vault ${one.op_vault}, item ${one.op_item}${
                  one.account ? `, account ${one.account}` : ""
                }${one.persona ? `, for persona ${one.persona}` : ""}; ${
                  NAMED_BY[one.half] ?? one.half
                }; ${HELD[one.held]}.`,
              }))}
              checked={ticked}
              onCheckedChange={(which, on) =>
                setTicked((was) => {
                  const next = new Set(was);
                  if (on) next.add(which);
                  else next.delete(which);
                  return next;
                })
              }
            />
          )}
        />
      )}

      {tested !== undefined &&
        (tested.failed ? (
          <p className="trouble" role="alert">
            {tested.failed.why}
          </p>
        ) : (
          <p className="vault-signin-passed" role="status">
            {`Signed in. ${tested.items === 1 ? "1 item" : `${tested.items} items`} in that 1Password vault; the item ${tested.item} ${
              tested.item_there ? "is there" : "will be made with the first secret"
            }. No value was read.`}
          </p>
        ))}

      {(trouble ?? nameTrouble) && (
        <p className="trouble" role="alert">
          {trouble ?? nameTrouble}
        </p>
      )}

      <SettingActions>
        {begun !== undefined && !refusedAtSignIn && (
          <button type="button" tabIndex={0} disabled={!ready} onClick={() => void test()}>
            {tested === undefined ? "Test" : "Test again"}
          </button>
        )}
        {begun !== undefined && tested !== undefined && (
          <button type="button" tabIndex={0} disabled={!ready} onClick={() => void store()}>
            {existing
              ? tested.failed
                ? "Store anyway"
                : "Store"
              : tested.failed
                ? "Create anyway"
                : "Create vault"}
          </button>
        )}
        <button type="button" tabIndex={0} disabled={busy} onClick={onCancel}>
          Cancel
        </button>
      </SettingActions>
    </>
  );
}

/**
 * What a finished conversion says of the variables the vault was read through before that no
 * vault reads now (#1542): the export left in a shell profile is the one copy of the token
 * outside the keyring. Empty when there are none.
 */
export function saidOfTheOldExport(done: SetupDone): string {
  const names = done.no_longer_read.map((name) => `$${name}`);
  if (names.length === 0) return "";
  const listed = names.join(", ");
  return `No vault reads ${listed} any more. If your shell's startup files export it, remove that line: until then every shell started from them carries the token outside the Keychain.`;
}

/** What a finished set-up says of the other vaults ticked: which got the token, which not. */
export function saidOfTheOthers(done: SetupDone): string {
  const parts: string[] = [];
  if (done.marked.length > 0) parts.push(`Also stored for ${done.marked.join(", ")}.`);
  for (const one of done.skipped) {
    parts.push(
      one.why === "changed"
        ? `${one.name} was not given the token: its settings changed after they were shown here. Open its tab to look, and give the token there.`
        : one.why === "gone"
          ? `${one.name} was not given the token: it is no longer registered that way.`
          : `${one.name} was not given the token: ${one.said ?? "the Keychain refused."}`,
    );
  }
  return parts.join(" ");
}
