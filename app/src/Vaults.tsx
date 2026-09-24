import { useEffect, useState } from "react";
import { KeyRound } from "lucide-react";
import { PanelList } from "./PanelList";
import { commands, type PanelRow, type VaultSummary } from "./bindings";

/**
 * The plane's vaults, in the Attention region: each vault with its provider and how many
 * secrets it holds (charter-app#234).
 *
 * **Names and counts, and never a value.** `vault_list` answers with nothing else
 * (`app/src-tauri/src/vaults.rs`, whose test serializes every answer and looks for the values
 * it wrote), so there is nothing here that could draw one.
 *
 * **The plane's, not the workspace's.** A vault is registered once per plane, so this section
 * is drawn whichever workspace is focused, and with none.
 */
export function Vaults({ plane }: { plane: string }) {
  const { vaults, trouble } = useVaults(plane);
  const [open, setOpen] = useState<string>();

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
            empty={{
              headline: "No vaults on this plane",
              body: "Make one: charter vault add <name>",
              offer: null,
            }}
            label="Vaults"
            testid="list-vaults"
            open={open}
            onOpen={setOpen}
          />
        )
      )}
    </section>
  );
}

/** One vault as a panel row: its name, then its provider and count, and its health in the card. */
function rowOf(vault: VaultSummary): PanelRow {
  const count =
    vault.count === null ? "" : ` · ${vault.count} ${vault.count === 1 ? "secret" : "secrets"}`;
  return {
    key: vault.name,
    text: vault.name,
    note: `${vault.provider}${count}`,
    mark: "vault",
    tone: vault.health.ok ? "plain" : "trouble",
    detail: { kind: "text", text: vault.health.detail },
    runs: null,
  };
}

/**
 * `vault_list` for one plane, asked when the plane is.
 *
 * **An answer that is not a list is no answer**, the rule `useDoctor` keeps: whole-window tests
 * mock every command they do not care about with `null` or `[]`.
 */
function useVaults(plane: string): { vaults?: VaultSummary[]; trouble?: string } {
  const [said, setSaid] = useState<{ plane: string; vaults?: VaultSummary[]; trouble?: string }>();
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
  }, [plane]);
  // An answer for the plane before is not this plane's.
  return said?.plane === plane ? said : {};
}
