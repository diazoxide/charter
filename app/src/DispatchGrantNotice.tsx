import { useContext, useEffect, useId, useRef, useState } from "react";
import { commands, type GrantLevel, type PlaneId } from "./bindings";
import { arrivedSaid, useDispatchArrival } from "./dispatchArrival";
import { useDispatchesHeld } from "./dispatchesHeld";
import { Notice, NoticeOf, type NoticeAction } from "./Notice";
import { Choice } from "./settings/components";

/** About how many lines the brief's box shows before it scrolls (`App.css`,
 *  `.block-report-brief`). */
const SHOWN_LINES = 8;

/** What the Notice says when its question read differently on a re-read while it was up. */
const BOXES_CHANGED =
  "What is offered under the answers changed while this was shown, so every box is unticked. Read it again before you answer.";

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
 * box unticked: a tick is for the words it was made under. **The boxes never change in place
 * with nothing said**: when the core's list is read again while the question is up and it reads
 * differently, the Notice says so and every box is unticked. The boxes stand in alphabetical
 * order, which is the core's, so where one stands is not a persona file's to choose.
 *
 * **Allowing a dispatch is not allowing a secret**, and the Notice says so: what a persona's
 * chat does with a vault is asked as it was before.
 *
 * **It names the task and the profile** (#1456): the task's name is the chat's text, drawn
 * above the brief and apart from purlis's words, like the brief; the profile the persona's
 * chat would start on is the app's choice, said in the sentence. The target persona's mark
 * leads the line (#1454).
 *
 * **The brief is the chat's text, never purlis's.** It is drawn in its own block, under a line
 * that says so, as plain text: nothing in it is markup, a control or a sentence of the
 * Notice's. An Allow sends the held dispatch's number and the level, and nothing of the pair:
 * the core holds who asked, from its own record of the chat.
 *
 * **Policy has the last word**: a pair an administrator's policy locks is refused by the
 * dispatch decision before any question is held, so the app's dispatch path raises no Notice
 * for it; the chat is told the policy's sentence (#1456). Should a held question ever say it
 * is locked (`locked`), the Notice says so, with the policy's sentence and who set it, and
 * offers no Allow.
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
  /** Drawn for a chat that is not on screen (#1538): its path is said first (`NoticeOf`), so
   *  the sentence says "it", and never "this chat", which would read as the chat on screen. */
  const self = useContext(NoticeOf) === null ? "this chat" : "it";
  const Self = self === "it" ? "It" : "This chat";
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
  /** The held dispatch the person chose "in any workspace" for. A later question is not it,
   *  so each starts from the narrower choice. */
  const [wide, setWide] = useState<number>();
  /** The boxes ticked, and the question they were ticked on: the held dispatch, and what it
   *  read as (`shown`). Another question, or this one once it reads differently, starts with
   *  none, so a tick is never carried onto words the person has not read. */
  const [ticks, setTicks] = useState<{
    on: number;
    shown: string;
    names: ReadonlySet<string>;
  }>();
  /** The question last drawn, and the held dispatch whose question changed while it was up. */
  const [drawn, setDrawn] = useState<{ on: number; shown: string }>();
  const [moved, setMoved] = useState<number>();

  if (allowed !== undefined)
    return (
      <Notice
        cause={`dispatch-grant:${session}:allowed`}
        at="pane"
        tone="news"
        label={`Dispatch to ${allowed.target}`}
        persona={allowed.target}
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
  // The same held dispatch, reading differently than when it was last drawn: said on the
  // Notice, so a box is never swapped under the pointer with no word. Adjusted while
  // rendering, as React has state follow what it is drawn from.
  if (drawn?.on !== first.id || drawn.shown !== first.shown) {
    if (drawn?.on === first.id) setMoved(first.id);
    setDrawn({ on: first.id, shown: first.shown });
  }
  const cause = `dispatch-grant:${session}:${first.id}`;
  const label = `Dispatch to ${first.target}`;
  const behind =
    rest.length > 0
      ? ` ${rest.length} more ${rest.length === 1 ? "dispatch is" : "dispatches are"} waiting behind this one.`
      : "";
  const who = first.asking === null ? Self : `${Self} runs as ${first.asking} and`;

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
  const asker = first.asking ?? self;

  /** What the answer covers besides the asked pair, and what it does not. */
  const besides = first.locked === null && (
    <div className="dispatch-also">
      {first.also.length > 0 && (
        <>
          <p id={`${id}-also-label`}>Also let {asker} dispatch to:</p>
          <Choice
            kind="checks"
            ids={{
              id: `${id}-also`,
              labelledBy: `${id}-also-label`,
              describedBy: `${id}-also-says`,
            }}
            options={first.also.map((one) => ({
              value: one.persona,
              label: one.persona,
              says: one.works_with,
            }))}
            checked={new Set(ticked)}
            onCheckedChange={tick}
          />
          <p id={`${id}-also-says`}>
            A ticked box is allowed with the Allow you press, for the same people and the same
            workspace as that answer. Keep blocked and Never are about {first.target} only.
          </p>
        </>
      )}
      <p className="dispatch-works-with">{first.works_with}</p>
      <p>Allowing a dispatch does not allow the use of a secret: that is asked as before.</p>
    </div>
  );

  const brief = (
    <div className="block-report" id={id}>
      {first.task !== null && (
        // The task's name is the chat's text too (#1456): drawn apart from purlis's words.
        <p>
          The task, as the chat named it:{" "}
          <span className="block-allow-target" aria-label="Task name from the chat">
            {first.task}
          </span>
          {first.task_cut && " (cut: purlis shows the start of it)"}
        </p>
      )}
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
        persona={first.target}
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
    setMoved(undefined);
    // The wider answer is a command of its own, and only the two wider Allows have one. The
    // ticked boxes go with either: each is kept where the answer itself holds.
    const run =
      anywhere && level !== "chat" ? commands.allowDispatchAnywhere : commands.allowDispatch;
    void run(plane, first.id, level, ticked, first.shown)
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
  ).map(([level, label]) => ({
    // An answer that keeps nothing is named as that, and one that holds everywhere says so.
    label: first.works_in_missing
      ? "Allow this one dispatch"
      : atRoot && level !== "chat"
        ? `${label}, in any workspace`
        : label,
    onPress: () => allow(level),
  }));
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
    <Notice
      cause={cause}
      at="pane"
      tone="trouble"
      label={label}
      persona={first.target}
      fixes={fixes}
      under={
        <>
          {where}
          {besides}
          {brief}
        </>
      }
    >
      {here.length > 0 &&
        `${arrivedSaid(here).join(" ")}${pair.length > 0 ? " You have not answered that on this machine." : ""} `}
      {who} wants to dispatch to {first.target}. Nothing starts until you answer. Allowing it lets{" "}
      {first.asking === null ? self : `${first.asking} chats`} ask {first.target} for anything{" "}
      {first.target} can do, without asking you again. The grant covers the helpers{" "}
      {first.asking === null ? `${self} runs` : "those chats run"} too: what one of them asks is
      asked as its chat.
      {holds}
      {first.profile !== null &&
        ` Its ${first.target} chat would start on profile ${first.profile}.`}
      {first.also.length > 0 &&
        " Under the answers are boxes for more personas: tick any you want before you press Allow."}
      {first.never_unread !== null &&
        ` ${first.never_unread} Allowing here starts this one dispatch, and the next one asks again.`}
      {said !== undefined && ` ${said}`}
      {moved === first.id && ` ${BOXES_CHANGED}`}
      {here.length > 0 && arrival.said !== undefined && ` ${arrival.said}`}
      {behind}
    </Notice>
  );
}
