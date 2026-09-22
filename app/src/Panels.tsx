import { useEffect, useState } from "react";
import * as Popover from "@radix-ui/react-popover";
import { CircleDashed, ListTodo, LoaderCircle, Star, UserRound } from "lucide-react";
import { NeedsYou } from "./NeedsYou";
import { commands, type PersonaDetails, type PlaneId } from "./bindings";
import type { WorkspaceState } from "./workspaceState";

/**
 * The right region: **what is asking for you** (charter ADR 0038).
 *
 * **Alerts are not here any more, and that is a correction to ADR 0038, not an omission.** It
 * put them on this side, and this side is one project's: it follows the project in front and
 * the workspace focused in it. An alert is about a PLANE — a pin, a front door, a workspace's
 * layout, a plane root being worked in — and the plane that has one is usually not the one on
 * screen. So alerts are the window's: the status line's Alerts button, always on screen and
 * counting every open project, opens a drawer over the whole window (`AlertsDrawer.tsx`). A
 * section here pointing at that button would spend this region's height, in every project, on
 * a sentence about a control that is already visible one line below.
 *
 * The needs-you queue, the workspace's todos and the plane's personas. It is the
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
  plane,
  workspace,
  state,
  queue,
  quiet,
  nameOf,
  showChat,
}: {
  /** Which project's plane the personas belong to. A window holds several, and two of them
   *  can both have a `steward`. */
  plane: PlaneId;
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
            <h2>
              <ListTodo className="node-icon" />
              Todos
            </h2>
            {panels?.todos_refused && (
              <p className="trouble" role="alert">
                {panels.todos_refused}
              </p>
            )}
            {panels === undefined ? (
              <p className="pending">
                <LoaderCircle className="node-icon spinning" />
                Reading the plane…
              </p>
            ) : panels.todos.length === 0 ? (
              <p className="none">Nothing to do</p>
            ) : (
              <ul className="todos">
                {panels.todos.map((todo) => (
                  <li key={todo.slug}>
                    <CircleDashed className="node-icon" />
                    <span className="todo-title">{todo.title}</span>
                    {todo.stamp && <span className="stamp"> · {todo.stamp}</span>}
                  </li>
                ))}
              </ul>
            )}
          </section>

          {/* **The personas, which were a plain list of words** — the operator's own last
              example of what was wrong with this window. Each is a person charter can run a
              chat as, so each carries the mark for one, and the plane's default carries a star
              beside the word it already said.

              The words are untouched. `· default` stays text rather than becoming a chip,
              because a chip is a picture of a word and this one is read out: the region's
              scenario spec asks the panel whether it says `default`, and a screen reader gets
              the same sentence a sighted reader does. The star is decoration on top.

              **And each row opens now** — the operator again: *"personas list in right sidebar
              is just texts, without click action, we can on clicking show some info about
              persona."* `PersonaRow` below has what it opens and why it is a popover. */}
          <section data-testid="panel-personas">
            <h2>
              <UserRound className="node-icon" />
              Personas
            </h2>
            {panels === undefined ? (
              <p className="pending">
                <LoaderCircle className="node-icon spinning" />
                Reading the plane…
              </p>
            ) : panels.personas.length === 0 ? (
              <p className="none">No personas on this plane</p>
            ) : (
              <ul className="personas">
                {panels.personas.map((persona) => (
                  <PersonaRow
                    key={persona}
                    plane={plane}
                    persona={persona}
                    isDefault={persona === panels.persona}
                  />
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
 * One persona, and what its definition says when the row is opened.
 *
 * ## Why a popover, and not a dialog or a sheet
 *
 * A persona's details are six short rows that answer *what is this one for*. Nothing here is
 * a question, nothing can be pressed inside it, and nothing is lost by dismissing it — so the
 * surface is chosen on what a reader does next, which is usually read the next one.
 *
 * - **A popover is anchored to the row it is about.** With five personas listed, the thing
 *   that makes a card legible is that it points at the name it belongs to. It is also the
 *   primitive `docs/ui-primitives.md` names for exactly this shape.
 * - **A dialog is modal, and modal is wrong here twice.** Radix marks everything outside an
 *   open dialog `aria-hidden` — including the needs-you queue two sections up, which charter
 *   ADR 0038 says this region must never compete with — and a modal is for a question that
 *   has to be answered before anything else happens. This is reading.
 * - **A sheet is already taken, and it is the window's.** `AlertsDrawer` is a sheet from the
 *   right over the whole window; a second sheet, over one project's region, would be two
 *   drawers with two different rules and two different scopes.
 *
 * It takes the menu's two decisions rather than the four dialogs' (charter ADR 0039): **not
 * modal**, so the rest of the window stays reachable to a screen reader and to a scenario
 * spec, and **a click outside closes it**, because there is no answer to lose. Escape closes
 * it too, and Radix puts the keyboard back on the row.
 *
 * `side="left"` is where it opens from in the default arrangement, and no more than that: a
 * region MOVES (ADR 0038), so the explorer's slot may hold this panel tomorrow. Radix flips to
 * the other side when there is no room, which is what makes naming a side safe.
 *
 * ## It is asked for when the row is opened, and asked again every time
 *
 * A definition is a file on disk that an operator edits — often while charter is running,
 * because `charter persona create` is how one arrives. Caching the first answer would show a
 * role that was corrected an hour ago. The cost is one small file read, plus one per step up
 * an `extends:` chain, per click.
 */
function PersonaRow({
  plane,
  persona,
  isDefault,
}: {
  plane: PlaneId;
  persona: string;
  /** Whether the plane names this one as the persona a chat started here adopts. */
  isDefault: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [shown, setShown] = useState<PersonaDetails>();
  const [refused, setRefused] = useState<string>();

  useEffect(() => {
    if (!open) return;
    let gone = false;
    void commands
      .personaDetails(plane, persona)
      .then((said) => {
        if (gone) return;
        if (said.status === "error") setRefused(said.error);
        else if (said.data) setShown(said.data);
      })
      .catch((err: unknown) => {
        if (!gone) setRefused(String(err));
      });
    return () => {
      gone = true;
    };
  }, [open, persona, plane]);

  return (
    <li className={isDefault ? "is-default" : ""}>
      <Popover.Root
        open={open}
        onOpenChange={(opening) => {
          // Nothing is held from the last time it was open: a definition is a file, and the
          // row the reader just opened is the one they are asking about.
          if (opening) {
            setShown(undefined);
            setRefused(undefined);
          }
          setOpen(opening);
        }}
      >
        <Popover.Trigger asChild>
          <button type="button" className="persona">
            <UserRound className="node-icon" />
            {persona}
            {isDefault && (
              <span className="default">
                {" · default"}
                <Star className="node-icon" />
              </span>
            )}
          </button>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            className="persona-card"
            data-testid={`persona-details-${persona}`}
            side="left"
            align="start"
            sideOffset={6}
            collisionPadding={8}
            aria-label={`${persona} — what this persona is`}
          >
            <PersonaCard persona={persona} isDefault={isDefault} shown={shown} refused={refused} />
            <Popover.Arrow className="persona-card-arrow" />
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    </li>
  );
}

/** What the card says, which is what `charter persona show` says about the same persona. */
function PersonaCard({
  persona,
  isDefault,
  shown,
  refused,
}: {
  persona: string;
  isDefault: boolean;
  shown: PersonaDetails | undefined;
  refused: string | undefined;
}) {
  if (refused !== undefined) {
    // charter's own sentence, which names the fix. Drawn rather than swallowed: a card that
    // came up empty reads as a persona with nothing in it.
    return (
      <p className="trouble" role="alert">
        {refused}
      </p>
    );
  }
  if (shown === undefined) {
    return (
      <p className="pending">
        <LoaderCircle className="node-icon spinning" />
        Reading the definition…
      </p>
    );
  }
  return (
    <>
      <h3>
        {persona}
        {shown.role !== null && shown.role !== "" && <span className="role"> — {shown.role}</span>}
      </h3>
      {isDefault && (
        <p className="is-default-note">
          <Star className="node-icon" />
          The plane&apos;s default: a chat started here adopts it unless one is picked.
        </p>
      )}
      <dl>
        <dt>Delegate to it for</dt>
        <dd>
          {shown.delegate_when !== null && shown.delegate_when !== "" ? (
            shown.delegate_when
          ) : (
            // `delegate-when` is what makes a persona findable — it becomes the description
            // whoever is routing reads — so a definition without one is worth saying.
            <span className="none">nothing declared, so nothing routes here by itself</span>
          )}
        </dd>

        <dt>Tools</dt>
        <dd>
          {shown.tools.length > 0 ? (
            shown.tools.join(", ")
          ) : (
            <span className="none">none auto-approved</span>
          )}
        </dd>

        {/* **The vault's NAME, and never a thing inside it.** charter refuses a secret by
            kind and never echoes one; a panel does not get an exception. The three answers
            are three because "holds no credentials" and "nobody has said" are different
            facts — see `PersonaDetails::declares_no_vault` for why charter-app cannot yet
            collapse the second into the first. */}
        <dt>Vault</dt>
        <dd>
          {shown.vault !== null ? (
            <>
              <code>{shown.vault}</code>
              <span className="note"> — the name; what is in it is never shown here</span>
            </>
          ) : shown.declares_no_vault ? (
            <span className="none">none: this persona holds no credentials of its own</span>
          ) : (
            <span className="none">not declared in its definition</span>
          )}
        </dd>

        {shown.lineage.length > 1 && (
          <>
            <dt>Inherits</dt>
            <dd>{shown.lineage.join(" → ")}</dd>
          </>
        )}

        <dt>Defined in</dt>
        <dd>
          <code>{shown.file}</code>
        </dd>
      </dl>
    </>
  );
}
