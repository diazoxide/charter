import { useCallback, useMemo, useState, type ReactNode } from "react";
import { DeleteVault, type VaultHolds } from "./DeleteVault";
import { NewPersona } from "./NewPersona";
import { RemovePersona } from "./RemovePersona";
import type { Doing, Ran } from "./actions";
import { commands, type PlaneId } from "./bindings";
import type { ViewRef } from "./tabs";

/** The verbs of `Doing` this hook carries out (SI-3). */
export type PlaneEditing = Pick<
  Doing,
  "removeVault" | "createPersona" | "editPersona" | "removePersona" | "closeTodo" | "forgetTodo"
>;

/**
 * **Making and deleting a plane's personas, vaults and todos from the window** (SI-3): the verbs
 * the catalogue's rows run, the dialogs they ask in, and the Todos panel's box.
 *
 * A hook of its own rather than more of `PlaneView`, which holds the window's arrangement: none
 * of this is arrangement. What it needs from the window is four things — the plane, a way to open
 * a view, a way to close one, and a way to have the plane read again — and it hands back the
 * verbs, the box's writer and the dialogs to draw.
 *
 * **Every write is the core's**, and every answer is redrawn from the disk rather than patched
 * here: a todo written, a persona made or deleted, a vault deleted — each ends in the window
 * reading the plane again (`reread`, `reloadVaults`), so the panels say what is there, not what
 * this hook believes it did.
 */
