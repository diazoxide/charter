import { useCallback, useEffect, useId, useMemo, useRef, useState, type Ref } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import * as Dialog from "@radix-ui/react-dialog";
import * as Menu from "@radix-ui/react-dropdown-menu";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { Ellipsis, Eye, EyeOff, KeyRound, LoaderCircle, Plus, Search } from "lucide-react";
import { EmptyState } from "./EmptyState";
import {
  commands,
  type PlaneId,
  type VaultContents,
  type VaultIdentity,
  type VaultSecret,
} from "./bindings";
import { useTabStop } from "./roving";
import { counted } from "./Vaults";

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
 * **A 1Password token moves into the Keychain from here** (charter-app#237). Where the vault is
 * read through an identity variable charter's environment carries (`$OP_TEAM_TOKEN`), the header
 * offers "Move this token into the Keychain": `vault_identity_move` reads the token in the core,
 * stores it in the keyring and answers with the vault's names. The token never comes here, and
 * no chat carries an `OP_*` variable, so after the move the keyring is where every `charter
 * secret` finds it.
 *
 * **Every write answers with the vault as it now is**, so the table is redrawn from the core's
 * answer and never patched by hand here; `onChanged` tells the window, whose Vaults panel counts
 * the secrets too.
 */
export function VaultTab({
  plane,
  vault,
  onChanged,
}: {
  plane: PlaneId;
  vault: string;
  /** A write changed the vault: the window reads its vault list again. */
  onChanged: () => void;
}) {
  const [said, setSaid] = useState<{ contents?: VaultContents; trouble?: string }>();
  const [query, setQuery] = useState("");
  const [asking, setAsking] = useState<Asking>();
  const [shown, setShown] = useState<Shown>();
  const [note, setNote] = useState<{ said: string; trouble?: boolean }>();
  const [moving, setMoving] = useState(false);
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
    const names = named(answer.data.identity);
    const stillExported = answer.data.identity_in_app_env;
    const relaunch =
      stillExported.length > 0
        ? ` Your shell still exports ${stillExported.map((v) => `$${v}`).join(", ")}, which a chat can still read from charter's own environment — quit and relaunch charter from a shell that does not, and remove the export from your shell's startup files.`
        : "";
    setNote({
      said: `Stored ${names} in the Keychain. charter reads it from there, and no chat is given the token.${relaunch}`,
      trouble: relaunch !== "",
    });
    return true;
  };

  useEffect(() => {
    if (note === undefined || note.trouble) return;
    const gone = setTimeout(() => setNote(undefined), NOTE_FOR_MS);
    return () => clearTimeout(gone);
  }, [note]);

  useEffect(() => {
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
            <span className="panel-from">{` · ${contents.provider} · ${counted(contents.count)}`}</span>
          )}
        </h2>
      </header>
      <div className="view-body">
        {said === undefined ? (
          <p className="pending" aria-busy="true">
            <LoaderCircle className="node-icon spinning" />
            Opening the vault…
          </p>
        ) : contents === undefined ? (
          // The core's sentence, which names the vault and what to do. An empty table here would
          // read as a vault with nothing in it.
          <p className="trouble" role="alert">
            {said.trouble}
          </p>
        ) : (
          <>
            {!contents.health.ok && (
              <p className="trouble" role="alert">
                {contents.health.detail}
              </p>
            )}
            <IdentityPanel
              identity={contents.identity}
              inAppEnv={contents.identity_in_app_env}
              busy={moving}
              onPut={(token) => void store("put", token)}
              onMove={() => void store("move")}
            />
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

            {note?.trouble && (
              <p className="trouble" role="alert">
                {note.said}
              </p>
            )}
            <p className="vault-note" role="status">
              {note?.trouble ? "" : note?.said}
            </p>

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

/** Identity variables as the tab names them: `$OP_TEAM_TOKEN, $OP_OTHER_TOKEN`. */
function named(identity: VaultIdentity[]): string {
  return identity.map((one) => `$${one.variable}`).join(", ");
}

/**
 * Where the vault's identity token is, under the header, and how to put it in the Keychain
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
 * A vault whose token is nowhere says so in its health line above; a vault read through no
 * identity variable shows nothing here.
 */
function IdentityPanel({
  identity,
  inAppEnv,
  busy,
  onPut,
  onMove,
}: {
  identity: VaultIdentity[];
  inAppEnv: string[];
  busy: boolean;
  onPut: (token: string) => void;
  onMove: () => void;
}) {
  const box = useRef<HTMLInputElement>(null);
  if (identity.length === 0) return null;

  const stillExported =
    inAppEnv.length > 0 ? (
      <span className="trouble">
        {` Your shell still exports ${inAppEnv.map((v) => `$${v}`).join(", ")}; quit and relaunch charter without it so no chat can read it from charter's environment.`}
      </span>
    ) : null;

  if (identity.every((one) => one.held === "keyring")) {
    return (
      <p className="vault-identity">
        {`charter reads ${named(identity)} from the Keychain.`}
        {stillExported}
      </p>
    );
  }

  const put = () => {
    const token = box.current?.value ?? "";
    if (box.current) box.current.value = "";
    if (token !== "") onPut(token);
  };

  const inEnv = identity.some((one) => one.held === "environment");
  return (
    <div className="vault-identity">
      <p>
        {`Read through ${named(identity)}. Put the token in the Keychain, where no chat can read it and charter finds it for every command.`}
        {stillExported}
      </p>
      <div className="vault-identity-put">
        <input
          ref={box}
          type="password"
          aria-label={`Token for ${named(identity)}`}
          placeholder="Paste the token"
          autoComplete="off"
          spellCheck={false}
          disabled={busy}
          onKeyDown={(e) => {
            if (e.key === "Enter") put();
          }}
        />
        <button type="button" tabIndex={0} disabled={busy} onClick={put}>
          Put this vault's token in the Keychain
        </button>
        {inEnv && (
          <button
            type="button"
            className="panel-view"
            tabIndex={0}
            disabled={busy}
            onClick={onMove}
          >
            Move the token from charter's environment
          </button>
        )}
      </div>
    </div>
  );
}

/** What the core answers every vault command with: the vault as it now is, or its refusal. */
type VaultAnswer = Awaited<ReturnType<typeof commands.vaultOpen>>;

/** How long a revealed value stays on the page. */
const SHOWN_FOR_MS = 30_000;

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
  const nameId = useId();
  const valueId = useId();
  const nameBox = useRef<HTMLInputElement>(null);
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
          className="warning"
          aria-describedby={undefined}
          // A click outside answers nothing (`docs/ui-primitives.md`). Escape is Cancel.
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            (secret === undefined ? nameBox : valueBox).current?.focus();
          }}
        >
          <Dialog.Title>{title}</Dialog.Title>
          <form
            className="asks"
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            {secret === undefined && (
              <>
                <label htmlFor={nameId}>Name</label>
                <input
                  id={nameId}
                  ref={nameBox}
                  value={name}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(event) => setName(event.target.value)}
                />
              </>
            )}
            <label htmlFor={valueId}>{secret === undefined ? "Value" : "New value"}</label>
            <input
              id={valueId}
              ref={valueBox}
              type="password"
              // What keeps a password manager from filling it; WebKit ignores `off` here.
              autoComplete="new-password"
              spellCheck={false}
              onChange={(event) => setFilled(event.target.value !== "")}
            />
            <p className="came-back">
              It goes into the vault and nowhere else. charter never shows it here.
            </p>
            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}
            <div className="doing">
              <button type="submit" tabIndex={0} disabled={!ready}>
                {doing}
              </button>
              <button type="button" tabIndex={0} disabled={busy} onClick={onCancel}>
                Cancel
              </button>
            </div>
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
  const nameId = useId();
  const box = useRef<HTMLInputElement>(null);
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
          className="warning"
          aria-describedby={undefined}
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            box.current?.select();
          }}
        >
          <Dialog.Title>Rename {secret}</Dialog.Title>
          <form
            className="asks"
            onSubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            <label htmlFor={nameId}>New name</label>
            <input
              id={nameId}
              ref={box}
              value={name}
              autoComplete="off"
              spellCheck={false}
              onChange={(event) => setName(event.target.value)}
            />
            {trouble && (
              <p className="trouble" role="alert">
                {trouble}
              </p>
            )}
            <div className="doing">
              <button type="submit" tabIndex={0} disabled={!ready}>
                Rename
              </button>
              <button type="button" tabIndex={0} disabled={busy} onClick={onCancel}>
                Cancel
              </button>
            </div>
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
          <div className="doing">
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
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0} disabled={busy} ref={cancel}>
                Cancel
              </button>
            </AlertDialog.Cancel>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
