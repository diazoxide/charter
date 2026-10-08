import { Fragment } from "react";
import { ChatShownState } from "./ChatRows";
import type { Crumbs } from "./tabChats";

/**
 * **A pane's breadcrumb, while its tab shows a task** (#1486, V100-34): `steward 4 › talk ·
 * working`, in the pane's existing top line beside the gauge and the harness's name. It is why
 * a person never types into the wrong chat: the session the tab is, the task on screen, and
 * what that task is doing.
 *
 * **Each name before the last goes to that chat**, the session's own chat first: pressing it
 * switches the tab, as pressing the task's row did. The last is where the person is, and is a
 * name and no button. A task of a task shows its whole path (V100-41).
 *
 * **A task that works in another workspace says so** (V100-40), `in <workspace>`, beside its
 * name: its tab is on the strip of the session that asked.
 *
 * **The state is the one its row says** (#1484), from the one function, and it never gives
 * way: at a narrow pane the names in the middle of the path shrink first, then the two ends,
 * and the state word stays whole (`App.css`, `.pane-crumbs`).
 *
 * Drawn only while a pane shows a task. A session with no tasks has the top line it had.
 */
export function PaneCrumbs({
  crumbs,
  onShow,
}: {
  crumbs: Crumbs;
  /** Switches the tab to that chat. */
  onShow: (session: number) => void;
}) {
  const last = crumbs.path.length - 1;
  const shown = crumbs.path[last];
  const said = crumbs.path.map((chat) => chat.name).join(" › ");
  return (
    <nav
      className="pane-crumbs"
      aria-label={`Where this pane is: ${said}`}
      // The whole path, for a pane too narrow to draw it.
      title={crumbs.elsewhere === null ? said : `${said} in ${crumbs.elsewhere}`}
    >
      <span className="crumb-path">
        {crumbs.path.map((chat, at) => (
          <Fragment key={chat.session}>
            {at === last ? (
              <span className="crumb shown" aria-current="page">
                {chat.name}
              </span>
            ) : (
              <button
                type="button"
                className={at === 0 ? "crumb own" : "crumb between"}
                // In the tab sequence, said out loud (`docs/ui-primitives.md`).
                tabIndex={0}
                aria-label={chat.name}
                title={`Show ${chat.name}`}
                onClick={() => onShow(chat.session)}
              >
                {chat.name}
              </button>
            )}
            {at < last && (
              <span className="crumb-sep" aria-hidden="true">
                {" › "}
              </span>
            )}
          </Fragment>
        ))}
      </span>
      {crumbs.elsewhere !== null && <span className="crumb-where"> in {crumbs.elsewhere}</span>}
      <span className="crumb-sep" aria-hidden="true">
        {" · "}
      </span>
      <ChatShownState
        session={shown.session}
        shell={shown.shell}
        report={shown.report}
        outcome={shown.outcome}
        asking={shown.asking}
        harness={shown.harness}
      />
    </nav>
  );
}
