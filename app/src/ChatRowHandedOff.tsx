import { memo } from "react";
import { useChatsHere, useChatsSelect } from "./chatState";
import { shownOfRow } from "./chatsList";
import { handedOffSaid } from "./chatsTree";
import type { TaskFacts } from "./shownState";

/** What a row's press reads off the text it landed on: the chat that text goes to. */
export const GOES_TO = "data-goes-to";

/**
 * **Where a chat's work went, on the second line of its row** (#1492, V100-69): `handed off to
 * drop commons`, and `and 2 more` where it handed off to several. The newest is the one named,
 * by the name its own row says, so a rename is followed.
 *
 * **Said while the chat is not working.** The start of the second line is the one place a row
 * keeps for what its chat is doing (`ChatRowActivity`, #1493), and a chat at work says that
 * there. The order of precedence in that place: working, what it is doing; else where its work
 * went. What follows it on the line (workspace, time, own branch, where it came from) is
 * always there.
 *
 * **A press on it goes to that chat**: the row is one button, so this is text in it that says
 * where a press on it leads ({@link GOES_TO}), and the row's own press reads that. The
 * keyboard's way is the row's menu and the palette, which have a row for each chat the work
 * went to (`actions.handedOffRows`): those reach the "and 2 more" too, and work on one line,
 * where these words are only the row's tooltip.
 *
 * It reads its own chat's state, as the state mark does, so a chat starting or ending a turn
 * redraws this and no row (SC-3). It draws nothing, and the line keeps its height, for a chat
 * that handed nothing off.
 */
export const ChatRowHandedOff = memo(function ChatRowHandedOff({
  session,
  shell,
  report,
  outcome,
  asking,
  harness,
  to,
  name,
  more,
}: {
  session: number;
  /** What its state is derived from (`ChatShownState`): the same facts, the one function. */
  shell: boolean;
  report: TaskFacts["report"] | null;
  outcome: string | null;
  asking: string | null;
  harness: string | null;
  /** The newest chat its work was handed off to. */
  to: number;
  /** That chat's name, as its own row says it. */
  name: string;
  /** How many other open chats it handed off to. */
  more: number;
}) {
  const working = useChatsSelect(
    useChatsHere(),
    // The state its row says, by the function the list orders and filters its rows by.
    (states) =>
      shownOfRow(states, { session, shell, report, outcome, asking, harness })?.kind === "working",
  );
  if (working) return null;
  return (
    <span className="handed-off" {...{ [GOES_TO]: to }} title={`Go to ${name}`}>
      {handedOffSaid(name, more)}
    </span>
  );
});

/** The chat a press that landed on `target`, inside `row`, goes to in place of the row's own;
 *  nothing for a press anywhere else on the row. */
export function goesTo(target: EventTarget | null, row: Element): number | undefined {
  if (!(target instanceof Element)) return undefined;
  const text = target.closest(`[${GOES_TO}]`);
  if (text === null || !row.contains(text)) return undefined;
  const session = Number(text.getAttribute(GOES_TO));
  return Number.isInteger(session) ? session : undefined;
}
