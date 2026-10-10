import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode, type Ref } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Dialog from "@radix-ui/react-dialog";
import * as Menu from "@radix-ui/react-dropdown-menu";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { Ellipsis, Eye, EyeOff, KeyRound, LoaderCircle, Plus, Search } from "lucide-react";
import { EmptyState } from "./EmptyState";
import {
  commands,
  type PlaneId,
  type UnreadFor,
  type VaultContents,
  type VaultIdentity,
  type VaultSecret,
} from "./bindings";
import { useTabStop } from "./roving";
import { Field, SettingActions, SettingRow } from "./settings/components";
import { counted } from "./Vaults";
import { saidOfTheOldExport, saidOfTheOthers, VaultSignIn } from "./VaultSignIn";
import { AnswerBar } from "./AnswerBar";
import { providerName } from "./vaultProviders";

/**
 * **One vault, in a tab of its own** (charter-app#235): its name, its provider and how many
 * secrets it holds, a search box, **Add**, and a table of NAME / SIZE / UPDATED whose rows each
 * have a menu — Edit value, Rename, Copy, Delete — and an eye that reveals the value.
 *
 * A view like the persona's (`tabs.ts`: `{ from: null, view: "vault", key: <name> }`), opened
 * by the one `open_view` path the palette, the Vaults panel and a relaunch all take — but drawn
 * by this component rather than from panel blocks, because a table the operator writes to is
 * not something the panel vocabulary has words for, and should not grow them.
 *
 * **No value is in the document unless the operator asked to see it.** Nothing this tab reads
 * carries one: `vault_open`, every write and a copy answer with names, size bands and times, and
 * `vaults.rs`'s test serializes every answer looking for the values it wrote. A value comes in
 * two ways only:
 *
 * - **the eye** (charter-app#236): `vault_secret_reveal` answers with that one value, which is
 *   shown for {@link SHOWN_FOR_MS} and then dropped from state and the page. Pressing the eye
 *   again, or Escape, drops it sooner; revealing another drops it first; so does any write.
 * - **a box the operator types into**, which is **uncontrolled**: React writes a controlled
 *   input's value into its `value` attribute, which is markup — a copy of the page, a devtools
 *   snapshot, an accessibility dump would all carry it. Read from the element once, at the press,
 *   and emptied there.
 *
 * **Copy never brings the value here.** The core reads it, puts it on the clipboard itself, and
 * a minute later clears the clipboard if it still holds it (`vaults.rs`, `clear_later`). The
 * timer is the core's, so a tab or a window that closes within the minute leaves nothing behind.
 *
 * **A 1Password token moves into your system keychain from here** (charter-app#237). Where the vault is
 * read through an identity variable charter's environment carries (`$OP_TEAM_TOKEN`), the header
 * offers "Move this token into your system keychain": `vault_identity_move` reads the token in the core,
 * stores it in the keyring and answers with the vault's names. The token never comes here, and
 * no chat carries an `OP_*` variable, so after the move the keyring is where every `charter
 * secret` finds it.
 *
 * **A vault whose contents could not be read still says whose token it needs** (#1526). Reading
 * a 1Password vault takes its token, so the tab of a vault whose token is nowhere is the one tab
 * that must draw the box that stores it: the core answers such a vault with `refused` (why, and
 * what kind of failure), no secrets and its identity all the same, and the tab draws the
 * refusal, the box and nothing that would read as an empty vault. A stored token's answer is the
 * vault read again, so the table follows by itself. What is said beside the box follows the
 * kind: a program that is missing, a network that is down and a refused sign-in are not the
 * same advice, and only the last is the token's fault.
 *
 * **A token is stored for this vault alone.** Other vaults read through the same variable that
 * still have none are named under it, each a link to its own tab, where its token is put in.
 * A tab already open hears of a store made in another ({@link STORED}) and reads again, so
 * what it points at is not stale.
 *
 * **How a 1Password vault signs in is changed from here** (#1527): *Change how this vault signs
 * in* opens {@link VaultSignIn} for this vault, which is also how a vault bound to an environment
 * variable comes to keep its token in your system keychain. The token is handed to the core there and is
 * never in this tab; the answer is the vault read again, with no restart.
 *
 * **A tab a launch put back reads nothing until the person presses for it** (#1660), where
 * reading the vault reads more than files: a 1Password vault's table is `op`'s answer, which
 * takes the vault's token — a Keychain read macOS may ask about — and `op` may read the
 * 1Password app's own data. A tab nobody opened at this launch must not make macOS ask
 * anything, so it waits ({@link Waiting}) and reads on *Read*, as an extension's put-back view
 * asks its program on a press. What the vault is comes from `vault_list`, which reads no secret
 * and runs no provider; a vault read from files alone ({@link READ_FROM_FILES}) is read at once,
 * and one the listing does not name waits.
 *
 * **Every write answers with the vault as it now is**, so the table is redrawn from the core's
 * answer and never patched by hand here; `onChanged` tells the window, whose Vaults panel counts
 * the secrets too.
 */
