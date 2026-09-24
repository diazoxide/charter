import { useCallback, useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { KeyRound } from "lucide-react";
import { PanelList } from "./PanelList";
import type { Catalogued, Offer } from "./actions";
import { commands, type PanelRow, type VaultSummary } from "./bindings";

/**
 * The plane's vaults, in the Attention region: each vault with its provider and how many
 * secrets it holds (charter-app#234), and **pressing one opens its tab** (charter-app#235).
 *
 * **Names and counts, and never a value.** `vault_list` answers with nothing else
 * (`app/src-tauri/src/vaults.rs`, whose test serializes every answer and looks for the values
 * it wrote), so there is nothing here that could draw one.
 *
 * **The plane's, not the workspace's.** A vault is registered once per plane, so this section
 * is drawn whichever workspace is focused, and with none.
 *
 * **The window asks, and this draws** (`useVaults`, held by `PlaneView`): the same answer is
 * the palette's `vault.open:<name>` rows and the picker's list, and a vault's tab asks the window
 * to read it again after a write — so one list, read in one place, serves all four.
 */
export function Vaults({
  said,
  offers,
  onPress,
}: {
  said: Pick<VaultsSaid, "vaults" | "trouble">;
  /** The catalogue by id, which is what a row's verb is looked up in. */
  offers: Catalogued;
  onPress: (offer: Offer) => void;
}) {
  const { vaults, trouble } = said;
  return (
    <section data-testid="panel-vaults">
      <div className="panel-head">
        <h2>
          <KeyRound className="node-icon" />
          Vaults
        </h2>
      </div>
      {trouble !== undefined ? (
        <p className="trouble" role="alert">
          {trouble}
        </p>
      ) : (
        vaults !== undefined && (
          <PanelList
            rows={vaults.map(rowOf)}
            empty={NO_VAULTS}
            label="Vaults"
            testid="list-vaults"
            // No row opens a card: pressing one opens the vault's tab, which says the rest.
            open={undefined}
            onOpen={() => undefined}
            onRun={(id) => {
              // **The catalogue's row or nothing**, `Panels.tsx`'s rule: an id the catalogue
              // has stopped offering runs nothing rather than something else.
              const offer = offers.get(id);
              if (offer) onPress(offer);
            }}
          />
        )
      )}
    </section>
  );
}

/** How many secrets, in words: `1 secret`, `2 secrets`. The panel and a vault's tab both say it. */
export function counted(n: number): string {
  return `${n} ${n === 1 ? "secret" : "secrets"}`;
}

/** What a list of vaults says when the plane has none. */
const NO_VAULTS = {
  headline: "No vaults on this plane",
  body: "Make one with New vault… in the palette.",
  offer: null,
};

/**
 * One vault as a row: its name, then its provider and count. Pressing it runs the catalogue's
 * `vault.open:<name>`, which opens the vault's tab. A vault charter cannot read is marked, and
 * its tab says why — the card that used to say it would be a second surface for one vault.
 */
function rowOf(vault: VaultSummary): PanelRow {
  const count = vault.count === null ? "" : ` · ${counted(vault.count)}`;
  return {
    key: vault.name,
    text: vault.name,
    note: `${vault.provider}${count}`,
    mark: "vault",
    tone: vault.health.ok ? "plain" : "trouble",
    detail: null,
    runs: `vault.open:${vault.name}`,
  };
}

/**
 * **"Open vault…"**: the plane's vaults in a dialog, one press from the palette when the name is
 * not what the operator has in mind. Each row runs the same `vault.open:<name>` the panel's rows
 * and the palette's own rows run. The keyboard lands on the first vault, and Up, Down and Enter
 * are all it takes.
 */
export function OpenVault({
  vaults,
  offers,
  onPress,
  onCancel,
}: {
  vaults: readonly VaultSummary[];
  offers: Catalogued;
  onPress: (offer: Offer) => void;
  onCancel: () => void;
}) {
  const list = useRef<HTMLDivElement>(null);
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
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
            list.current?.querySelector<HTMLElement>("button.row")?.focus();
          }}
        >
          <Dialog.Title>Open vault</Dialog.Title>
          <div ref={list}>
            <PanelList
              rows={vaults.map(rowOf)}
              empty={NO_VAULTS}
              label="Vaults to open"
              open={undefined}
              onOpen={() => undefined}
              onRun={(id) => {
                const offer = offers.get(id);
                if (!offer) return;
                onCancel();
                onPress(offer);
              }}
            />
          </div>
          <div className="doing">
            <button type="button" tabIndex={0} onClick={onCancel}>
              Cancel
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** What `vault_list` said for a plane, and a way to ask it again. */
export type VaultsSaid = { vaults?: VaultSummary[]; trouble?: string; reload: () => void };

/**
 * `vault_list` for one plane, asked when the plane is and whenever `reload` is called — which a
 * vault's tab does after every write, since the counts here are that vault's.
 *
 * **An answer that is not a list is no answer**, the rule `useDoctor` keeps: whole-window tests
 * mock every command they do not care about with `null` or `[]`.
 */
export function useVaults(plane: string): VaultsSaid {
  const [said, setSaid] = useState<{ plane: string; vaults?: VaultSummary[]; trouble?: string }>();
  const [asked, setAsked] = useState(0);
  useEffect(() => {
    let gone = false;
    void commands
      .vaultList(plane)
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setSaid({ plane, trouble: answer.error });
        else if (Array.isArray(answer.data)) setSaid({ plane, vaults: answer.data });
      })
      .catch(() => {
        // Nothing: a core that did not answer leaves the section with its heading and no rows.
      });
    return () => {
      gone = true;
    };
  }, [plane, asked]);
  const reload = useCallback(() => setAsked((was) => was + 1), []);
  // An answer for the plane before is not this plane's.
  return said?.plane === plane ? { ...said, reload } : { reload };
}
