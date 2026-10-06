import { useRef, useState, type ReactNode } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { FolderOpen, LoaderCircle, Monitor, OctagonAlert, TriangleAlert, X } from "lucide-react";
import { commands, type AlertRow, type PlaneAlerts, type PlaneId } from "./bindings";
import type { AlertsReading } from "./alerts";
import { drawWhatIsInForce } from "./Extensions";
import { Notice, type NoticeAction } from "./Notice";
import { sayAboutThisMachine, usingTheBuiltIn, type MachineAlert } from "./windowprefs";

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
 *
 * **Each row carries its way out** (NO-6, #1238; V91a): a row is a {@link Notice}, and its
 * button is what the core says fixes it (`AlertRow.way`) — a Settings group, a fix of the
 * doctor's registry, the outer project, the Saving view — never a command to type elsewhere.
 * A row about this machine links to its Settings group, offers Use built-in for the theme file,
 * or, with nothing to press, can be dismissed for this launch.
 */
export function AlertsDrawer({
  open,
  onOpenChange,
  reading,
  aboutThisMachine = [],
  planes,
  nameOf,
  does,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  reading: AlertsReading;
  /** What the window says about this machine rather than a project: a layout or theme file in
   *  charter's config directory that it could not use as written (`windowprefs.ts`). Listed
   *  above the projects, and only when there is something to say. */
  aboutThisMachine?: MachineAlert[];
  /** The projects this window holds, in the strip's order, which is the order they are listed
   *  in. A project the core holds that this window does not is listed after them. */
  planes: readonly PlaneId[];
  /** What a project is called on its tab. */
  nameOf: (plane: PlaneId) => string;
  /** What the rows' ways out do in the window. */
  does: AlertsDrawerDoes;
}) {
  const close = () => onOpenChange(false);
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
          {aboutThisMachine.length > 0 && (
            <section className="drawer-project" aria-label="Alerts about this machine">
              <h3>
                <Monitor className="node-icon" />
                This machine
              </h3>
              <ul className="drawer-alerts">
                {aboutThisMachine.map((alert) => (
                  <MachineRow
                    key={alert.subject}
                    alert={alert}
                    onOpenSettings={(group) => {
                      close();
                      does.openSettings(group);
                    }}
                  />
                ))}
              </ul>
            </section>
          )}
          <Body reading={reading} planes={planes} nameOf={nameOf} does={does} close={close} />
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * **What the rows' ways out do in the window** (NO-6). Each that leaves the drawer is called
 * once it has closed.
 */
export type AlertsDrawerDoes = {
  /** Opens Settings at a group (SE-22): a You group, or — with `plane` — that project's. */
  openSettings: (group: string, plane?: PlaneId) => void;
  /** Opens another project by its path, through the window's one way in (the trust gate). */
  openProject: (path: string) => void;
  /** Opens a project's Saving view. */
  openSaving: (plane: PlaneId) => void;
  /** Reads every project's alerts again: after a fix, so a row it cured goes. */
  reread: () => void;
};

function Body({
  reading,
  planes,
  nameOf,
  does,
  close,
}: {
  reading: AlertsReading;
  planes: readonly PlaneId[];
  nameOf: (plane: PlaneId) => string;
  does: AlertsDrawerDoes;
  close: () => void;
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
        purlis could not read the alerts: {reading.why}
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
        <Project
          key={plane}
          plane={plane}
          name={nameOf(plane)}
          read={answered.get(plane)}
          does={does}
          close={close}
        />
      ))}
    </>
  );
}