export function usePlaneEdits({
  plane,
  showView,
  closeView,
  reread,
  reloadVaults,
}: {
  plane: PlaneId;
  showView: (view: ViewRef, title: string) => void;
  /** Close the tab showing that view, where it shows nothing else. */
  closeView: (view: ViewRef) => void;
  /** Read the focused workspace's panels again: its todos, and the plane's personas. */
  reread: () => void;
  /** Read the plane's vaults again. */
  reloadVaults: () => void;
}): {
  doing: PlaneEditing;
  addTodo: (workspace: string, text: string) => Promise<string | undefined>;
  dialogs: ReactNode;
} {
  const [deletingVault, setDeletingVault] = useState<{
    vault: string;
    holds?: VaultHolds;
    unreadable?: string;
    trouble?: string;
    busy: boolean;
  }>();
  const [makingPersona, setMakingPersona] = useState<{ trouble?: string; busy: boolean }>();
  const [removingPersona, setRemovingPersona] = useState<{
    persona: string;
    trouble?: string;
    busy: boolean;
  }>();

  /** Asks about deleting a vault, and reads what it holds (names only) for the dialog to list. */
  const removeVault = useCallback(
    (vault: string) => {
      setDeletingVault({ vault, busy: false });
      void settled(commands.vaultOpen(plane, vault)).then((answer) =>
        setDeletingVault((now) =>
          now?.vault !== vault
            ? now
            : answer.status === "ok"
              ? {
                  ...now,
                  holds: {
                    provider: answer.data.provider,
                    secrets: answer.data.secrets.map((one) => one.key),
                  },
                }
              : { ...now, unreadable: answer.error },
        ),
      );
    },
    [plane],
  );

  const deleteVault = useCallback(
    async (vault: string) => {
      setDeletingVault((now) => (now ? { ...now, busy: true, trouble: undefined } : now));
      const answer = await settled(commands.vaultRemove(plane, vault));
      if (answer.status === "error") {
        setDeletingVault((now) => (now ? { ...now, busy: false, trouble: answer.error } : now));
        return;
      }
      setDeletingVault(undefined);
      closeView({ from: null, view: "vault", key: vault });
      reloadVaults();
    },
    [closeView, plane, reloadVaults],
  );

  const createPersona = useCallback(() => setMakingPersona({ busy: false }), []);

  /** Makes it through `charter persona create`'s own path, then opens its tab: a new persona is
   *  a draft, and its tab is where the operator sees what to write next. */
  const makePersona = useCallback(
    async (name: string, role: string | null, when: string | null, parent: string | null) => {
      setMakingPersona({ busy: true });
      const answer = await settled(commands.personaCreate(plane, name, role, when, parent));
      if (answer.status === "error") {
        setMakingPersona({ busy: false, trouble: answer.error });
        return;
      }
      setMakingPersona(undefined);
      reread();
      showView({ from: null, view: "persona", key: name }, name);
    },
    [plane, reread, showView],
  );

  const editPersona = useCallback(
    async (persona: string): Promise<Ran> => {
      const answer = await settled(commands.personaEdit(plane, persona));
      return answer.status === "error" ? { ok: false, refused: answer.error } : { ok: true };
    },
    [plane],
  );

  const removePersona = useCallback(
    (persona: string) => setRemovingPersona({ persona, busy: false }),
    [],
  );

  const deletePersona = useCallback(
    async (persona: string) => {
      setRemovingPersona({ persona, busy: true });
      const answer = await settled(commands.personaRemove(plane, persona));
      if (answer.status === "error") {
        setRemovingPersona({ persona, busy: false, trouble: answer.error });
        return;
      }
      setRemovingPersona(undefined);
      closeView({ from: null, view: "persona", key: persona });
      reread();
    },
    [closeView, plane, reread],
  );

  /** A todo write: the core's sentence either way, and the panel read again when it wrote. */
  const todoWrite = useCallback(
    async (write: Promise<{ status: "ok"; data: string } | { status: "error"; error: string }>) => {
      const answer = await settled(write);
      if (answer.status === "error") return { ok: false as const, refused: answer.error };
      reread();
      return { ok: true as const, said: answer.data };
    },
    [reread],
  );

  const closeTodo = useCallback(
    (workspace: string, slug: string) => todoWrite(commands.todoDone(plane, workspace, slug)),
    [plane, todoWrite],
  );
  const forgetTodo = useCallback(
    (workspace: string, slug: string) => todoWrite(commands.todoForget(plane, workspace, slug)),
    [plane, todoWrite],
  );
  const addTodo = useCallback(
    async (workspace: string, text: string) => {
      const ran = await todoWrite(commands.todoAdd(plane, workspace, text));
      return ran.ok ? undefined : ran.refused;
    },
    [plane, todoWrite],
  );

  const doing = useMemo<PlaneEditing>(
    () => ({ removeVault, createPersona, editPersona, removePersona, closeTodo, forgetTodo }),
    [removeVault, createPersona, editPersona, removePersona, closeTodo, forgetTodo],
  );

  const dialogs = (
    <>
      {deletingVault && (
        <DeleteVault
          vault={deletingVault.vault}
          holds={deletingVault.holds}
          unreadable={deletingVault.unreadable}
          trouble={deletingVault.trouble}
          deleting={deletingVault.busy}
          onDelete={() => void deleteVault(deletingVault.vault)}
          onCancel={() => setDeletingVault(undefined)}
        />
      )}
      {makingPersona && (
        <NewPersona
          plane={plane}
          trouble={makingPersona.trouble}
          making={makingPersona.busy}
          onCreate={(name, role, when, parent) => void makePersona(name, role, when, parent)}
          onCancel={() => setMakingPersona(undefined)}
        />
      )}
      {removingPersona && (
        <RemovePersona
          persona={removingPersona.persona}
          trouble={removingPersona.trouble}
          deleting={removingPersona.busy}
          onDelete={() => void deletePersona(removingPersona.persona)}
          onCancel={() => setRemovingPersona(undefined)}
        />
      )}
    </>
  );

  return { doing, addTodo, dialogs };
}

/** A command's answer, or a core that did not answer as a refusal in the words it threw. */
async function settled<T>(
  asked: Promise<{ status: "ok"; data: T } | { status: "error"; error: string }>,
): Promise<{ status: "ok"; data: T } | { status: "error"; error: string }> {
  try {
    const answer = await asked;
    // Whole-window tests mock every command they do not care about with `null`.
    return answer ?? { status: "error", error: "charter did not answer" };
  } catch (err) {
    return { status: "error", error: String(err) };
  }
}
