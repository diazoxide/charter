import { useEffect, useState } from "react";
import { commands, type ExtensionFacts, type PlaneId } from "./bindings";

/**
 * **What the extensions on in a project show in the window**: the status bar's badges and the
 * repo table's extra columns (charter-app#340), read by the core's one reader from each
 * extension's facts file — `extension_facts`, which never starts a program.
 *
 * Asked when a project or workspace comes into focus, and again after an extension answers a
 * view (`factsChanged`, from `Views.tsx`) or an extension is approved or turned on or off
 * (`extensionsChanged`): those are when a facts file, or who may show one, changes. Nothing
 * polls — an app nobody touches reads nothing.
 *
 * Until the answer arrives nothing is drawn, and a failed question draws nothing: a badge is
 * the least important thing on the line.
 */

const listeners = new Set<(plane: PlaneId | undefined) => void>();

/** Ask again, in every window part showing `plane` (or every plane). */
export function factsChanged(plane?: PlaneId) {
  for (const listener of listeners) listener(plane);
}

const NONE: ExtensionFacts = { badges: [], columns: [], notes: [] };

type Held = { key: string; facts: ExtensionFacts };

export function useExtensionFacts(plane: PlaneId, workspace: string | undefined): ExtensionFacts {
  const key = `${plane}\u0000${workspace ?? ""}`;
  const [held, setHeld] = useState<Held>();
  const [asked, setAsked] = useState(0);

  useEffect(() => {
    const listener = (changed: PlaneId | undefined) => {
      if (changed === undefined || changed === plane) setAsked((n) => n + 1);
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, [plane]);

  useEffect(() => {
    let gone = false;
    void commands
      .extensionFacts(plane, workspace ?? null)
      .then((said) => {
        // An `ok` with no body reads as nothing to show, as `workspaceState.ts` treats one:
        // the window must not throw inside a promise nothing is holding.
        if (!gone && said.status === "ok") setHeld({ key, facts: said.data ?? NONE });
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [asked, key, plane, workspace]);

  // An answer for another project or workspace is not this one's.
  return held?.key === key ? held.facts : NONE;
}
