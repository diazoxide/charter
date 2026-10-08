import { useId, useState } from "react";
import { commands, type GrantLevel, type PlaneId } from "./bindings";
import { useDispatchesHeld } from "./dispatchesHeld";
import { Notice, type NoticeAction } from "./Notice";
import { Choice } from "./settings/components";

/** About how many lines the brief's box shows before it scrolls (`App.css`,
 *  `.block-report-brief`). */
const SHOWN_LINES = 8;

/** What each level's Allow says, in the order they read. */
const ALLOWS: readonly (readonly [GrantLevel, string])[] = [
  ["chat", "Allow for this chat"],
  ["you", "Allow for me on this machine"],
  ["project", "Allow for everyone in this project"],
];

/**
 * **A dispatch to another persona that no grant covers, on the asking chat's tab** (#1437).
 * Nothing has started. The Notice says who wants to dispatch to whom and shows the first brief
 * whole, and the person answers once: **Allow for this chat**, **Allow for me on this machine**,
 * **Allow for everyone in this project**, **Keep blocked**, or **Never for this pair**. After an
 * Allow the dispatch starts, and so does every later one the grant covers, with no prompt.
 *
 * **The two ways to say no** (#1503). Keep blocked holds for this chat's life: it is refused at
 * once if it asks again, and a new chat is asked. Never for this pair is kept for the person on
 * this machine: no chat of that persona is asked or allowed for that target until it is lifted
 * in Settings. It is offered where the chat runs as a persona, since a chat on none has no pair.
 * No answer here grants "any persona": that is Settings' alone.
 *
 * **Where the list of nevers does not read, the Notice says so** (`never_unread`): no grant
 * counts until it does, which is why a pair already granted is asked about again.
 *
 * **It reads top to bottom as it is answered** (#1481, #1502): the sentence, then the ways out
 * in the order above, then what the asking persona wants besides and what the target works
 * with, then the brief in a box of about eight lines that scrolls. The brief is under the
 * buttons and never beside or above them: a long brief must not push the answer out of sight.
 * On a pane it is the first Notice, above what purlis only reports (`PaneFrame`).
 *
 * **Several pairs in one answer** (#1502). A persona's definition may say which personas it
 * usually works with. That grants nothing; it puts a box under the answers for each of them
 * nothing answers for yet, **unticked**, with what that persona works with beside it. An Allow
 * keeps the asked pair and every ticked one at the level pressed. Keep blocked and Never for
 * this pair are about the asked pair only, and send no box.
 *
 * **The boxes and the access lines are the core's, read from the project's own files.** The
 * window draws the names and sentences it is told and sends back only the names ticked and the
 * digest of what it showed (`shown`): the core reads the files again at the answer, and an
 * answer to a question that reads differently now allows nothing and is shown again, with every
 * box unticked: a tick is for the words it was made under.
 *
 * **Allowing a dispatch is not allowing a secret**, and the Notice says so: what a persona's
 * chat does with a vault is asked as it was before.
 *
 * **The brief is the chat's text, never purlis's.** It is drawn in its own block, under a line
 * that says so, as plain text: nothing in it is markup, a control or a sentence of the
 * Notice's. An Allow sends the held dispatch's number and the level, and nothing of the pair:
 * the core holds who asked, from its own record of the chat.
 *
 * **Policy has the last word**: a pair an administrator's policy locks was refused already.
 * The Notice says so, with the policy's sentence and who set it, and offers no Allow.
 */
