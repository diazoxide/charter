import { useRef } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { FolderOpen, LoaderCircle, OctagonAlert, TriangleAlert, X } from "lucide-react";
import type { PlaneAlerts, PlaneId } from "./bindings";
import type { AlertsReading } from "./alerts";

/**
 * **The alerts drawer**: an overlay over the whole window, opened from the status line's
 * Alerts button, listing what charter says is wrong in **every** project this window holds.
 *
 * The operator's correction, and it is a modelling one: *"alerts can be cross
 * project/workspace/sessions, so what if make it some drawer outer of our window … show button
 * in new bottom status bar, and when clicking — show some independent drawer?"* The right-hand
 * region is a project's, and a pinned version or a plane root being worked in is not about the
 * project on screen — it is about whichever plane it is about, and the one that matters is
 * usually the one the operator is NOT looking at. So the drawer belongs to the window, and it
 * is cross-project by construction: the command behind it (`alerts_everywhere`) takes no plane.
 *
 * **Radix `Dialog`, drawn as a sheet from the right** (`docs/ui-primitives.md`). Not a copied
 * shadcn `Sheet`: that component is this same primitive with a class list written in shadcn's
 * token names, every one of which is a class this app does not have and would emit no CSS at
 * all (`docs/design-system.md`). The look is `App.css`'s, in charter's tokens.
 *
 * Two behaviours that differ from the window's four modal questions, on purpose:
 *
 * - **A click outside closes it.** Those dialogs refuse it because a stray click would answer
 *   a question; a drawer asks nothing, so it closes the way every drawer on every platform does.
 * - **It says what it could not read**, per project and in charter's words. A project where
 *   charter stopped looking lists what it found first and says there may be more — which is
 *   also why the button's count is dropped rather than drawn in that case.
 */
export function AlertsDrawer({
  open,
  onOpenChange,
  reading,
  planes,
  nameOf,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  reading: AlertsReading;
  /** The projects this window holds, in the strip's order, which is the order they are listed
   *  in. A project the core holds that this window does not is listed after them. */
  planes: readonly PlaneId[];
  /** What a project is called on its tab. */
  nameOf: (plane: PlaneId) => string;
}) {
  // Where the keyboard was when the drawer opened — the status line's button, in practice.
  // Radix hands focus back to a `Dialog.Trigger`, and the button that opens this is not one:
  // it lives in a project's status line and the drawer in the window. Without this, closing
  // the drawer would leave the keyboard nowhere.
  const opener = useRef<HTMLElement | null>(null);
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          className="drawer"
          data-testid="alerts-drawer"
          onOpenAutoFocus={() => {
            opener.current =
              document.activeElement instanceof HTMLElement ? document.activeElement : null;
          }}
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            opener.current?.focus();
          }}
        >
          <header className="drawer-head">
            <Dialog.Title>Alerts</Dialog.Title>
            {/* `tabIndex={0}`, per `docs/ui-primitives.md` (charter-app#186): WebKit leaves a
                `<button>` out of the tab sequence unless its `tabindex` is written down, and
                this is the drawer's only control. */}
            <Dialog.Close className="drawer-close" tabIndex={0}>
              <X aria-hidden="true" /> Close
            </Dialog.Close>
          </header>
          <Dialog.Description className="drawer-about">
            Every open project, not only the one in front. Each alert carries what fixes it.
          </Dialog.Description>
          <Body reading={reading} planes={planes} nameOf={nameOf} />
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Body({
  reading,
  planes,
  nameOf,
}: {
  reading: AlertsReading;
  planes: readonly PlaneId[];
  nameOf: (plane: PlaneId) => string;
}) {
  if (reading.at === "reading") {
    return (
      <p className="pending">
        <LoaderCircle className="node-icon spinning" />
        Reading every open project…
      </p>
    );
  }
  if (reading.at === "failed") {
    return (
      <p className="trouble" role="alert">
        charter could not read the alerts: {reading.why}
      </p>
    );
  }
  if (planes.length === 0 && reading.planes.length === 0) {
    return <p className="none">No project is open.</p>;
  }
  const answered = new Map(reading.planes.map((one) => [one.plane, one]));
  const order = [
    ...planes,
    ...reading.planes.map((one) => one.plane).filter((p) => !planes.includes(p)),
  ];
  return (
    <>
      {order.map((plane) => (
        <Project key={plane} plane={plane} name={nameOf(plane)} read={answered.get(plane)} />
      ))}
    </>
  );
}

function Project({
  plane,
  name,
  read,
}: {
  plane: PlaneId;
  name: string;
  read: PlaneAlerts | undefined;
}) {
  return (
    <section className="drawer-project" aria-label={`Alerts in ${name}`}>
      <h3 title={plane}>
        <FolderOpen className="node-icon" />
        {name}
      </h3>
      {read === undefined ? (
        // Opened after the last reading was taken. The next one — which opening the project
        // asked for — will have it.
        <p className="pending">
          <LoaderCircle className="node-icon spinning" />
          Not read yet.
        </p>
      ) : (
        <>
          {read.stopped !== null && (
            <p className="trouble" role="alert">
              charter stopped looking here: {read.stopped}. The alerts below are the ones it found
              before that, and there may be more.
            </p>
          )}
          {read.alerts.length === 0 ? (
            read.stopped === null && <p className="none">Nothing needs you here.</p>
          ) : (
            <ul className="drawer-alerts">
              {read.alerts.map((alert) => (
                <li key={`${alert.subject}\n${alert.detail}`} data-severity={alert.severity}>
                  {alert.severity === "bad" ? (
                    <OctagonAlert className="node-icon" />
                  ) : (
                    <TriangleAlert className="node-icon" />
                  )}
                  <span className="alert-words">
                    <span className="alert-subject">{alert.subject}</span>{" "}
                    <span className="alert-detail">{alert.detail}</span>
                  </span>
                  <code className="alert-remedy">{alert.remedy}</code>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
