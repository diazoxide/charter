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
 *
 * **Where an Allow holds** (#1505), said on every question. At the project's root there is no
 * workspace to limit a grant to: the sentence says an Allow for the person or the project
 * holds in any workspace, and those two buttons say it too. Where the workspace is not there
 * yet (`works_in_missing`), nothing is kept: one answer, **Allow this one dispatch**. Where
 * the pair is already allowed in other workspaces (`allowed_in`), the sentence says so first,
 * which is why the person is asked again. Where the task works in a workspace (`works_in`), every
 * Allow is for work in that workspace only, and the sentence says so. Under the answers and
 * above the brief the person may choose **In any workspace** for the two wider Allows; the
 * narrower one is preselected, and each new question starts from it. The narrower Allow is
 * `allow_dispatch`, which the core limits by its own record of the task; the wider one is a
 * command of its own, `allow_dispatch_anywhere`. The window never sends a workspace's name.
 */
export function DispatchGrantNotice({ plane, session }: { plane: PlaneId; session: number }) {
  const id = useId();
  const { waiting, read } = useDispatchesHeld(plane, session);
  /** What the last Allow, or Never for this pair, answered, until it is put away. */
  const [allowed, setAllowed] = useState<{ target: string; said: string }>();
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  /** The held dispatch the person chose "in any workspace" for. A later question is not it,
   *  so each starts from the narrower choice. */
  const [wide, setWide] = useState<number>();

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

  /** Whether an Allow for the person or the project can be limited to the task's workspace. */
  const wider = first.locked === null && first.levels.some((level) => level !== "chat");
  const limits = first.works_in !== null && !first.works_in_missing && wider;
  /** At the project's root there is no workspace to limit a grant to: the two wider Allows
   *  hold in any workspace, and each says so on its own button. */
  const atRoot = first.works_in === null && wider;
  /** The other workspaces this dispatch is already allowed in: why it is asked again. */
  const before =
    first.allowed_in.length === 0
      ? ""
      : ` You allowed this for work in ${first.allowed_in.join(", ")}.`;
  /** Where an Allow holds, in a sentence. */
  const holds =
    first.works_in === null
      ? atRoot
        ? `${before} This task works at the project's root, which is no workspace: an Allow for you or for the project holds in any workspace.`
        : before
      : first.works_in_missing
        ? `${before} ${first.works_in} is not a workspace of this project yet: an Allow starts this one dispatch and keeps no grant, so the next one asks again.`
        : `${before} ${before === "" ? "The" : "This"} task works in ${first.works_in}: an Allow holds for work there only${
            limits ? ", unless you choose any workspace below" : ""
          }.`;
  const anywhere = limits && wide === first.id;
  // A group of radios with its own label, and no fieldset: a fieldset is as wide as its
  // longest word in some engines, and this has to fit a narrow pane.
  const where = limits && (
    <div className="dispatch-within" role="radiogroup" aria-labelledby={`${id}-within`}>
      <p id={`${id}-within`}>Where an Allow for you or for the project holds</p>
      <label>
        <input
          type="radio"
          name={`${id}-within`}
          checked={!anywhere}
          onChange={() => setWide(undefined)}
        />{" "}
        In {first.works_in} only
      </label>
      <label>
        <input
          type="radio"
          name={`${id}-within`}
          checked={anywhere}
          onChange={() => setWide(first.id)}
        />{" "}
        In any workspace
      </label>
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
    // The wider answer is a command of its own, and only the two wider Allows have one.
    const run =
      anywhere && level !== "chat" ? commands.allowDispatchAnywhere : commands.allowDispatch;
    void run(plane, first.id, level)
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
    ([level, label]) => ({
      // An answer that keeps nothing is named as that, and one that holds everywhere says so.
      label: first.works_in_missing
        ? "Allow this one dispatch"
        : atRoot && level !== "chat"
          ? `${label}, in any workspace`
          : label,
      onPress: () => allow(level),
    }),
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
          {where}
          {brief}
        </>
      }
    >
      {who} wants to dispatch to {first.target}. Nothing starts until you answer. Allowing it lets{" "}
      {first.asking === null ? "this chat" : `${first.asking} chats`} ask {first.target} for
      anything {first.target} can do, without asking you again. The grant covers the helpers{" "}
      {first.asking === null ? "this chat runs" : "those chats run"} too: what one of them asks is
      asked as its chat.
      {holds}
      {first.never_unread !== null &&
        ` ${first.never_unread} Allowing here starts this one dispatch, and the next one asks again.`}
      {said !== undefined && ` ${said}`}
      {behind}
    </Notice>
  );
}
