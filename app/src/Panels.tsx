import { NeedsYou } from "./NeedsYou";
import type { WorkspaceState } from "./workspaceState";

/**
 * The right region: **what is asking for you** (charter ADR 0038).
 *
 * The needs-you queue, alerts, the workspace's todos and the plane's personas. It is the
 * same `<aside className="panels">` that has been here all along, re-tenanted: the repos and
 * the CI it used to hold are state, and state went to the bottom bar. The queue came the
 * other way, out of `<header className="bar">` where it was sharing a line with the tab
 * strip, the `+`, the split buttons and the plane path.
 *
 * **Not read-only any more, and the queue is the reason.** Every chat in it is a button that
 * brings that chat forward — the one surface in this window ADR 0038 says must never be
 * competed with. What stayed read-only is the bottom bar.
 */
export function Panels({
  workspace,
  state,
  queue,
  quiet,
  nameOf,
  showChat,
}: {
  /** The focused workspace, whose todos these are. The queue is not its — it is every
   *  workspace's, because a chat asking for you in a workspace nobody is looking at is
   *  exactly the one that must not be hidden. */
  workspace: string | undefined;
  state: WorkspaceState;
  queue: readonly number[];
  quiet: readonly string[];
  nameOf: (session: number) => string;
  showChat: (session: number) => void;
}) {
  const { panels, trouble } = state;
  return (
    <aside
      className="panels"
      aria-label={workspace === undefined ? "Attention" : `Attention · ${workspace}`}
      data-testid="panels"
    >
      <NeedsYou queue={queue} quiet={quiet} nameOf={nameOf} show={showChat} />

      <Alerts />

      {workspace === undefined ? (
        <p className="empty">No workspace focused.</p>
      ) : (
        <>
          {trouble && (
            <p className="trouble" role="alert">
              {trouble}
            </p>
          )}

          <section data-testid="panel-todos">
            <h2>Todos</h2>
            {panels?.todos_refused && (
              <p className="trouble" role="alert">
                {panels.todos_refused}
              </p>
            )}
            {panels === undefined ? (
              <p className="pending">Reading the plane…</p>
            ) : panels.todos.length === 0 ? (
              <p className="none">Nothing to do</p>
            ) : (
              <ul className="todos">
                {panels.todos.map((todo) => (
                  <li key={todo.slug}>
                    <span className="todo-title">{todo.title}</span>
                    {todo.stamp && <span className="stamp"> · {todo.stamp}</span>}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section data-testid="panel-personas">
            <h2>Personas</h2>
            {panels === undefined ? (
              <p className="pending">Reading the plane…</p>
            ) : panels.personas.length === 0 ? (
              <p className="none">No personas on this plane</p>
            ) : (
              <ul className="personas">
                {panels.personas.map((persona) => (
                  <li key={persona}>
                    {persona}
                    {persona === panels.persona && <span className="default"> · default</span>}
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      )}
    </aside>
  );
}

/**
 * The alerts area, which has a region and nothing to draw.
 *
 * ADR 0038 assigns alerts to this side and this build cannot source one: charter's alert row
 * is `charter/statusline.py:_alerts` and it is not ported. `crates/charter-core/src/footer.rs`
 * names the same omission in its own output rather than hiding it — *"a footer that silently
 * omitted the alert row would be worse than a sentence, because an operator reads a footer to
 * find out whether anything needs them, and one that can only ever say 'nothing' is a footer
 * that lies once a week."*
 *
 * **An alerts area that draws nothing tells that same lie**, and more convincingly, because
 * an empty area under a heading reads as "charter looked and there is nothing". So the
 * heading is here and under it is the sentence, until the port exists. Nothing is invented:
 * there is no command for alerts, and this component asks for none.
 */
function Alerts() {
  return (
    <section data-testid="panel-alerts">
      <h2>Alerts</h2>
      <p className="none">
        Not drawn by this build. charter&apos;s alert row is not ported, so charter cannot tell you
        whether anything is alerting — an empty list here would be a claim it has no way to make.
      </p>
    </section>
  );
}
