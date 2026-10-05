import { useEffect, useState } from "react";
import Markdown from "react-markdown";
import { LoaderCircle } from "lucide-react";
import { EmptyState } from "./EmptyState";
import { COMPONENTS } from "./SessionRecordTab";
import { Facts } from "./Views";
import { commands, type PlaneId, type TodoView } from "./bindings";
import type { TodoRef } from "./todos";

/**
 * **One todo, in a tab of its own** (#1214): the todo's facts — its workspace, when it was
 * opened, its state — and its whole text, rendered as the Markdown it is. The title is the
 * view's heading (`Views.ViewPane`), and so are its actions: **Mark done** and **Forget**, the
 * catalogue's `todo.done:<slug>` and `todo.forget:<slug>` rows (`Views.OWN_ROWS`), so the
 * heading, the row's menu and the palette are one verb.
 *
 * **Its state is always Open**, and that is the store's rule, not a shortcut: a todo has no state
 * field, and a closed one is a deleted file whose trace is the journal's (ADR 0004). So there is
 * no Reopen, and a tab whose todo was closed or forgotten since — here, by a chat or in a
 * terminal — says the todo is not open any more, rather than drawing a state nothing records.
 *
 * **No HTML**, for `SessionRecordTab`'s reason: a todo is a file a chat or a terminal wrote.
 */
export function TodoTab({
  plane,
  at,
  changed,
}: {
  plane: PlaneId;
  at: TodoRef;
  /** Bumped when the plane changes on disk: the tab reads again. */
  changed: number;
}) {
  const { workspace, slug } = at;
  const [said, setSaid] = useState<{ todo?: TodoView | null; trouble?: string }>();
  useEffect(() => {
    let gone = false;
    void commands
      .todoRead(plane, workspace, slug)
      .then((answer) => {
        if (gone) return;
        setSaid(answer.status === "error" ? { trouble: answer.error } : { todo: answer.data });
      })
      .catch((err: unknown) => {
        if (!gone) setSaid({ trouble: String(err) });
      });
    return () => {
      gone = true;
    };
  }, [plane, workspace, slug, changed]);

  if (said === undefined) {
    return (
      <p className="pending" aria-busy="true">
        <LoaderCircle className="node-icon spinning" />
        Reading the todo…
      </p>
    );
  }
  if (said.trouble !== undefined) {
    return (
      <p className="trouble" role="alert">
        {said.trouble}
      </p>
    );
  }
  const todo = said.todo;
  if (todo === null || todo === undefined) {
    // **Not an error**: closed as done or forgotten since its tab opened.
    return (
      <EmptyState
        headline="This todo is not open any more"
        body={`It was closed or forgotten in ${workspace}. A todo closed as done is in the workspace's journal.`}
        testid="view-gone"
      />
    );
  }
  return (
    <>
      <Facts
        facts={{
          kind: "facts",
          facts: [
            { label: "Workspace", value: todo.workspace },
            { label: "Opened", value: todo.stamp === "" ? "not recorded" : todo.stamp },
            { label: "State", value: "Open" },
          ],
        }}
      />
      <article className="release-notes" data-testid="todo-body">
        <Markdown skipHtml components={COMPONENTS}>
          {todo.body.trim() === "" ? todo.title : todo.body}
        </Markdown>
      </article>
    </>
  );
}
