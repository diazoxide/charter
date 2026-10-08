import { useEffect, useState } from "react";
import { commands, type PlaneId } from "./bindings";
import { seenElsewhereOnThisMachine, useSeenOnThisMachine } from "./dismissals";
import { Notice } from "./Notice";

/**
 * **The first dispatch on a machine explains the chip, once** (#1501, V100-55).
 *
 * The first time a session the person is at has a task, one Notice on that session's tab says
 * that its tasks work inside the tab, and how to see them: the chip beside the tab's name. It
 * goes on Dismiss and on its own action (which opens the chip's menu), and either is kept as
 * seen **on this machine** (`dismissals.ts`, {@link useSeenOnThisMachine}): the person's own
 * layout file, where what they have dismissed is already kept, and never a project, which
 * would carry it to every clone. It is added by the core under the file's lock, on the
 * person's press, and the file is asked again before the Notice is drawn, so a second window
 * open at once does not show it a second time. No chat can set or clear it.
 *
 * - **Never to a chat nobody is at** (`chat_attended`): a chat whose harness runs with its
 *   prompts off has nobody to read it, and is told nothing, as it is told of no dispatch. Its
 *   tasks do not use the once up, so the next session a person is at still has it explained.
 *   Until the core answers, or when it cannot, nothing is drawn.
 * - **Never in the person's way.** A Notice is drawn and never focused, so whoever is typing
 *   goes on typing. It stands in the pane's corner, and the chip's menu is drawn over the
 *   whole window (`App.css`, `#root`), so it never covers the menu. Nothing about it moves, so
 *   there is nothing for reduced motion to still.
 * - **On the session's own pane**, while its tab shows the session: a task shown in the tab
 *   says where it came from in its breadcrumb, and has nothing here to say.
 */

/** What the tab in front says of its tasks, for the Notice: the session whose tab it is, how
 *  many tasks its chip counts, and the way to open the chip's menu. The project's view
 *  (`PlaneView`) works it out for its tab in front, and has none for a tab with no tasks. */
export type ChipToExplain = { session: number; tasks: number; onShow: () => void };

/** The Notice's cause: what the person has seen, kept on this machine. */
export const CHIP_EXPLAINED = "chip-explained";

/** What it says, for `tasks` tasks: the four states are the chip's (`taskBuckets.ts`). */
export function chipExplained(tasks: number): string {
  return tasks === 1
    ? "This chat started a task, and it works inside this tab. The chip beside the tab's name " +
        "says whether it is working, waiting, failed or done; press the chip to switch to it."
    : `This chat started ${tasks} tasks, and they work inside this tab. The chip beside the ` +
        "tab's name counts them as working, waiting, failed and done; press the chip to switch " +
        "between them.";
}

/** The Notice, on the pane showing chat `session`, when `chip` is that chat's. */
export function ChipExplained({
  plane,
  session,
  chip,
}: {
  plane: PlaneId;
  session: number;
  chip: ChipToExplain | undefined;
}) {
  const { seen, see } = useSeenOnThisMachine(CHIP_EXPLAINED);
  const ours = chip !== undefined && chip.session === session && chip.tasks > 0 && !seen;
  const owed = useOwed(plane, ours ? session : undefined, chip?.tasks ?? 0);
  if (!ours || owed !== true) return null;
  const one = chip.tasks === 1;
  return (
    <Notice
      cause={CHIP_EXPLAINED}
      at="pane"
      label="Tasks in this tab"
      fixes={[
        {
          label: one ? "Show the task" : "Show the tasks",
          onPress: () => {
            see();
            chip.onShow();
          },
        },
      ]}
      onDismiss={see}
    >
      {chipExplained(chip.tasks)}
    </Notice>
  );
}

/**
 * Whether the Notice is owed on chat `session`: a person is at it, as the core holds it, and
 * the layout file does not say it was seen on this machine since this window launched (by
 * another window: then it is held as seen here too). `undefined` until both answer; asked
 * again as its tasks change, since a chat's harness may since have said its prompts are off.
 * Nothing is asked for no chat, and anything but a plain yes and a plain no is no.
 */
function useOwed(plane: PlaneId, session: number | undefined, tasks: number) {
  const [answer, setAnswer] = useState<{ session: number; owed: boolean }>();
  useEffect(() => {
    if (session === undefined) return;
    let current = true;
    const failed = { status: "error" as const, error: "" };
    void Promise.all([
      commands.chatAttended(plane, session).catch(() => failed),
      commands.seenOnThisMachine(CHIP_EXPLAINED).catch(() => failed),
    ]).then(([attended, seen]) => {
      if (seen.status === "ok" && seen.data === true) seenElsewhereOnThisMachine(CHIP_EXPLAINED);
      const owed =
        attended.status === "ok" &&
        attended.data === true &&
        seen.status === "ok" &&
        seen.data === false;
      if (current) setAnswer({ session, owed });
    });
    return () => {
      current = false;
    };
  }, [plane, session, tasks]);
  return answer !== undefined && answer.session === session ? answer.owed : undefined;
}
