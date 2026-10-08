import { useEffect, useId, useRef, useState } from "react";
import { commands, type GrantLevel, type PlaneId } from "./bindings";
import { arrivedSaid, useDispatchArrival } from "./dispatchArrival";
import { useDispatchesHeld } from "./dispatchesHeld";
import { Notice, type NoticeAction } from "./Notice";

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
 * **Where the project already grants it and the person has not answered that** (#1506), this
 * is the fallback for the Notice that said it arrived, which the person put away or missed. It
 * says the same thing in the same words (`arrivedSaid`), and for the pair it offers the same
 * two answers: **Accept**, in place of allowing it for everyone, since the project's settings
 * hold it already, and **Not on my machine**. Answering here clears that Notice, and answering
 * that one clears this. A project's "any persona" is said here with where it is accepted,
 * Settings: no Notice accepts it (V100-23). After Not on my machine the dispatch is still
 * held and this says so; the project's level is then not offered for the pair, which would
 * undo the answer by another name.
 *
 * **Where the list of nevers does not read, the Notice says so** (`never_unread`): no grant
 * counts until it does, which is why a pair already granted is asked about again.
 *
 * **It reads top to bottom as it is answered** (#1481): the sentence, then the ways out in the
 * order above, then the brief in a box of about eight lines that scrolls. The brief is under
 * the buttons and never beside or above them: a long brief must not push the answer out of
 * sight. On a pane it is the first Notice, above what purlis only reports (`PaneFrame`).
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
  const arrival = useDispatchArrival(plane);
  // What waits of the project's grants moved (an answer on the window's own Notice, or in
  // Settings): what this chat has held may have started, or lost its grant.
  const arrived = arrival.waiting;
  const waits = arrived.map((one) => one.id).join("\n");
  const readFor = useRef(waits);
  useEffect(() => {
    if (readFor.current === waits) return;
    readFor.current = waits;
    read();
  }, [waits, read]);
  /** What the last Allow, or Never for this pair, answered, until it is put away. */
  const [allowed, setAllowed] = useState<{ target: string; said: string }>();
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);

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
      .allowDispatch(plane, first.id, level)
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
  // The project's grants that would cover this dispatch and wait for the person's answer.
  const here = arrived.filter(
    (one) => one.asking === first.asking && (one.any || one.target === first.target),
  );
  const pair = here.filter((one) => !one.any && one.undefined === null);
  const answer = (accepted: boolean) => () => {
    if (busy) return;
    setBusy(true);
    setSaid(undefined);
    void arrival
      .answer(accepted, pair)
      .then(() => {
        if (!accepted)
          setSaid(
            `Not followed on this machine. This dispatch to ${first.target} still waits for your answer here.`,
          );
        read();
      })
      .finally(() => setBusy(false));
  };
  const keep: NoticeAction = { label: "Keep blocked", onPress: putAway };
  const never: NoticeAction[] =
    first.asking === null ? [] : [{ label: "Never for this pair", onPress: sayNever }];
  // The project's settings hold the pair already: for everyone is the project's own grant,
  // accepted here, and never a second write of it.
  const allows: NoticeAction[] = ALLOWS.filter(
    ([level]) => first.levels.includes(level) && !(level === "project" && pair.length > 0),
  ).map(([level, label]) => ({ label, onPress: () => allow(level) }));
  const project: NoticeAction[] =
    pair.length > 0
      ? [
          { label: "Accept", onPress: answer(true) },
          { label: "Not on my machine", onPress: answer(false) },
        ]
      : [];
  const [one, ...others] = [...allows, ...project, keep, ...never];
  const fixes: readonly [NoticeAction, ...NoticeAction[]] = [one ?? keep, ...others];

  return (
    <Notice cause={cause} at="pane" tone="trouble" label={label} fixes={fixes} under={brief}>
      {here.length > 0 &&
        `${arrivedSaid(here).join(" ")}${pair.length > 0 ? " You have not answered that on this machine." : ""} `}
      {who} wants to dispatch to {first.target}. Nothing starts until you answer. Allowing it lets{" "}
      {first.asking === null ? "this chat" : `${first.asking} chats`} ask {first.target} for
      anything {first.target} can do, without asking you again. The grant covers the helper
      sub-agents {first.asking === null ? "this chat runs" : "those chats run"} too: what one of
      them asks is asked as its chat.
      {first.never_unread !== null &&
        ` ${first.never_unread} Allowing here starts this one dispatch, and the next one asks again.`}
      {said !== undefined && ` ${said}`}
      {here.length > 0 && arrival.said !== undefined && ` ${arrival.said}`}
      {behind}
    </Notice>
  );
}
