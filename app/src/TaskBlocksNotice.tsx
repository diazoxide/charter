import { useEffect, useId, useRef, useState } from "react";
import { commands, type GrantLevel, type PlaneId } from "./bindings";
import { Notice, type NoticeAction } from "./Notice";
import { sandboxCommandReturned } from "./sandboxAsked";
import { listed, type Member, type TaskBlockGroup } from "./taskAsks";

/**
 * How long after the tasks a question lists changed an answer to it grants nothing: long
 * enough that a press aimed at what was there before lands on a refusal, not on the new list.
 */
export const SETTLE_MS = 1500;

/**
 * **One question for the tasks of one session that hit the same block** (#1508, V100-57), on
 * the session's tab: "3 tasks want to reach registry.npmjs.org", each task named by its whole
 * path, and one answer that applies to each task listed and to no other chat.
 *
 * - **Allow for these tasks** grants each task listed its own grant ("this chat" for each of
 *   them), never the session that asked them: a session's permission and its tasks' are apart.
 *   **Always allow…** keeps one grant for every chat here, as a block's own does, and every
 *   task listed restarts to take it. **Keep blocked** answers each task listed.
 * - **An answer is to what was shown.** The press sends the tasks drawn at that moment, and
 *   the core grants only to those it records as tasks of this session. A task that hits the
 *   same block later joins the question visibly ("probe joined this question"), and an answer
 *   pressed within {@link SETTLE_MS} of that grants nothing and says why. Once answered, the
 *   blocks go, so a task blocked after that is asked in a new question.
 */
export function TaskBlocksNotice({
  plane,
  group,
  onAnswered,
  onKeepBlocked,
}: {
  plane: PlaneId;
  group: TaskBlockGroup;
  /** Allowed: the tasks listed are owed a restart, and the core said `said`. */
  onAnswered: (members: readonly Member[], said: string) => void;
  onKeepBlocked: (members: readonly Member[]) => void;
}) {
  const id = useId();
  const [alwaysOpen, setAlways] = useState(false);
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  const count = group.members.length;
  const listing = group.members.map((one) => one.session).join(",");
  /** The tasks as first shown, and when the list last changed, with who joined it. */
  const shown = useRef({ listing, names: new Set(group.members.map((one) => one.session)) });
  const [changed, setChanged] = useState<{ at: number; joined: string[] }>();
  useEffect(() => {
    if (listing === shown.current.listing) return;
    const joined = group.members
      .filter((one) => !shown.current.names.has(one.session))
      .map((one) => one.whose);
    shown.current = { listing, names: new Set(group.members.map((one) => one.session)) };
    setChanged({ at: Date.now(), joined });
    setSaid(undefined);
  }, [group.members, listing]);

  const allow = (level: GrantLevel) => {
    if (busy) return;
    if (changed !== undefined && Date.now() - changed.at < SETTLE_MS) {
      setSaid(
        "This question changed just now, so nothing was allowed. Read the tasks it lists and " +
          "answer again.",
      );
      return;
    }
    const members = group.members;
    setBusy(true);
    setSaid(undefined);
    void commands
      .allowSandboxBlockForTasks(
        plane,
        group.session,
        members.map((one) => one.session),
        group.offer,
        group.target,
        level,
      )
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else onAnswered(members, done.data.said);
      })
      .catch((err: unknown) => setSaid(`purlis could not allow it: ${String(err)}`))
      .finally(() => {
        setBusy(false);
        sandboxCommandReturned();
      });
  };
  const allowsAt = (level: GrantLevel) => group.levels.includes(level);
  const always = allowsAt("you") || (group.offer === "host" && allowsAt("project"));
  const target = <code className="block-allow-target">{group.target}</code>;

  const under = (
    <div className="block-allow" id={id}>
      <ul aria-label="Tasks this answers">
        {group.members.map((one) => (
          <li key={one.session}>{one.whose}</li>
        ))}
      </ul>
      {alwaysOpen && (
        <div className="block-allow-actions">
          {allowsAt("you") && (
            <button type="button" tabIndex={0} disabled={busy} onClick={() => allow("you")}>
              Allow for me on this machine
            </button>
          )}
          {group.offer === "host" && allowsAt("project") && (
            <button type="button" tabIndex={0} disabled={busy} onClick={() => allow("project")}>
              Allow for everyone in this project
            </button>
          )}
        </div>
      )}
    </div>
  );
  const keep: NoticeAction = {
    label: "Keep blocked",
    onPress: () => onKeepBlocked(group.members),
  };
  const allows: NoticeAction[] = [
    ...(allowsAt("chat")
      ? [{ label: `Allow for these ${count} tasks`, onPress: () => allow("chat") }]
      : []),
    ...(always
      ? [
          {
            label: "Always allow…",
            onPress: () => setAlways((was) => !was),
            opens: { id, open: alwaysOpen },
          },
        ]
      : []),
  ];
  const [first, ...rest] = [...allows, keep];
  const fixes: readonly [NoticeAction, ...NoticeAction[]] = [first ?? keep, ...rest];
  return (
    <Notice
      cause={`sandbox-blocked-tasks:${group.session}:${group.key}`}
      at="pane"
      tone="trouble"
      label={`Sandbox block for ${count} tasks`}
      fixes={fixes}
      under={under}
    >
      {count} tasks want to {group.offer === "host" ? "reach " : "write "}
      {target}
      {group.offer === "write" && " and everything in it"}:{" "}
      {listed(group.members.map((one) => one.whose))}. The sandbox blocked {group.said} in each. One
      answer applies to each task listed, and to no other chat.
      {changed !== undefined &&
        changed.joined.length > 0 &&
        ` ${listed(changed.joined)} joined this question after it was first shown.`}
      {said !== undefined && ` ${said}`}
    </Notice>
  );
}

/** What one answer to several tasks allowed, until it is put away. */
export function TaskBlocksAnswered({
  target,
  said,
  onDismiss,
}: {
  target: string;
  said: string;
  onDismiss: () => void;
}) {
  return (
    <Notice
      cause={`sandbox-blocked-tasks-allowed:${target}`}
      at="pane"
      tone="news"
      label="Sandbox block"
      onDismiss={onDismiss}
    >
      <code className="block-allow-target">{target}</code>: {said}
    </Notice>
  );
}
