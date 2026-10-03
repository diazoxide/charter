import { useEffect, useState } from "react";
import { commands, type PlaneId } from "./bindings";

/**
 * **The one-time offer of the sandbox to a project made before it existed** (ADR 0067 §1,
 * ruling V21 1). A project charter makes now runs every chat sandboxed; one made earlier keeps
 * running as it did, and the first time it is opened after the upgrade this asks once whether
 * to turn it on. Nothing flips on by itself.
 *
 * **A notice, never a dialog.** It sits in the project view like "Session saved" does, asks
 * nothing until the person goes to it, and costs nothing of the first run's interrupt budget
 * (DS-9, W10). A new project never shows it: it has the sandbox on already.
 *
 * **Answered once, either way.** "Turn the sandbox on" writes `[sandbox] mode = "on"` into the
 * project's `charter.toml`; "Keep it off" changes nothing in the project. Either answer is kept
 * on this machine, where a sandboxed chat cannot write it — the chats of a project offered it
 * are unsandboxed, and run as the person — and the notice does not come back. Closing the
 * window without answering leaves it to be asked next time.
 */
export function SandboxOffer({ plane }: { plane: PlaneId }) {
  const [due, setDue] = useState(false);
  // The harnesses never sandboxed on this machine (`sandbox::never_on`): said by name, so
  // "every new chat runs sandboxed" never reads as covering them.
  const [never, setNever] = useState<readonly string[]>([]);
  const [trouble, setTrouble] = useState<string>();

  useEffect(() => {
    let live = true;
    void commands
      .sandboxState(plane)
      .then((said) => {
        if (live && said.status === "ok") {
          setDue(said.data?.offer === true);
          setNever(said.data?.never ?? []);
        }
      })
      // A project whose state cannot be read is offered nothing: the offer can wait.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [plane]);

  if (!due) return null;

  const answer = (turnOn: boolean) =>
    void commands
      .answerSandboxOffer(plane, turnOn)
      .then((said) => {
        if (said.status === "ok") {
          setDue(said.data?.offer === true);
          setTrouble(undefined);
        } else setTrouble(said.error);
      })
      .catch((err: unknown) => setTrouble(String(err)));

  return (
    <div className="came-back" role="status" data-testid="sandbox-offer">
      <p>
        This project runs its chats without the sandbox. Turned on, every new chat runs sandboxed:
        it reaches only model providers, your forges and package registries, and never your vaults.
        You can still start one chat without it from the new-chat picker.
      </p>
      {never.length > 0 && (
        <p>
          Never sandboxed on this machine, so a new chat on them starts only without it:{" "}
          {never.join("; ")}.
        </p>
      )}
      {trouble && (
        <p className="honest" role="alert">
          {trouble}
        </p>
      )}
      <button type="button" className="dismiss" tabIndex={0} onClick={() => answer(true)}>
        Turn the sandbox on
      </button>{" "}
      <button type="button" className="dismiss" tabIndex={0} onClick={() => answer(false)}>
        Keep it off
      </button>
    </div>
  );
}
