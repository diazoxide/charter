import { useState } from "react";
import type { DispatchArrived, PlaneId } from "./bindings";
import { arrivedSaid, MOST_SAID, pairSaid, useDispatchArrival } from "./dispatchArrival";
import { Notice, type NoticeAction } from "./Notice";

/**
 * **What a target persona works with**, in the words the grant question uses (#1502's
 * `wants`).
 *
 * THE ONE PLACE FOR #1502: that code is not on this branch's base, so this says nothing today.
 * When it lands, return its sentence for `target` here and the Notice says it after each
 * grant, with no other change.
 */
function worksWith(target: string): string | undefined {
  void target;
  return undefined;
}

/**
 * **A teammate's dispatch grant arrived** (#1506, V100-62): the Notice the window shows, at
 * its own level and not on a chat's tab, when the project's settings come to let one persona's
 * chats dispatch to another and this person has not answered that yet. After a pull, a branch
 * switched, a hand's edit, and once when the project opens.
 *
 * One Notice for everything that arrived together: the pairs listed, "any persona" in its own
 * words, a grant naming a persona this project does not define said as covering nothing. Two
 * answers for all that is listed, **Accept** and **Not on my machine**, and **Decide each in
 * Settings**, which opens the Dispatch section. Until it is answered nothing listed is in force
 * for this person, and a chat that needs one of the grants meanwhile is held and asks on its
 * own tab in the same words (`DispatchGrantNotice`); answering either clears both.
 *
 * **Putting it away answers nothing.** Dismiss hides this list until it changes or the project
 * is opened again; nothing is accepted by it, and the chat's own question still stands.
 *
 * **An answer is for what was shown.** It sends each listed grant's id; the core answers only
 * what is still exactly as it was shown, and where the list moved meanwhile this says so and
 * shows the list as it is now.
 *
 * **Many pairs are summed** (more than `MOST_SAID`), with each one listed under the line, so a
 * flood cannot hide one grant among others. "Any persona" is never summed.
 *
 * **What the project took away** is a Notice of its own that asks nothing: said once, and put
 * away with Dismiss.
 */
export function ProjectDispatchNotice({
  plane,
  onReview,
}: {
  plane: PlaneId;
  /** Opens Settings at the project's Dispatch section. */
  onReview: () => void;
}) {
  const { waiting, gone, said, answer, told } = useDispatchArrival(plane);
  /** The list the person put away, by what it showed: another list is shown. */
  const [putAway, setPutAway] = useState<string>();
  const [busy, setBusy] = useState(false);

  const shown = waiting.map((one) => one.id).join("\n");
  const usable = waiting.filter((one) => one.undefined === null);
  const pairs = usable.filter((one) => !one.any);

  const press = (accepted: boolean, these: readonly DispatchArrived[]) => () => {
    if (busy) return;
    setBusy(true);
    void answer(accepted, these).finally(() => setBusy(false));
  };
  const decline: NoticeAction = {
    label: "Not on my machine",
    onPress: press(false, waiting),
  };
  const accept: NoticeAction[] =
    usable.length === 0
      ? []
      : [
          {
            label: usable.length === 1 ? "Accept" : `Accept all ${usable.length}`,
            onPress: press(true, usable),
          },
        ];
  const [first, ...rest] = [...accept, decline];

  const each =
    pairs.length > MOST_SAID ? (
      <details className="block-report">
        <summary>Show all {pairs.length} pairs</summary>
        <ul>
          {pairs.map((one) => (
            <li key={one.id}>{pairSaid(one)}</li>
          ))}
        </ul>
      </details>
    ) : undefined;

  return (
    <>
      {waiting.length > 0 && shown !== putAway && (
        <Notice
          cause="dispatch-grants"
          label="The project's dispatch grants changed"
          fixes={[first ?? decline, ...rest]}
          link={{ label: "Decide each in Settings", onPress: onReview }}
          onDismiss={() => setPutAway(shown)}
          under={each}
        >
          <p>
            {arrivedSaid(waiting).join(" ")}
            {usable
              .map((one) => worksWith(one.target))
              .filter((one) => one !== undefined)
              .map((one) => ` ${one}`)}
            {usable.length > 0 &&
              " None of it is in force on this machine until you accept it. Accepting lets those chats ask the other persona for anything it can do, without asking you again."}
            {" Not on my machine leaves the project's settings as they are for your teammates."}
            {said !== undefined && ` ${said}`}
          </p>
        </Notice>
      )}
      {gone.length > 0 && (
        <Notice
          cause="dispatch-grants-gone"
          label="The project took dispatch grants away"
          onDismiss={() => told(gone)}
        >
          <p>
            The project no longer lets {gone.map(pairSaid).join(", ")}. You had accepted{" "}
            {gone.length === 1 ? "it" : "them"} on this machine; from now on{" "}
            {gone.length === 1 ? "it covers" : "they cover"} nothing here. There is nothing to
            answer.
          </p>
        </Notice>
      )}
    </>
  );
}
