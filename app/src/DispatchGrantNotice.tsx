import { useId, useState } from "react";
import { commands, type GrantLevel, type PlaneId } from "./bindings";
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
 * **Allow for everyone in this project**, or **Keep blocked**. After an Allow the dispatch
 * starts, and so does every later one the grant covers, with no prompt.
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
  /** What the last Allow answered, until it is put away. */
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
        } else {
          setAllowed({ target: first.target, said: done.data.said });
          // Read again at once: the tab's hand reads the same list (#1486), and it must not
          // stay up for a dispatch that has been answered.
          read();
        }
      })
      .catch((err: unknown) => setSaid(`purlis could not allow it: ${String(err)}`))
      .finally(() => setBusy(false));
  };
  const keep: NoticeAction = { label: "Keep blocked", onPress: putAway };
  const allows: NoticeAction[] = ALLOWS.filter(([level]) => first.levels.includes(level)).map(
    ([level, label]) => ({ label, onPress: () => allow(level) }),
  );
  const [one, ...others] = [...allows, keep];
  const fixes: readonly [NoticeAction, ...NoticeAction[]] = [one ?? keep, ...others];

  return (
    <Notice cause={cause} at="pane" tone="trouble" label={label} fixes={fixes} under={brief}>
      {who} wants to dispatch to {first.target}. Nothing starts until you answer. Allowing it lets{" "}
      {first.asking === null ? "this chat" : `${first.asking} chats`} ask {first.target} for
      anything {first.target} can do, without asking you again. The grant covers the helpers{" "}
      {first.asking === null ? "this chat runs" : "those chats run"} too: what one of them asks is
      asked as its chat.
      {said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}
