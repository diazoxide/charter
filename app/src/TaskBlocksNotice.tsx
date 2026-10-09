import { useId, useState } from "react";
import { commands, type GrantLevel, type PlaneId, type SeenBlock } from "./bindings";
import { Notice, type NoticeAction } from "./Notice";
import { sandboxCommandReturned } from "./sandboxAsked";
import { listed, type Member, type TaskBlockGroup } from "./taskAsks";

/**
 * How long after a question formed, or after the tasks it lists changed, a press on it does
 * nothing: a guard against a press aimed at what was there before. It only blunts a misclick;
 * what binds an answer to what was shown is the core's check of each task's block (D-1508-9).
 */
export const SETTLE_MS = 1500;

/**
 * The time a list is drawn at. Read in the render that draws a changed list, once per change
 * (the state it is kept in changes with it), so that render and every press on it know it.
 */
const drawnAt = () => Date.now();

/** What the window sends back for each task: the block it showed for it, exactly. */
function seenOf(members: readonly Member[]): SeenBlock[] {
  return members.map((one) => ({
    task: one.session,
    operation: one.block.operation,
    kind: one.block.kind,
    what: one.block.offer === "write" ? "write" : "host",
    target: one.block.target ?? "",
  }));
}

/**
 * **One question for the tasks of one session that hit the same block** (#1508, V100-57), on
 * the session's tab: "3 tasks want to reach registry.npmjs.org", each task named by its whole
 * path, and one answer that applies to each task listed and to no other chat.
 *
 * - **Allow for these tasks** grants each task listed its own grant ("this chat" for each of
 *   them), never the session that asked them: a session's permission and its tasks' are apart.
 *   **Always allow…** keeps one grant for every chat here, as a block's own does, and every
 *   task listed restarts to take it. **Keep blocked** answers each task listed.
 * - **An answer is to what was shown** (D-1508-9). Every press sends each task with the block
 *   shown for it, and the core refuses the whole answer unless each task is held on exactly
 *   that block now and is recorded as a task of this session. A task that hits the same block
 *   later joins the question visibly ("probe joined this question"). For {@link SETTLE_MS}
 *   after the question forms or changes, a press does nothing and says why.
 */
export function TaskBlocksNotice({
  plane,
  group,
  onAnswered,
  onKeepBlocked,
}: {
  plane: PlaneId;
  group: TaskBlockGroup;
  /** The core answered `members` (all those listed, or those it allowed before keeping failed
   *  part way): they are owed a restart, and the core said `said`. */
  onAnswered: (members: readonly Member[], said: string) => void;
  /** The core let `members`' blocks go, kept blocked. */
  onKeepBlocked: (members: readonly Member[]) => void;
}) {
  const id = useId();
  const [alwaysOpen, setAlways] = useState(false);
  const [said, setSaid] = useState<string>();
  const [busy, setBusy] = useState(false);
  const count = group.members.length;
  const listing = group.members.map((one) => `${one.session}:${one.block.target ?? ""}`).join(",");
  /**
   * **When this list was drawn**, set in the very render that draws it, so no press can land on
   * a list whose time is not yet known: at the question's forming, and at every change of who
   * it lists, with who joined. React's way to keep what a previous render showed.
   */
  const [drawn, setDrawn] = useState(() => ({
    listing,
    at: Date.now(),
    sessions: new Set(group.members.map((one) => one.session)),
    joined: [] as string[],
  }));
  if (drawn.listing !== listing) {
    setDrawn({
      listing,
      at: drawnAt(),
      sessions: new Set(group.members.map((one) => one.session)),
      joined: group.members
        .filter((one) => !drawn.sessions.has(one.session))
        .map((one) => one.whose),
    });
    setSaid(undefined);
  }
  const joined = drawn.joined;
  /** Whether a press now is too soon after what it is on was drawn. */
  const tooSoon = () => {
    if (Date.now() - drawn.at < SETTLE_MS) {
      setSaid(
        "This question was drawn or changed just now, so nothing was answered. Read the tasks " +
          "it lists and answer again.",
      );
      return true;
    }
    return false;
  };

  const allow = (level: GrantLevel) => {
    if (busy || tooSoon()) return;
    const members = group.members;
    setBusy(true);
    setSaid(undefined);
    void commands
      .allowSandboxBlockForTasks(plane, group.session, seenOf(members), level)
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else
          onAnswered(
            members.filter((one) => done.data.answered.includes(one.session)),
            done.data.said,
          );
      })
      .catch((err: unknown) => setSaid(`purlis could not allow it: ${String(err)}`))
      .finally(() => {
        setBusy(false);
        sandboxCommandReturned();
      });
  };
  const keepBlocked = () => {
    if (busy || tooSoon()) return;
    const members = group.members;
    setBusy(true);
    setSaid(undefined);
    void commands
      .keepSandboxBlockForTasks(plane, group.session, seenOf(members))
      .then((done) => {
        if (done.status === "error") setSaid(done.error);
        else onKeepBlocked(members.filter((one) => done.data.includes(one.session)));
      })
      .catch((err: unknown) => setSaid(`purlis could not keep it blocked: ${String(err)}`))
      .finally(() => setBusy(false));
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
  const keep: NoticeAction = { label: "Keep blocked", onPress: keepBlocked };
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
      {joined.length > 0 && ` ${listed(joined)} joined this question after it was first shown.`}
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
