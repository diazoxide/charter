import { useEffect, useState } from "react";
import { commands, type PlaneId, type SharedFolder } from "./bindings";
import { useChatsHere, useChatsSelect } from "./chatState";
import { Notice } from "./Notice";

/** The chats the window has heard of, as one string: what changes when one starts or ends. */
const heardOf = (states: { bySession: Readonly<Record<number, unknown>> }) =>
  Object.keys(states.bySession).join(",");

/**
 * **Two tasks of this chat working in one folder with no branch of their own** (#1511,
 * V100-68), on the asking chat's pane: every task by name, and the way to keep them apart.
 * There are no file locks between tasks, so this is all that is said; nothing is stopped.
 * Read again whenever a chat starts or ends, and gone once only one task works there.
 *
 * Dismissed per folder and its tasks: a third task joining the folder is said again.
 */
export function TasksSharingNotice({ plane, session }: { plane: PlaneId; session: number }) {
  const chats = useChatsSelect(useChatsHere(), heardOf);
  const [shared, setShared] = useState<readonly SharedFolder[]>([]);
  const [dismissed, setDismissed] = useState<ReadonlySet<string>>(new Set());
  useEffect(() => {
    let live = true;
    void commands
      .tasksSharingAFolder(plane, session)
      .then((said) => {
        if (live && said.status === "ok") setShared(Array.isArray(said.data) ? said.data : []);
      })
      // A list that cannot be read warns of nothing.
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [plane, session, chats]);
  return (
    <>
      {shared.map((one) => {
        const cause = `tasks-share-a-folder:${session}:${one.folder}:${one.tasks.join("\u0000")}`;
        if (dismissed.has(cause)) return null;
        return (
          <Notice
            key={cause}
            cause={cause}
            at="pane"
            tone="news"
            label={`Tasks sharing ${one.folder}`}
            onDismiss={() => setDismissed((was) => new Set([...was, cause]))}
          >
            {one.says}
          </Notice>
        );
      })}
    </>
  );
}
