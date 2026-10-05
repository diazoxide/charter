import { useEffect, useState } from "react";

import { commands, type VaultsToMove } from "./bindings";
import { Notice } from "./Notice";

/**
 * **The vaults the launch left under the old name, and the press that finishes moving them**
 * (#1306).
 *
 * At launch the app copies each project's keychain items to the new names with macOS's
 * Keychain dialogs off, so nothing asks before a window is even up (`vaultswaiting.rs`). An
 * item macOS would have asked about (one an earlier build made) is not read, and its vault stays
 * where it was, whole and working. This Notice says how many wait and offers to finish them:
 * only on that press does the copy run with the dialogs on, so macOS asks once for each secret
 * while the person is expecting it.
 *
 * An item the person does not allow keeps its vault waiting, and the Notice stays with the press
 * to try again. Dismissible: nothing is lost by leaving them, and the next launch says so again.
 */
export function VaultsWaitingNotice() {
  const [waiting, setWaiting] = useState<VaultsToMove | null>(null);
  const [moving, setMoving] = useState(false);
  /** What the last press could not finish, said with the Notice. */
  const [trouble, setTrouble] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let gone = false;
    void commands
      .vaultsToMove()
      .then((now) => {
        if (!gone) setWaiting(now);
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, []);

  if (!waiting || dismissed) return null;

  const finish = () => {
    setMoving(true);
    setTrouble(null);
    commands
      .finishMovingVaults()
      .then((answer) => {
        if (answer.status === "error") {
          setTrouble(answer.error);
          return;
        }
        const done = answer.data;
        setWaiting(done.left);
        if (done.left) {
          const failed = done.said
            .filter((line) => line.startsWith("✗"))
            .map((line) => line.slice(1).trim());
          setTrouble(
            failed.length > 0 ? failed.join(" ") : "macOS did not allow every secret to be read.",
          );
        }
      })
      .catch((why: unknown) => setTrouble(String(why)))
      .finally(() => setMoving(false));
  };

  const vaults = waiting.vaults === 1 ? "1 vault" : `${waiting.vaults} vaults`;
  const secrets = waiting.items === 1 ? "its secret" : `each of their ${waiting.items} secrets`;

  return (
    <Notice
      cause="vaults-waiting"
      tone={trouble ? "trouble" : "news"}
      fixes={moving ? undefined : [{ label: `Finish moving ${vaults}`, onPress: finish }]}
      onDismiss={() => setDismissed(true)}
    >
      {moving
        ? `Moving ${vaults}: macOS asks once for ${secrets}.`
        : `${vaults} still ${waiting.vaults === 1 ? "reads" : "read"} secrets under the old ` +
          `name, where they keep working. Moving ${waiting.vaults === 1 ? "it" : "them"} ` +
          `makes macOS ask once for ${secrets}.`}
      {trouble && !moving && ` ${trouble}`}
    </Notice>
  );
}