export function DispatchGrantNotice({ plane, session }: { plane: PlaneId; session: number }) {
  const id = useId();
  const { waiting, read } = useDispatchesHeld(plane, session);
  /** What the last Allow, or Never for this pair, answered, until it is put away. */
  const [allowed, setAllowed] = useState<{ target: string; said: string }>();
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  /** The boxes ticked, and the question they were ticked on: the held dispatch, and what it
   *  read as (`shown`). Another question, or this one once it reads differently, starts with
   *  none, so a tick is never carried onto words the person has not read. */
  const [ticks, setTicks] = useState<{
    on: number;
    shown: string;
    names: ReadonlySet<string>;
  }>();

  if (allowed !== undefined)
    return (
      <Notice
        cause={`dispatch-grant:${session}:allowed`}
        at="pane"
        tone="news"
        label={`Dispatch to ${allowed.target}`}
        onDismiss={() => {
          setAllowed(undefined);
          read();
        }}
      >
        {allowed.said}
      </Notice>
    );

  const [first, ...rest] = waiting;
  if (first === undefined) return null;
  const cause = `dispatch-grant:${session}:${first.id}`;
  const label = `Dispatch to ${first.target}`;
  const behind =
    rest.length > 0
      ? ` ${rest.length} more ${rest.length === 1 ? "dispatch is" : "dispatches are"} waiting behind this one.`
      : "";
  const who = first.asking === null ? "This chat" : `This chat runs as ${first.asking} and`;

  const putAway = () => {
    setSaid(undefined);
    void commands
      .keepDispatchBlocked(plane, first.id)
      .then(read)
      .catch(() => read());
  };

  // Only a box this question offers, ticked while it read as it does now, is ticked.
  const mine =
    ticks !== undefined && ticks.on === first.id && ticks.shown === first.shown
      ? ticks.names
      : undefined;
  const ticked = first.also.map((one) => one.persona).filter((name) => mine?.has(name));
  const tick = (name: string, on: boolean) => {
    const names = new Set(mine);
    if (on) names.add(name);
    else names.delete(name);
    setTicks({ on: first.id, shown: first.shown, names });
  };
  const asker = first.asking ?? "this chat";

  /** What the answer covers besides the asked pair, and what it does not. */
  const besides = first.locked === null && (
    <div className="dispatch-also">
      {first.also.length > 0 && (
        <>
          <p id={`${id}-also-label`}>Also let {asker} dispatch to:</p>
          <Choice
            kind="checks"
            ids={{ id: `${id}-also`, labelledBy: `${id}-also-label` }}
            options={first.also.map((one) => ({
              value: one.persona,
              label: one.persona,
              says: one.works_with,
            }))}
            checked={new Set(ticked)}
            onCheckedChange={tick}
          />
          <p>
            A ticked box is allowed with the answer you press, at the same level. Keep blocked and
            Never are about {first.target} only.
          </p>
        </>
      )}
      <p className="dispatch-works-with">{first.works_with}</p>
      <p>Allowing a dispatch does not allow the use of a secret: that is asked as before.</p>
    </div>
  );

  const brief = (
    <div className="block-report" id={id}>
      <p>The brief, as the chat wrote it. purlis did not write it.</p>
      <section aria-label="Brief from the chat">
        <pre className="block-report-draft block-report-brief">{first.brief}</pre>
      </section>
      {first.brief_lines > SHOWN_LINES && (
        <p>
          The brief is {first.brief_lines} lines. Scroll its box to read all of it before you
          answer.
        </p>
      )}
      {first.brief_cut && <p>The brief is longer than purlis shows here. The rest is not shown.</p>}
    </div>
  );

  if (first.locked !== null)
    return (
      <Notice
        cause={cause}
        at="pane"
        tone="trouble"
        label={label}
        onDismiss={putAway}
        under={brief}
      >
        {who} asked to dispatch to {first.target}. {first.locked}
        {behind}
      </Notice>
    );

  const allow = (level: GrantLevel) => {
    if (busy) return;
    setBusy(true);
    setSaid(undefined);
    void commands
      .allowDispatch(plane, first.id, level, ticked, first.shown)
      .then((done) => {
        if (done.status === "error") {
          setSaid(done.error);
          read();
        } else setAllowed({ target: first.target, said: done.data.said });
      })
      .catch((err: unknown) => setSaid(`purlis could not allow it: ${String(err)}`))
      .finally(() => setBusy(false));
  };
  const sayNever = () => {
    if (busy) return;
    setBusy(true);
    setSaid(undefined);
    void commands
      .neverDispatch(plane, first.id)
      .then((done) => {
        if (done.status === "error") {
          setSaid(done.error);
          read();
        } else setAllowed({ target: first.target, said: done.data.said });
      })
      .catch((err: unknown) => setSaid(`purlis could not keep it: ${String(err)}`))
      .finally(() => setBusy(false));
  };
  const keep: NoticeAction = { label: "Keep blocked", onPress: putAway };
  const never: NoticeAction[] =
    first.asking === null ? [] : [{ label: "Never for this pair", onPress: sayNever }];
  const allows: NoticeAction[] = ALLOWS.filter(([level]) => first.levels.includes(level)).map(
    ([level, label]) => ({ label, onPress: () => allow(level) }),
  );
  const [one, ...others] = [...allows, keep, ...never];
  const fixes: readonly [NoticeAction, ...NoticeAction[]] = [one ?? keep, ...others];

  return (
    <Notice
      cause={cause}
      at="pane"
      tone="trouble"
      label={label}
      fixes={fixes}
      under={
        <>
          {besides}
          {brief}
        </>
      }
    >
      {who} wants to dispatch to {first.target}. Nothing starts until you answer. Allowing it lets{" "}
      {first.asking === null ? "this chat" : `${first.asking} chats`} ask {first.target} for
      anything {first.target} can do, without asking you again. The grant covers the helper
      sub-agents {first.asking === null ? "this chat runs" : "those chats run"} too: what one of
      them asks is asked as its chat.
      {first.never_unread !== null &&
        ` ${first.never_unread} Allowing here starts this one dispatch, and the next one asks again.`}
      {said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}