function Project({
  plane,
  name,
  read,
  does,
  close,
}: {
  plane: PlaneId;
  name: string;
  read: PlaneAlerts | undefined;
  does: AlertsDrawerDoes;
  close: () => void;
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
              purlis stopped looking here: {read.stopped}. The alerts below are the ones it found
              before that, and there may be more.
            </p>
          )}
          {read.alerts.length === 0 ? (
            read.stopped === null && <p className="none">Nothing needs you here.</p>
          ) : (
            <ul className="drawer-alerts">
              {read.alerts.map((alert) => (
                <ProjectRow
                  key={`${alert.subject}\n${alert.detail}`}
                  plane={plane}
                  alert={alert}
                  does={does}
                  close={close}
                />
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}

/** The words on a fix's button, by its id in the doctor's registry (FX-1). */
const FIX_LABELS: Readonly<Record<string, string>> = { "workspace-reinit": "Reinit" };

/**
 * What one row says, inside its Notice: its mark, what it is about and what is wrong, then what
 * the last press answered. Each row draws its own `<Notice>` with its ways out named, never
 * spread, so `Notice.guard.test.ts` reads every one of them.
 */
function Words({
  severity,
  subject,
  detail,
  then,
  said,
}: {
  severity: string;
  subject: string;
  detail: ReactNode;
  then?: string;
  said?: string;
}) {
  return (
    <>
      {severity === "bad" ? (
        <OctagonAlert className="node-icon" />
      ) : (
        <TriangleAlert className="node-icon" />
      )}
      <span className="alert-words">
        <span className="alert-subject">{subject}</span>{" "}
        <span className="alert-detail">{detail}</span>
        {then !== undefined && <span className="alert-detail"> — {then}</span>}
        {said !== undefined && <span className="alert-said">{said}</span>}
      </span>
    </>
  );
}

/** A row's Notice look: trouble for what loses work if it is left. */
const toneOf = (severity: string) => (severity === "bad" ? "trouble" : "news");

/** A project's row, with the way out the core gave it. */
function ProjectRow({
  plane,
  alert,
  does,
  close,
}: {
  plane: PlaneId;
  alert: AlertRow;
  does: AlertsDrawerDoes;
  close: () => void;
}) {
  /** What the last fix answered, when it did not cure the row: its refusal, or what it said. */
  const [said, setSaid] = useState<string>();
  const fixing = useRef(false);
  const leaving =
    (go: () => void): NoticeAction["onPress"] =>
    () => {
      close();
      go();
    };
  const way = alert.way;
  let action: NoticeAction;
  switch (way.kind) {
    case "settings":
      action = {
        label: "Fix it in Settings",
        onPress: leaving(() => does.openSettings(way.group, plane)),
      };
      break;
    case "open-project":
      action = {
        label: "Open the outer project",
        onPress: leaving(() => does.openProject(way.path)),
      };
      break;
    case "saving":
      action = { label: "Go to Saving", onPress: leaving(() => does.openSaving(plane)) };
      break;
    case "fix": {
      const label = FIX_LABELS[way.id] ?? "Fix";
      action = {
        label,
        onPress: () => {
          if (fixing.current) return;
          fixing.current = true;
          setSaid(undefined);
          const could = (why: string) => `purlis could not ${label.toLowerCase()}: ${why}`;
          void commands
            .planeDoctorFix(plane, way.id)
            .then((answer) => {
              if (answer.status === "error") setSaid(could(answer.error));
              else if (answer.data.refused !== null) setSaid(could(answer.data.refused));
              else if (!answer.data.complete) setSaid(answer.data.said.join(" "));
            })
            .catch((err: unknown) => setSaid(could(String(err))))
            .finally(() => {
              fixing.current = false;
              // Read again whatever came of it: a fix that half-ran changed something too.
              does.reread();
            });
        },
      };
      break;
    }
  }
  const cause = `alert:${alert.subject}`;
  const tone = toneOf(alert.severity);
  const words = (
    <Words severity={alert.severity} subject={alert.subject} detail={alert.detail} said={said} />
  );
  return (
    <li data-severity={alert.severity}>
      {way.kind === "fix" ? (
        <Notice cause={cause} at="drawer" tone={tone} fixes={[action]}>
          {words}
        </Notice>
      ) : (
        <Notice cause={cause} at="drawer" tone={tone} link={action}>
          {words}
        </Notice>
      )}
    </li>
  );
}

/**
 * A row about this machine: its Settings group, Use built-in for the theme file (which asks
 * first, in the row), or — with neither — Dismiss, which takes it back for this launch.
 */
function MachineRow({
  alert,
  onOpenSettings,
}: {
  alert: MachineAlert;
  onOpenSettings: (group: string) => void;
}) {
  const [asking, setAsking] = useState(false);
  const [said, setSaid] = useState<string>();
  const useBuiltIn = () => {
    setAsking(false);
    void commands
      .useBuiltInTheme()
      .then((answer) => {
        if (answer.status === "error") {
          setSaid(answer.error);
          return;
        }
        usingTheBuiltIn();
        void drawWhatIsInForce();
      })
      .catch((err: unknown) => setSaid(String(err)));
  };
  const cause = `alert:${alert.subject}`;
  const tone = toneOf(alert.severity);
  const shown = { severity: alert.severity, subject: alert.subject, said };
  const words = <Words {...shown} detail={alert.detail} then={alert.remedy} />;
  const settings = alert.settings;
  let notice: ReactNode;
  if (asking)
    notice = (
      <Notice
        cause={cause}
        at="drawer"
        tone={tone}
        fixes={[
          { label: "Use built-in", onPress: useBuiltIn },
          { label: "Keep it", onPress: () => setAsking(false) },
        ]}
      >
        <Words
          {...shown}
          detail="Use the built-in theme? purlis moves the theme file aside to theme.aside.json (or the next free theme.aside-N.json), never over a file, and draws what is in force without it."
        />
      </Notice>
    );
  else if (settings !== undefined)
    notice = (
      <Notice
        cause={cause}
        at="drawer"
        tone={tone}
        link={{ label: "Fix it in Settings", onPress: () => onOpenSettings(settings) }}
      >
        {words}
      </Notice>
    );
  else if (alert.builtIn)
    notice = (
      <Notice
        cause={cause}
        at="drawer"
        tone={tone}
        fixes={[
          {
            label: "Use built-in…",
            onPress: () => {
              setSaid(undefined);
              setAsking(true);
            },
          },
        ]}
      >
        {words}
      </Notice>
    );
  else
    notice = (
      <Notice
        cause={cause}
        at="drawer"
        tone={tone}
        onDismiss={() => sayAboutThisMachine(alert.subject, undefined)}
      >
        {words}
      </Notice>
    );
  return <li data-severity={alert.severity}>{notice}</li>;
}
