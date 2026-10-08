import { useState } from "react";
import type { DispatchArrived, PlaneId } from "./bindings";
import {
  arrivedSaid,
  letsSaid,
  listed,
  MOST_SAID,
  pairSaid,
  useDispatchArrival,
} from "./dispatchArrival";
import { Notice, type NoticeAction } from "./Notice";

/**
 * **What a target persona works with**, in the words the grant question uses (#1502's
 * `wants`).
 *
 * THE ONE PLACE FOR #1502: that code is not on this branch's base, so this says nothing today,
 * and until it does the Notice asks for a yes to "anything it can do" without showing what
 * that is. When #1502 lands, return its sentence for `target` here and the Notice says it
 * after each grant; and put a digest of those words in the grant's id in the core
 * (`arrived_id`), so a late answer re-checks them.
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
 * One Notice for everything that arrived together: the pairs listed, a grant naming a persona
 * this project does not define said as covering nothing. Until it is answered nothing listed
 * is in force for this person, and a chat that needs one of the pairs meanwhile is held and
 * asks on its own tab in the same words (`DispatchGrantNotice`); answering either clears both.
 *
 * **"Any persona" is told here and never accepted here** (V100-23). It has its own sentence,
 * which says where it is accepted: Settings › Project › Dispatch. Accept counts and sends
 * named pairs only, and the core refuses an "any persona" id on accept whatever is sent. Not
 * on my machine may decline it, since that narrows.
 *
 * **Anyone who can push can raise this Notice**, and can make it look routine. So:
 *
 * - **A re-ask never carries a new grant.** Where the list holds grants the person accepted
 *   before and new ones, each has its own Accept.
 * - **Accept is the first action only when every pair it accepts is written out in the line.**
 *   More than `MOST_SAID` pairs are summed, each listed under the line; until that list is
 *   opened, Not on my machine comes first and Accept after it.
 * - **Putting it away answers nothing.** Dismiss hides this list until it changes or the
 *   project is opened again; the chat's own question still stands.
 * - **An answer is for what was shown.** It sends each listed grant's id; the core answers
 *   only what is still exactly as it was shown, and where the list moved this says so and
 *   shows the list as it is now.
 *
 * **What a commit took away** is a Notice of its own that asks nothing: said once, and put
 * away with Dismiss. **Where the project's git history cannot be asked**, a third says that
 * no accepted project grant counts until it can.
 */
export function ProjectDispatchNotice({
  plane,
  onReview,
}: {
  plane: PlaneId;
  /** Opens Settings at the project's Dispatch section. */
  onReview: () => void;
}) {
  const { waiting, gone, unread, said, answer, told } = useDispatchArrival(plane);
  /** The list the person put away, by what it showed: another list is shown. */
  const [putAway, setPutAway] = useState<string>();
  /** What the last answer said that the person put away. */
  const [saidAway, setSaidAway] = useState<string>();
  /** The summed list the person opened, by what it showed. */
  const [opened, setOpened] = useState<string>();
  const [busy, setBusy] = useState(false);

  const shown = waiting.map((one) => one.id).join("\n");
  const usable = waiting.filter((one) => one.undefined === null);
  // What Accept may answer: named pairs. Never "any persona".
  const pairs = usable.filter((one) => !one.any);
  const fresh = pairs.filter((one) => one.again === null);
  const before = pairs.filter((one) => one.again !== null);
  const summed = pairs.length > MOST_SAID;

  const press = (accepted: boolean, these: readonly DispatchArrived[]) => () => {
    if (busy) return;
    setBusy(true);
    void answer(accepted, these).finally(() => setBusy(false));
  };
  const decline: NoticeAction = {
    label: "Not on my machine",
    onPress: press(false, waiting),
  };
  const accepts: NoticeAction[] =
    fresh.length > 0 && before.length > 0
      ? [
          { label: `Accept the ${fresh.length} new`, onPress: press(true, fresh) },
          {
            label: `Accept the ${before.length} you accepted before`,
            onPress: press(true, before),
          },
        ]
      : pairs.length > 0
        ? [
            {
              label: pairs.length === 1 ? "Accept" : `Accept all ${pairs.length}`,
              onPress: press(true, pairs),
            },
          ]
        : [];
  // Accept leads only where the line itself names every pair it accepts.
  const [first, ...rest] =
    summed && opened !== shown ? [decline, ...accepts] : [...accepts, decline];
  const fixes: readonly [NoticeAction, ...NoticeAction[]] = [first ?? decline, ...rest];

  const each = summed ? (
    <details
      className="block-report"
      onToggle={(event) => setOpened(event.currentTarget.open ? shown : undefined)}
    >
      <summary>Show all {pairs.length} pairs</summary>
      <ul>
        {pairs.map((one) => (
          <li key={one.id}>{pairSaid(one)}</li>
        ))}
      </ul>
    </details>
  ) : undefined;

  const arrived = waiting.length > 0 && shown !== putAway;
  return (
    <>
      {arrived && (
        <Notice
          cause="dispatch-grants"
          label="The project's dispatch grants changed"
          fixes={fixes}
          link={{
            label: pairs.length > 0 ? "Decide each in Settings" : "Open Dispatch settings",
            onPress: onReview,
          }}
          onDismiss={() => setPutAway(shown)}
          under={each}
        >
          <p>
            {arrivedSaid(waiting).join(" ")}
            {pairs
              .map((one) => worksWith(one.target))
              .filter((one) => one !== undefined)
              .map((one) => ` ${one}`)}
            {pairs.length > 0 &&
              ` ${pairs.length === 1 ? "The pair is not" : "None of the pairs is"} in force on this machine until you accept ${pairs.length === 1 ? "it" : "them"}. Accepting lets those chats ask the other persona for anything it can do, without asking you again.`}
            {" Not on my machine leaves the project's settings as they are for your teammates."}
            {said !== undefined && ` ${said}`}
          </p>
        </Notice>
      )}
      {!arrived && said !== undefined && said !== saidAway && (
        <Notice
          cause="dispatch-grants-answered"
          label="The project's dispatch grants changed"
          link={{ label: "Open Dispatch settings", onPress: onReview }}
          onDismiss={() => setSaidAway(said)}
        >
          <p>{said}</p>
        </Notice>
      )}
      {gone.length > 0 && (
        <Notice
          cause="dispatch-grants-gone"
          label="The project took dispatch grants away"
          onDismiss={() => told(gone)}
        >
          <p>
            The project no longer lets {listed(gone.map(letsSaid))}. You had accepted{" "}
            {gone.length === 1 ? "that" : "those"} on this machine; from now on{" "}
            {gone.length === 1 ? "it covers" : "they cover"} nothing here. There is nothing to
            answer.
          </p>
        </Notice>
      )}
      {unread && (
        <Notice
          cause="dispatch-grants-unread"
          label="The project's dispatch grants cannot be checked"
          link={{ label: "Open Dispatch settings", onPress: onReview }}
        >
          <p>
            purlis could not read this project&apos;s git history just now. Until it can, no
            dispatch grant of the project&apos;s that you accepted counts on this machine, and none
            can be accepted: a chat that needs one asks you on its own tab. Your own grants are as
            they were.
          </p>
        </Notice>
      )}
    </>
  );
}