export function VaultTab({
  plane,
  vault,
  onChanged,
  onOpenVault,
  actions,
  waits = false,
  onAsk,
}: {
  plane: PlaneId;
  vault: string;
  /** Put back by a launch and not pressed for yet (`tabs.Content.waits`, #1660). */
  waits?: boolean;
  /** The person pressed *Read* on a tab that waits: the window stops it waiting. */
  onAsk?: () => void;
  /** Open another vault's tab: what a vault named as still having no token is a link to. */
  onOpenVault?: (vault: string) => void;
  /** A write changed the vault: the window reads its vault list again. */
  onChanged: () => void;
  /** Buttons for the heading — Delete vault… (SI-3), the catalogue's row drawn by the view. */
  actions?: ReactNode;
}) {
  const [said, setSaid] = useState<{ contents?: VaultContents; trouble?: string }>();
  /** For a put-back tab, whether its vault is read from files alone: `undefined` until the
   *  listing answers. */
  const [fromFiles, setFromFiles] = useState<boolean>();
  const [query, setQuery] = useState("");
  const [asking, setAsking] = useState<Asking>();
  const [shown, setShown] = useState<Shown>();
  const [note, setNote] = useState<{ said: string; trouble?: boolean }>();
  const [moving, setMoving] = useState(false);
  /** Whether *Change how this vault signs in* is open, and whether the core is at work for it. */
  const [changing, setChanging] = useState(false);
  const [changingBusy, setChangingBusy] = useState(false);
  /** Bumped to read the vault again: Read again, and a token stored in another vault's tab. */
  const [again, setAgain] = useState(0);
  /** Whether this vault is read through a token, for the listener below to ask. */
  const declares = useRef(false);
  /** Which press of an eye is the latest, so an answer to an earlier one is dropped. */
  const pressed = useRef(0);
  /** The secret whose reveal is on its way, so a second press cancels it rather than asks again. */
  const coming = useRef<string | undefined>(undefined);

  const hide = useCallback(() => {
    pressed.current += 1;
    coming.current = undefined;
    setShown(undefined);
  }, []);

  const reveal = async (key: string) => {
    const again = shown?.key === key || coming.current === key;
    hide();
    if (again) return;
    const mine = pressed.current;
    coming.current = key;
    setNote(undefined);
    const answer = await settled(commands.vaultSecretReveal(plane, vault, key));
    if (mine !== pressed.current) return;
    coming.current = undefined;
    if (answer.status === "error") setNote({ said: answer.error, trouble: true });
    else setShown({ key, value: answer.data });
  };

  useEffect(() => {
    if (shown === undefined) return;
    const gone = setTimeout(hide, SHOWN_FOR_MS);
    // Escape anywhere in the window, not only in this tab: it only ever hides, and a value on
    // the screen is the one thing an operator reaching for Escape wants gone.
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    window.addEventListener("keydown", escape);
    return () => {
      clearTimeout(gone);
      window.removeEventListener("keydown", escape);
    };
  }, [shown, hide]);

  const copy = async (key: string) => {
    setNote(undefined);
    const answer = await settled(commands.vaultSecretCopy(plane, vault, key));
    if (answer.status === "error") {
      setNote({ said: answer.error, trouble: true });
      return;
    }
    setNote({
      said: `Copied ${key}. The clipboard clears in a minute, unless something else is copied first.`,
    });
  };

  /** Store a token the operator pasted, or move the one charter's environment already has. The
   *  answer's `identity_in_app_env` decides the note: a paste keeps the token out of the app's
   *  environment, so no chat can read it; a move leaves the export in the app's process, so the
   *  note says to relaunch (#271 review, U3). */
  const store = async (how: "put" | "move", token?: string) => {
    setMoving(true);
    setNote(undefined);
    const answer = await settled(
      how === "put"
        ? commands.vaultIdentityPut(plane, vault, token ?? "")
        : commands.vaultIdentityMove(plane, vault),
    );
    setMoving(false);
    if (answer.status === "error") {
      setNote({ said: answer.error, trouble: true });
      return false;
    }
    setSaid({ contents: answer.data });
    onChanged();
    // The other open vault tabs of this project read again: what they point at has changed.
    window.dispatchEvent(new CustomEvent<Stored>(STORED, { detail: { plane, vault } }));
    const names = named(answer.data.identity);
    const stillExported = answer.data.identity_in_app_env;
    const relaunch =
      stillExported.length > 0
        ? ` Your shell still exports ${stillExported.map((v) => `$${v}`).join(", ")}, which a chat can still read from purlis's own environment — quit and relaunch purlis from a shell that does not, and remove the export from your shell's startup files.`
        : "";
    // Never "stored" alone beside a refusal: the token is in your system keychain and the read that
    // followed still failed, and the reason for that is the sentence above the box.
    const unread = answer.data.refused !== null;
    setNote({
      said: unread
        ? `${sentence(names)} is stored in your system keychain, and the vault still could not be read: the reason is above.${relaunch}`
        : `Stored ${names} in your system keychain. purlis reads it from there, and no chat is given the token.${relaunch}`,
      trouble: unread || relaunch !== "",
    });
    return true;
  };

  useEffect(() => {
    if (note === undefined || note.trouble) return;
    const gone = setTimeout(() => setNote(undefined), NOTE_FOR_MS);
    return () => clearTimeout(gone);
  }, [note]);

  useEffect(() => {
    if (!waits) return;
    let gone = false;
    void commands
      .vaultList(plane)
      .then((answer) => {
        if (gone) return;
        const provider =
          answer.status === "ok"
            ? answer.data.find((one) => one.name === vault)?.provider
            : undefined;
        setFromFiles(provider !== undefined && READ_FROM_FILES.has(provider));
      })
      .catch(() => {
        if (!gone) setFromFiles(false);
      });
    return () => {
      gone = true;
    };
  }, [plane, vault, waits]);

  /** Whether the vault may be read now: asked for, or put back and read from files alone. */
  const mayRead = !waits || fromFiles === true;

  useEffect(() => {
    if (!mayRead) return;
    // No reset to "opening" here: the tab is keyed by plane and vault (`Views.tsx`), so a pane
    // that comes to show another vault is a new tab from its first render.
    let gone = false;
    void commands
      .vaultOpen(plane, vault)
      .then((answer) => {
        if (gone) return;
        setSaid(answer.status === "error" ? { trouble: answer.error } : { contents: answer.data });
      })
      .catch((err: unknown) => {
        if (!gone) setSaid({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [plane, vault, again, mayRead]);

  useEffect(() => {
    declares.current = (said?.contents?.identity.length ?? 0) > 0;
  }, [said]);

  useEffect(() => {
    // A token stored in ANOTHER vault's tab of this project: a vault read through a token reads
    // again, since which vaults still have none is part of what it shows.
    const stored = (event: Event) => {
      const from = (event as CustomEvent<Stored>).detail;
      if (from.plane === plane && from.vault !== vault && declares.current) {
        setAgain((was) => was + 1);
      }
    };
    window.addEventListener(STORED, stored);
    return () => window.removeEventListener(STORED, stored);
  }, [plane, vault]);

  /**
   * One write, answered: the vault as it now is replaces the table and the dialog closes, or the
   * core's sentence comes back for the dialog to show beside what the operator was doing.
   */
  const write = async (asked: Promise<VaultAnswer>): Promise<string | undefined> => {
    try {
      const answer = await asked;
      if (answer.status === "error") return answer.error;
      setSaid({ contents: answer.data });
      setAsking(undefined);
      // The value shown may be the one just replaced, or under a name that has gone.
      hide();
      onChanged();
      return undefined;
    } catch (err) {
      return String(err);
    }
  };

  const contents = said?.contents;
  const secrets = useMemo(() => {
    const wanted = query.trim().toLowerCase();
    const all = contents?.secrets ?? [];
    return wanted === "" ? all : all.filter((one) => one.key.toLowerCase().includes(wanted));
  }, [contents, query]);
  const stop = useTabStop(
    undefined,
    secrets.map((one) => one.key),
  );

  const add = () => setAsking({ doing: "add" });

  /** What the last press was answered with: a refusal, or what was done. Under the tools of a
   *  vault that was read, and under the box of one that was not. */
  const noted = (
    <>
      {note?.trouble && (
        <p className="trouble" role="alert">
          {note.said}
        </p>
      )}
      <p className="vault-note" role="status">
        {note?.trouble ? "" : note?.said}
      </p>
    </>
  );

  /** The way to the set-up, under what the tab says of the token: a 1Password vault's alone. */
  const changeSignIn = contents?.provider === "1password" && (
    <p className="vault-identity-change">
      <button
        type="button"
        className="panel-view"
        tabIndex={0}
        disabled={moving}
        onClick={() => setChanging(true)}
      >
        Change how this vault signs in
      </button>
    </p>
  );

  /** Read the vault again, beside a refusal or a health line that may pass (NO-8's follow-up,
   *  #1296): a provider signed in or unlocked meanwhile is read without reopening the tab. */
  const readAgain = (
    <p>
      <button
        type="button"
        className="panel-view"
        tabIndex={0}
        onClick={() => setAgain((was) => was + 1)}
      >
        Read again
      </button>
    </p>
  );

  return (
    <section
      className="view-pane vault-tab"
      data-testid={`vault-tab-${vault}`}
      aria-label={`Vault ${vault}`}
    >
      <header className="view-head">
        <h2>
          <KeyRound className="tab-mark" aria-hidden="true" />
          {vault}
          {contents && (
            <span className="panel-from">
              {contents.refused === null
                ? ` · ${providerName(contents.provider)} · ${counted(contents.count)}`
                : ` · ${providerName(contents.provider)}`}
            </span>
          )}
        </h2>
        {actions}
      </header>
      <div className="view-body">
        {!mayRead ? (
          fromFiles === false ? (
            <Waiting vault={vault} onAsk={onAsk} />
          ) : (
            // Only the listing is asked, which reads no secret: what kind of vault this is.
            <p className="pending" aria-busy="true">
              <LoaderCircle className="node-icon spinning" />
              Looking up the vault…
            </p>
          )
        ) : said === undefined ? (
          <p className="pending" aria-busy="true">
            <LoaderCircle className="node-icon spinning" />
            Opening the vault…
          </p>
        ) : contents === undefined || contents.refused !== null ? (
          // The core's sentence, which names the vault and what to do. An empty table here would
          // read as a vault with nothing in it.
          <>
            <p className="trouble" role="alert">
              {contents === undefined ? said.trouble : contents.refused?.why}
            </p>
            {/* Not opened at all: no box below to read again from (#1296). */}
            {contents === undefined && readAgain}
            {/* Not read, and read through a token (#1526): under why, the box that stores the
                token, which is the way out. No table, no search and no Add: none of them has
                a vault to act on. */}
            {contents && (
              <>
                <IdentityPanel
                  identity={contents.identity}
                  inAppEnv={contents.identity_in_app_env}
                  unread={contents.refused?.kind ?? "other"}
                  elsewhere={[]}
                  busy={moving}
                  onPut={(token) => void store("put", token)}
                  onMove={() => void store("move")}
                  onAgain={() => setAgain((was) => was + 1)}
                />
                {changeSignIn}
                {noted}
              </>
            )}
          </>
        ) : (
          <>
            {!contents.health.ok && (
              <>
                <p className="trouble" role="alert">
                  {contents.health.detail}
                </p>
                {readAgain}
              </>
            )}
            <IdentityPanel
              identity={contents.identity}
              inAppEnv={contents.identity_in_app_env}
              elsewhere={contents.identity_unset_elsewhere}
              onOpenVault={onOpenVault}
              busy={moving}
              onPut={(token) => void store("put", token)}
              onMove={() => void store("move")}
            />
            {changeSignIn}
            <div className="vault-tools">
              <div className="panel-search">
                <Search className="node-icon" />
                <input
                  type="search"
                  value={query}
                  aria-label={`Search secrets in ${vault}`}
                  placeholder={`Search ${counted(contents.count)}`}
                  onChange={(e) => setQuery(e.target.value)}
                />
              </div>
              <button type="button" className="panel-view" tabIndex={0} onClick={add}>
                <Plus className="node-icon" aria-hidden="true" />
                Add
              </button>
            </div>

            {noted}

            {contents.secrets.length === 0 ? (
              <EmptyState
                mark={KeyRound}
                headline={`${vault} holds no secrets yet`}
                body="A secret's value goes into the vault and is never shown here."
                action={
                  <button type="button" tabIndex={0} onClick={add}>
                    Add a secret
                  </button>
                }
                testid="vault-empty"
              />
            ) : secrets.length === 0 ? (
              // Not the empty state: the vault holds secrets, and the search found none of them.
              <p className="none">
                Nothing in {vault} matches “{query.trim()}”.
              </p>
            ) : (
              <table className="vault-secrets" aria-label={`Secrets in ${vault}`}>
                <thead>
                  <tr>
                    <th scope="col">Name</th>
                    <th scope="col">Size</th>
                    <th scope="col">Updated</th>
                    <th scope="col" aria-label="Reveal" />
                  </tr>
                </thead>
                {/* The rows are ONE Tab stop, and Up and Down move along them (charter-app#189,
                    `roving.ts`); Enter or Space opens the row's menu. */}
                <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
                  <tbody>
                    {secrets.map((one) => (
                      <SecretRow
                        key={one.key}
                        secret={one}
                        value={shown?.key === one.key ? shown.value : undefined}
                        onAsk={setAsking}
                        onReveal={() => void reveal(one.key)}
                        onCopy={() => void copy(one.key)}
                      />
                    ))}
                  </tbody>
                </RovingFocusGroup.Root>
              </table>
            )}
          </>
        )}
      </div>

      {changing && (
        <Dialog.Root
          open
          onOpenChange={(open) => {
            if (!open && !changingBusy) setChanging(false);
          }}
        >
          <Dialog.Portal>
            <Dialog.Overlay className="asking" />
            <Dialog.Content
              className="warning"
              aria-describedby={undefined}
              onInteractOutside={(e) => e.preventDefault()}
            >
              <Dialog.Title>{`How ${vault} signs in`}</Dialog.Title>
              <VaultSignIn
                plane={plane}
                vault={vault}
                name={vault}
                onBusy={setChangingBusy}
                onCancel={() => setChanging(false)}
                onDone={(done) => {
                  setChanging(false);
                  setChangingBusy(false);
                  setSaid({ contents: done.contents });
                  hide();
                  onChanged();
                  // The other open vault tabs read again: a ticked one now has its token.
                  window.dispatchEvent(
                    new CustomEvent<Stored>(STORED, { detail: { plane, vault } }),
                  );
                  const others = [saidOfTheOthers(done), saidOfTheOldExport(done)]
                    .filter((said) => said !== "")
                    .join(" ");
                  const unread = done.contents.refused !== null;
                  setNote({
                    said: `${
                      unread
                        ? "How this vault signs in is stored, and the vault still could not be read: the reason is above."
                        : "How this vault signs in is stored. purlis reads the vault with it from now on, with no restart."
                    }${others === "" ? "" : ` ${others}`}`,
                    // An export left to remove is asked of the person: it stays until read.
                    trouble: unread || done.skipped.length > 0 || done.no_longer_read.length > 0,
                  });
                }}
              />
            </Dialog.Content>
          </Dialog.Portal>
        </Dialog.Root>
      )}
      {asking?.doing === "add" && (
        <ValueDialog
          title={`Add a secret to ${vault}`}
          doing="Add secret"
          onWrite={(key, value) => write(commands.vaultSecretAdd(plane, vault, key, value))}
          onCancel={() => setAsking(undefined)}
        />
      )}
      {asking?.doing === "edit" && (
        <ValueDialog
          title={`Edit the value of ${asking.secret}`}
          secret={asking.secret}
          doing="Save value"
          onWrite={(key, value) => write(commands.vaultSecretSet(plane, vault, key, value))}
          onCancel={() => setAsking(undefined)}
        />
      )}
      {asking?.doing === "rename" && (
        <RenameDialog
          secret={asking.secret}
          onRename={(to) => write(commands.vaultSecretRename(plane, vault, asking.secret, to))}
          onCancel={() => setAsking(undefined)}
        />
      )}
      {asking?.doing === "delete" && (
        <DeleteDialog
          vault={vault}
          secret={asking.secret}
          onDelete={() => write(commands.vaultSecretDelete(plane, vault, asking.secret))}
          onCancel={() => setAsking(undefined)}
        />
      )}
    </section>
  );
}

/** Identity variables as the tab names them: `$OP_TEAM_TOKEN, $OP_OTHER_TOKEN`. A token kept
 *  in your system keychain and read through no variable (#1527) is "this vault's token". */
function named(identity: VaultIdentity[]): string {
  return identity.map((one) => (one.kept ? "this vault's token" : `$${one.variable}`)).join(", ");
}

/** What a token is for, where a sentence says "the token for …": the variable, or the vault. */
function tokenFor(identity: VaultIdentity[]): string {
  return identity.every((one) => one.kept) ? "this vault" : named(identity);
}

/** `text` with its first letter in upper case, for a name that begins a sentence. */
function sentence(text: string): string {
  return text.charAt(0).toUpperCase() + text.slice(1);
}

/**
 * Where the vault's identity token is, under the header, and how to put it in your system keychain
 * (charter-app#237, hardened after the #271 review).
 *
 * **The primary way is a password box**: the operator pastes the token and it goes straight to
 * the keyring, so it never sits in the app's environment where a same-user process could read it.
 * The box is uncontrolled and read once at the press, so the value is not in the page's markup.
 *
 * **Moving the token charter's environment already has** stays as a second offer, for an app
 * launched from a shell that exports it — but it leaves the export in the app's own process, so
 * the note after a move (and this line, while `inAppEnv` is non-empty) says to relaunch.
 *
 * **A vault that could not be read keeps the box** (`unread`, #1526), wherever its token is: with
 * the token nowhere the box is the only way to read the vault at all, and with a token in the
 * Keychain it is how the token is replaced. **What is said beside it is what is known**
 * ({@link KEPT_AND_UNREAD}): the refusal above is the provider's own reason, and most reasons are
 * not the token's, so only a refused sign-in says to replace it.
 *
 * **A vault read through several variables has no box**: the box stores one token, and a press
 * the core must refuse is not an offer. It says so, and offers the move where the app's
 * environment has them.
 *
 * **Other vaults that still have no token are named, never written** (`elsewhere`): each is a
 * link to its own tab. A vault read through no identity variable shows nothing here.
 */
function IdentityPanel({
  identity,
  inAppEnv,
  unread,
  elsewhere,
  busy,
  onPut,
  onMove,
  onAgain,
  onOpenVault,
}: {
  identity: VaultIdentity[];
  inAppEnv: string[];
  /** What kept the vault's contents from being read, when they were not: the box is then drawn
   *  wherever the token is. */
  unread?: UnreadFor;
  /** Other vaults read through the same variable whose token is nowhere. */
  elsewhere: string[];
  busy: boolean;
  onPut: (token: string) => void;
  onMove: () => void;
  /** Read the vault again, for a failure that may pass. */
  onAgain?: () => void;
  onOpenVault?: (vault: string) => void;
}) {
  const box = useRef<HTMLInputElement>(null);
  if (identity.length === 0) return null;

  const stillExported =
    inAppEnv.length > 0 ? (
      <span className="trouble">
        {` Your shell still exports ${inAppEnv.map((v) => `$${v}`).join(", ")}; quit and relaunch purlis without it so no chat can read it from purlis's environment.`}
      </span>
    ) : null;

  const inKeychain = identity.every((one) => one.held === "keyring");
  if (inKeychain && unread === undefined) {
    return (
      <div className="vault-identity">
        <p>
          {`purlis reads ${named(identity)} from your system keychain.`}
          {stillExported}
        </p>
        {elsewhere.map((other) => (
          <p key={other}>
            {onOpenVault ? (
              <button
                type="button"
                className="panel-view"
                tabIndex={0}
                onClick={() => onOpenVault(other)}
              >
                {other}
              </button>
            ) : (
              other
            )}
            {` also reads through ${named(identity)} and has no token yet. A token is stored per vault: put it in from that vault's tab.`}
          </p>
        ))}
      </div>
    );
  }

  const put = () => {
    const token = box.current?.value ?? "";
    if (box.current) box.current.value = "";
    if (token !== "") onPut(token);
  };

  const inEnv = identity.some((one) => one.held === "environment");
  const several = identity.length > 1;
  const say = several
    ? `Read through ${named(identity)}. A box stores one token and this vault needs ${identity.length}, so there is none here: start purlis from a shell that exports them, then move them from this tab.`
    : inKeychain
      ? KEPT_AND_UNREAD[unread ?? "other"](named(identity))
      : identity.every((one) => one.held === "unset")
        ? `Paste the service-account token for ${tokenFor(identity)} here. It goes straight into your system keychain; purlis reads it from there, and no chat is given it.`
        : `Read through ${named(identity)}. Put the token in your system keychain, where no chat can read it and purlis finds it for every command.`;
  return (
    <div className="vault-identity">
      <p>
        {say}
        {stillExported}
      </p>
      <div className="vault-identity-put">
        {!several && (
          <>
            <input
              ref={box}
              type="password"
              aria-label={`Token for ${tokenFor(identity)}`}
              placeholder="Paste the token"
              autoComplete="off"
              spellCheck={false}
              disabled={busy}
              onKeyDown={(e) => {
                if (e.key === "Enter") put();
              }}
            />
            <button type="button" tabIndex={0} disabled={busy} onClick={put}>
              Put this vault's token in your system keychain
            </button>
          </>
        )}
        {inEnv && (
          <button
            type="button"
            className="panel-view"
            tabIndex={0}
            disabled={busy}
            onClick={onMove}
          >
            Move the token from purlis's environment
          </button>
        )}
        {unread !== undefined && onAgain && (
          <button
            type="button"
            className="panel-view"
            tabIndex={0}
            disabled={busy}
            onClick={onAgain}
          >
            Read again
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * What the box of a vault says when its token IS in your system keychain and its contents still could
 * not be read, by what kept them (#1526). The reason itself is the core's sentence, drawn above;
 * this says what the box is for in that case, and never blames the token for a failure that is
 * not known to be the token's.
 */
const KEPT_AND_UNREAD: Record<UnreadFor, (names: string) => string> = {
  // Not reached with a token in your system keychain; said plainly all the same.
  "no-token": (names) => `Paste the token for ${names} here.`,
  program: (names) =>
    `purlis reads ${names} from your system keychain, and could not run the program that reads this vault. Nothing says the token is wrong. Storing the token again here pins the program purlis finds now.`,
  "try-again": (names) =>
    `purlis reads ${names} from your system keychain. Nothing says the token is wrong: read again in a moment. The box replaces the token, should you need it.`,
  "sign-in": (names) =>
    `purlis reads ${names} from your system keychain, and the sign-in with it was refused. Paste the right token here to replace it.`,
  other: (names) =>
    `purlis reads ${names} from your system keychain. If it is the token that is wrong, paste the right one here to replace it.`,
};

/** The window event a tab sends when it has stored a token, for the other vault tabs. */
const STORED = "purlis:vault-token-stored";

/** What {@link STORED} carries: whose token was stored. Names, never a value. */
type Stored = { plane: PlaneId; vault: string };

/** What the core answers every vault command with: the vault as it now is, or its refusal. */
type VaultAnswer = Awaited<ReturnType<typeof commands.vaultOpen>>;

/** How long a revealed value stays on the page. */
const SHOWN_FOR_MS = 30_000;

/** The providers whose table is read from the project's files alone — a keyring vault's keys
 *  index, a plain or reference file — so opening their tab reads no secret and runs no
 *  provider (`vaults.rs`, `secrets_of`). Every other one waits when put back (#1660). */
const READ_FROM_FILES: ReadonlySet<string> = new Set(["keyring", "plain-file", "reference"]);

/** A put-back vault tab that has read nothing yet, and the press that reads it (#1660). */
function Waiting({ vault, onAsk }: { vault: string; onAsk?: () => void }) {
  return (
    <EmptyState
      mark={KeyRound}
      headline={`${vault} was open when purlis last quit`}
      body={`Reading it uses its token, which macOS may ask you about, so purlis reads it only when you press Read ${vault}.`}
      action={
        <button type="button" tabIndex={0} onClick={onAsk}>
          {`Read ${vault}`}
        </button>
      }
      testid="vault-waits"
    />
  );
}

/** How long a note under the header stays: a copy's lasts as long as the value stays on the
 * clipboard (`vaults.rs`, `CLEAR_AFTER`), and a token move's as long. A refusal stays until the
 * next press. */
const NOTE_FOR_MS = 60_000;

/** The one value the tab is showing, and whose it is. */
type Shown = { key: string; value: string };

/** A command's answer, with a promise that failed outright read as a refusal. */
async function settled<T>(
  asked: Promise<{ status: "ok"; data: T } | { status: "error"; error: string }>,
): Promise<{ status: "ok"; data: T } | { status: "error"; error: string }> {
  return asked.catch((err: unknown) => ({ status: "error", error: String(err) }));
}

/** Which dialog the tab is asking in, and about which secret. */
type Asking =
  | { doing: "add" }
  | { doing: "edit"; secret: string }
  | { doing: "rename"; secret: string }
  | { doing: "delete"; secret: string };

/**
 * When a secret was written, as the keys index records it (`2026-09-24T11:32:17Z`), to the
 * minute and in UTC — which is what it says, so two machines' tabs agree. Anything else is
 * drawn as it came.
 */
function shownAt(updated: string): string {
  const at = /^(\d{4}-\d\d-\d\d)T(\d\d:\d\d)(:\d\d(\.\d+)?)?Z$/.exec(updated);
  return at ? `${at[1]} ${at[2]} UTC` : updated;
}

/**
 * One secret's row: its name and menu, its size band and when it was written, and the eye.
 *
 * **The eye is not a Tab stop of its own**: the rows are one (charter-app#189), and an eye in
 * each would make them one per row. Right from a row's name reaches its eye, and Left comes
 * back; the pointer reaches it as any button.
 */
function SecretRow({
  secret,
  value,
  onAsk,
  onReveal,
  onCopy,
}: {
  secret: VaultSecret;
  /** The value, while it is revealed. */
  value: string | undefined;
  onAsk: (asking: Asking) => void;
  onReveal: () => void;
  onCopy: () => void;
}) {
  const name = useRef<HTMLButtonElement>(null);
  const eye = useRef<HTMLButtonElement>(null);
  const key = secret.key;
  return (
    <tr>
      <td>
        <SecretMenu
          secret={key}
          trigger={name}
          onAsk={onAsk}
          onCopy={onCopy}
          onRight={() => eye.current?.focus()}
        />
        {value !== undefined && <code className="vault-value">{value}</code>}
      </td>
      <td>{secret.size ?? "—"}</td>
      <td>
        {secret.updated === null ? (
          "—"
        ) : (
          <time dateTime={secret.updated}>{shownAt(secret.updated)}</time>
        )}
      </td>
      <td>
        <button
          type="button"
          ref={eye}
          className="vault-reveal"
          tabIndex={-1}
          aria-label={`Reveal ${key}`}
          aria-pressed={value !== undefined}
          onClick={onReveal}
          onKeyDown={(event) => {
            if (event.key !== "ArrowLeft") return;
            event.preventDefault();
            name.current?.focus();
          }}
        >
          {value === undefined ? (
            <Eye className="node-icon" aria-hidden="true" />
          ) : (
            <EyeOff className="node-icon" aria-hidden="true" />
          )}
        </button>
      </td>
    </tr>
  );
}

/**
 * A secret's name in its row, and the row's menu.
 *
 * **The roving item is outside the menu's trigger**, and the order is the point: the item's
 * handler sees a key first and, for Up and Down, moves the focus and marks the event handled —
 * so the trigger, which would otherwise open the menu on Down, leaves it alone. Enter and Space
 * are not the item's, and open the menu. Right is neither's, and goes to the row's eye.
 */
function SecretMenu({
  secret,
  trigger,
  onAsk,
  onCopy,
  onRight,
}: {
  secret: string;
  trigger: Ref<HTMLButtonElement>;
  onAsk: (asking: Asking) => void;
  onCopy: () => void;
  onRight: () => void;
}) {
  return (
    // **Not modal**, for the strip's show-more menu's reason (`PlaneView.tsx`): a menu is not a
    // question. What an item opens is, and that dialog is modal.
    <Menu.Root modal={false}>
      <RovingFocusGroup.Item asChild tabStopId={secret}>
        <Menu.Trigger asChild>
          <button
            type="button"
            ref={trigger}
            className="vault-secret"
            onKeyDown={(event) => {
              if (event.key !== "ArrowRight") return;
              event.preventDefault();
              onRight();
            }}
          >
            <span className="vault-secret-name">{secret}</span>
            <Ellipsis className="node-icon" aria-hidden="true" />
          </button>
        </Menu.Trigger>
      </RovingFocusGroup.Item>
      <Menu.Portal>
        <Menu.Content className="more-menu" align="start" sideOffset={4} collisionPadding={8}>
          <Menu.Item className="more-tab" onSelect={() => onAsk({ doing: "edit", secret })}>
            Edit value
          </Menu.Item>
          <Menu.Item className="more-tab" onSelect={() => onAsk({ doing: "rename", secret })}>
            Rename
          </Menu.Item>
          <Menu.Item className="more-tab" onSelect={onCopy}>
            Copy
          </Menu.Item>
          <Menu.Item className="more-tab" onSelect={() => onAsk({ doing: "delete", secret })}>
            Delete
          </Menu.Item>
        </Menu.Content>
      </Menu.Portal>
    </Menu.Root>
  );
}

/**
 * Asking for a value: a new secret's name and value, or a held secret's new value.
 *
 * **The value box is uncontrolled** — see `VaultTab`. What the component keeps is whether the
 * box has anything in it, so the button knows; the value itself is read at the press, the box
 * is emptied, and the value is handed to the core.
 */
function ValueDialog({
  title,
  secret,
  doing,
  onWrite,
  onCancel,
}: {
  title: string;
  /** The secret whose value this replaces; a new secret's name is asked for when absent. */
  secret?: string;
  /** What the button says. */
  doing: string;
  /** Writes it: nothing when written, the core's sentence when refused. */
  onWrite: (key: string, value: string) => Promise<string | undefined>;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [filled, setFilled] = useState(false);
  const [trouble, setTrouble] = useState<string>();
  const [writing, setWriting] = useState(false);
  const busy = writing;
  // The dialog, so its first box can be found in it on opening: the rows draw the boxes, and a
  // setting's field takes no ref.
  const content = useRef<HTMLDivElement>(null);
  const valueBox = useRef<HTMLInputElement>(null);
  const key = secret ?? name.trim();
  const ready = key !== "" && filled && !writing;

  const submit = async () => {
    const box = valueBox.current;
    if (!ready || box === null) return;
    // **Emptied at the press**, before the core has answered: the value is in the call now, and
    // a refusal asks for it again rather than keeping it on the page for a retry.
    const value = box.value;
    box.value = "";
    setFilled(false);
    setWriting(true);
    const refused = await onWrite(key, value);
    if (refused === undefined) return;
    setTrouble(refused);
    setWriting(false);
  };

  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        // Not while the core is writing: the answer would land on a dialog nobody is looking at.
        if (!open && !busy) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          ref={content}
          className="warning"
          aria-describedby={undefined}
          // A click outside answers nothing (`docs/ui-primitives.md`). Escape is Cancel.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            // The name box when a new secret is asked for, else the value box.
            content.current?.querySelector("input")?.focus();
          }}
        >
          <Dialog.Title>{title}</Dialog.Title>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            {secret === undefined && (
              <SettingRow
                label="Name"
                control={(ids) => <Field ids={ids} kind="text" value={name} onChange={setName} />}
              />
            )}
            <SettingRow
              label={secret === undefined ? "Value" : "New value"}
              help="It goes into the vault and nowhere else. purlis never shows it here."
              control={(ids) => (
                // **A native box in the row's control slot, not a `Field`**: a `Field` is
                // controlled, and a secret's value is never held in React state (see above).
                <input
                  id={ids.id}
                  ref={valueBox}
                  className="ui-field"
                  type="password"
                  // What keeps a password manager from filling it; WebKit ignores `off` here.
                  autoComplete="new-password"
                  spellCheck={false}
                  aria-describedby={ids.describedBy}
                  onChange={(event) => setFilled(event.target.value !== "")}
                />
              )}
            />
            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}
            <SettingActions>
              <button type="submit" tabIndex={0} disabled={!ready}>
                {doing}
              </button>
              <button type="button" tabIndex={0} disabled={busy} onClick={onCancel}>
                Cancel
              </button>
            </SettingActions>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** Asking for a secret's new name. A name is not a value, so this box is an ordinary one. */
function RenameDialog({
  secret,
  onRename,
  onCancel,
}: {
  secret: string;
  onRename: (to: string) => Promise<string | undefined>;
  onCancel: () => void;
}) {
  const [name, setName] = useState(secret);
  const [trouble, setTrouble] = useState<string>();
  const [writing, setWriting] = useState(false);
  const busy = writing;
  // The dialog, so the box can be found in it on opening: the row draws the box, and a
  // setting's field takes no ref.
  const content = useRef<HTMLDivElement>(null);
  const to = name.trim();
  const ready = to !== "" && to !== secret && !writing;

  const submit = async () => {
    if (!ready) return;
    setWriting(true);
    const refused = await onRename(to);
    if (refused === undefined) return;
    setTrouble(refused);
    setWriting(false);
  };

  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        // Not while the core is writing: the answer would land on a dialog nobody is looking at.
        if (!open && !busy) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          ref={content}
          className="warning"
          aria-describedby={undefined}
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            content.current?.querySelector("input")?.select();
          }}
        >
          <Dialog.Title>Rename {secret}</Dialog.Title>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            <SettingRow
              label="New name"
              control={(ids) => <Field ids={ids} kind="text" value={name} onChange={setName} />}
            />
            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}
            <SettingActions>
              <button type="submit" tabIndex={0} disabled={!ready}>
                Rename
              </button>
              <button type="button" tabIndex={0} disabled={busy} onClick={onCancel}>
                Cancel
              </button>
            </SettingActions>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * Asking before a secret is deleted. **Cancel has the focus**, as it has in every dialog here
 * that ends something: an Enter pressed out of habit keeps the secret.
 */
function DeleteDialog({
  vault,
  secret,
  onDelete,
  onCancel,
}: {
  vault: string;
  secret: string;
  onDelete: () => Promise<string | undefined>;
  onCancel: () => void;
}) {
  const [trouble, setTrouble] = useState<string>();
  const [deleting, setDeleting] = useState(false);
  const busy = deleting;
  const cancel = useRef<HTMLButtonElement>(null);
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        // Not while the core is writing: the answer would land on a dialog nobody is looking at.
        if (!open && !busy) onCancel();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="asking" />
        <AlertDialog.Content
          className="warning"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            cancel.current?.focus();
          }}
        >
          <AlertDialog.Title>
            Delete {secret} from {vault}?
          </AlertDialog.Title>
          <AlertDialog.Description className="came-back">
            Its value is gone from the vault for good. There is no undo.
          </AlertDialog.Description>
          {trouble && (
            <p className="trouble" role="alert">
              {trouble}
            </p>
          )}
          <AnswerBar>
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0} disabled={busy} ref={cancel}>
                Cancel
              </button>
            </AlertDialog.Cancel>
            <button
              type="button"
              className="ends-it"
              tabIndex={0}
              disabled={deleting}
              onClick={() => {
                setDeleting(true);
                void onDelete().then((refused) => {
                  if (refused === undefined) return;
                  setTrouble(refused);
                  setDeleting(false);
                });
              }}
            >
              Delete
            </button>
          </AnswerBar>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
