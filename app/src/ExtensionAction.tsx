import { useRef, useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { commands, type ActionAnswer, type PlaneId, type RowAction } from "./bindings";

/**
 * **An extension's action, run** — from a row of its view or from a palette command
 * (charter-app#341, ADR 0053).
 *
 * The core decides everything that matters: the gate re-taken at the press, the one process,
 * and the refusal of an action that asks first without a yes (`charter_core::executor::act`).
 * What the window does is ask — when the action's manifest says to, and always when it deletes
 * (`RowAction.asks_first`, which the core computed) — and say what came back.
 */

/** Where an action was pressed: the view and its persona, and the row — or none of them, for a
 *  palette command. */
export type Pressed = {
  view: string | null;
  key: string;
  row: string | null;
};

/** What running it came to: the core's answer, or its refusal in its own words. */
export type Outcome = { answer: ActionAnswer } | { refused: string };

/** Ask the core to run `action`, with the operator's yes or without it. */
export async function runExtensionAction(
  plane: PlaneId,
  extension: string,
  action: RowAction,
  pressed: Pressed,
  workspace: string | undefined,
  confirmed: boolean,
): Promise<Outcome> {
  try {
    const said = await commands.runAction(
      plane,
      extension,
      action.id,
      pressed.view,
      pressed.key,
      pressed.row,
      workspace ?? null,
      confirmed,
    );
    return said.status === "error" ? { refused: said.error } : { answer: said.data };
  } catch (err: unknown) {
    return { refused: String(err) };
  }
}

/** What pressing Run came to, for the dialog: a refusal to show and let the operator try
 *  again or cancel, a sentence about what it ran to show before it closes, or nothing — the
 *  caller closes it. */
export type Ran = { refused: string } | { seen: string } | undefined;

/**
 * The question charter asks before an action that asks first. **Cancel has the focus**, as in
 * every dialog here that can end something: an Enter pressed out of habit runs nothing.
 *
 * `onRun` answers what to show: a refusal, or — from a palette command, which has no view to
 * say it above — what changed outside the extension's declared paths, after which the only
 * button left is Close.
 */
export function AskFirst({
  extension,
  action,
  onRun,
  onCancel,
}: {
  extension: string;
  action: RowAction;
  onRun: () => Promise<Ran>;
  onCancel: () => void;
}) {
  const [trouble, setTrouble] = useState<string>();
  const [seen, setSeen] = useState<string>();
  const [running, setRunning] = useState(false);
  const cancel = useRef<HTMLButtonElement>(null);
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        // Not while the core is running it: the answer would land on a dialog nobody sees.
        if (!open && !running) onCancel();
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
            Run “{action.title}” from {extension}?
          </AlertDialog.Title>
          <AlertDialog.Description className="came-back">
            {action.deletes
              ? "It deletes, and charter asks before every action that deletes. What it deletes is its own to say; charter does not stop it."
              : `${extension} asks charter to ask you before it runs this.`}
          </AlertDialog.Description>
          {(trouble ?? seen) !== undefined && (
            <p className="trouble" role="alert">
              {trouble ?? seen}
            </p>
          )}
          <div className="doing">
            {seen === undefined && (
              <button
                type="button"
                className={action.deletes ? "ends-it" : undefined}
                tabIndex={0}
                disabled={running}
                onClick={() => {
                  setRunning(true);
                  void onRun().then((ran) => {
                    if (ran === undefined) return;
                    if ("refused" in ran) setTrouble(ran.refused);
                    else {
                      setTrouble(undefined);
                      setSeen(ran.seen);
                    }
                    setRunning(false);
                  });
                }}
              >
                {action.deletes ? "Delete" : "Run"}
              </button>
            )}
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0} disabled={running} ref={cancel}>
                {seen === undefined ? "Cancel" : "Close"}
              </button>
            </AlertDialog.Cancel>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
