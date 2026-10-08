import { Fragment, type ComponentProps, type ReactNode } from "react";
import { ChatShownState } from "./ChatRows";
import { ASKED_BY_YOU } from "./chatsTree";
import type { Crumbs } from "./tabChats";

/**
 * **A pane's breadcrumb, while its tab shows a task** (#1486, V100-34): `steward 4 › talk ·
 * working`, in the pane's existing top line beside the gauge and the harness's name. It is why
 * a person never types into the wrong chat: the session the tab is, the task on screen, and
 * what that task is doing. **A pane never shows a task without it.**
 *
 * **Each name before the last goes to that chat**, the session's own chat first: pressing it
 * switches the tab, as pressing the task's row did. The last is where the person is, and is a
 * name and no button. A task of a task shows its whole path (V100-41). A chat on the path that
 * has ended is a name and no button: there is nowhere to go.
 *
 * **A task that works in another workspace says so** (V100-40), `in <workspace>`, beside its
 * name: its tab is on the strip of the session that asked.
 *
 * **A task the person asked for themselves says so after its state** (#1492, V100-70), `working
 * · asked by you`, as its row does: the chat whose tab this is did not dispatch it.
 *
 * **The state is the one its row says** (#1484), from the one function. In a narrow pane the
 * names give way before it does (`App.css`, `.pane-crumbs`, which says how far that holds).
 *
 * **To a screen reader it is a list called "Chat path"**: each name is read once, as the button
 * or the text it is, and the one on screen is `aria-current`. The whole path is the tooltip.
 *
 * Drawn only while a pane shows a task. A session with no tasks has the top line it had.
 *
 * **Also for a task in a pane of its own** (#1489): its own tab, where it is why that tab is
 * not mistaken for a session's, and beside its session, where each side of the split says which
 * chat it is (the session's side by its name alone). A right-click on a task's breadcrumb is
 * its row's menu in the Chats list: whatever else is handed in goes on the list's element, so
 * a menu can make it its trigger.
 */
export function PaneCrumbs({
  crumbs,
  onShow,
  state,
  gone,
  ...rest
}: {
  crumbs: Crumbs;
  /** Switches the tab to that chat. */
  onShow: (session: number) => void;
  /** The state said after the path, where it is not the live one of the chat shown: a task
   *  that has ended says how it ended. */
  state?: ReactNode;
  /** The chats of the path that are not open any more: each is a name and no way. */
  gone?: (session: number) => boolean;
} & Omit<ComponentProps<"nav">, "children" | "className" | "title" | "aria-label">) {
  const last = crumbs.path.length - 1;
  const shown = crumbs.path[last];
  const path = crumbs.path.map((chat) => chat.name).join(" › ");
  const said = shown.byYou === true ? `${path}, ${ASKED_BY_YOU}` : path;
  return (
    <nav
      {...rest}
      className="pane-crumbs"
      aria-label="Chat path"
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
            ) : gone?.(chat.session) ? (
              <span className={at === 0 ? "crumb own" : "crumb between"}>{chat.name}</span>
            ) : (
              <button
                type="button"
                className={at === 0 ? "crumb own" : "crumb between"}
                // In the tab sequence, said out loud (`docs/ui-primitives.md`).
                tabIndex={0}
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
      {state ?? (
        <ChatShownState
          session={shown.session}
          shell={shown.shell}
          report={shown.report}
          outcome={shown.outcome}
          asking={shown.asking}
          harness={shown.harness}
        />
      )}
      {shown.byYou === true && (
        /* One box with its separator, so both give way together, before the state does. */
        <span className="crumb-by" title="You asked for this task from this tab">
          <span aria-hidden="true">{" · "}</span>
          {ASKED_BY_YOU}
        </span>
      )}
    </nav>
  );
}
