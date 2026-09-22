import { useCallback, useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { LoaderCircle, Stethoscope } from "lucide-react";
import { commands, type DoctorReport, type DoctorRow, type PlaneId } from "./bindings";

/**
 * **`charter doctor`, from inside the window** — the last of charter ADR 0038's named gaps.
 *
 * The case for it is one incident, and it decides the shape. A charter launched from Finder
 * could not find `claude`, because macOS hands a GUI app a four-directory `PATH`
 * (charter-app#134). The doctor was ported and would have said so — but it answers for the
 * process that runs it, and the only way to run it was a terminal, whose shell has the
 * operator's whole `PATH` and so cannot reproduce the thing being diagnosed. So the doctor
 * runs **in the app's process** (`app/src-tauri/src/doctor.rs`) and is drawn here.
 *
 * # Where it lives: the status line, as one button
 *
 * The status line is the frame, not a region (`StatusLine.tsx` argues it): always drawn, never
 * put away. That is the property a health indicator needs — a doctor in a region can be hidden
 * by the operator and then report nothing, which is the "absent answer read as health" charter
 * ADR 0013 forbids. And it is one button, not a row of counts, because what the line can carry
 * is a verdict; the rows themselves are a list, and a list is a dialog.
 *
 * # The verdict counts only what this build checked
 *
 * About twenty of the doctor's rows are WARNs that say *not checked (…not ported…)*. They are
 * drawn in the dialog — a doctor that dropped them would read as those problems being fixed —
 * but they are never COUNTED on the line: a count that includes them never goes below twenty,
 * and the one real warning among them is furniture on its first day. The core tells the two
 * apart (`Row::deferred`), so this does not guess from the wording.
 *
 * And the footer's rule holds: **zero draws nothing.** A clean doctor is the word `Doctor` with
 * no number — never a green tick, because about twenty checks did not run and a tick over them
 * would be the claim ADR 0013 exists to refuse.
 *
 * # Two depths
 *
 * The window runs the **preflight** when a project opens — what every session start already
 * runs, so asking it unprompted costs nothing new. Opening the dialog runs the **full**
 * doctor, which also probes each harness profile: that RUNS the harness, and the core says
 * only a doctor a person asked for may (`doctor/profiles.rs`). Opening the dialog is the
 * asking. The dialog says which of the two it is showing.
 */

/** What the window knows about the doctor for one project. */
export type DoctorState = {
  /** The last report that came back. Kept while a newer one is being asked for, so the
   *  dialog does not blank while it checks again. */
  report?: DoctorReport;
  /** Whether an ask is on its way. */
  running: boolean;
  /** Why the last ask did not come back with a report, in the core's words. */
  trouble?: string;
  /** Ask again — `full` probes the harness profiles. */
  run: (full: boolean) => void;
};

/**
 * The doctor for one project: the preflight once when it opens, and whatever is asked after.
 *
 * **The newest ask wins**, and that is a rule rather than a race: the preflight asked at open
 * and the full doctor asked a moment later by the operator opening the dialog can come back in
 * either order, and the full one must not be overwritten by the preflight that started first
 * and finished second.
 */
export function useDoctor(plane: PlaneId): DoctorState {
  const [report, setReport] = useState<DoctorReport>();
  // `true` from the first render, because the preflight is asked the moment the project
  // opens — and a flag the effect set would be a render the window does not need.
  const [running, setRunning] = useState(true);
  const [trouble, setTrouble] = useState<string>();
  const newest = useRef(0);

  /** Asks, and lands the answer only if nothing newer was asked since. */
  const ask = useCallback(
    (full: boolean) => {
      const mine = ++newest.current;
      void commands
        .planeDoctor(plane, full)
        .then((answer) => {
          if (mine !== newest.current) return;
          if (answer.status === "ok") {
            // Something that is not a report — nothing at all, or a test's catch-all `[]` — is
            // not drawn: a verdict made out of it would be a verdict made of nothing, and a
            // missing `rows` would take the whole window down with it.
            if (Array.isArray(answer.data?.rows)) {
              setReport(answer.data);
              setTrouble(undefined);
            }
          } else {
            setTrouble(answer.error);
          }
        })
        .catch((err: unknown) => {
          if (mine === newest.current) setTrouble(String(err));
        })
        .finally(() => {
          if (mine === newest.current) setRunning(false);
        });
    },
    [plane],
  );

  const run = useCallback(
    (full: boolean) => {
      setRunning(true);
      ask(full);
    },
    [ask],
  );

  useEffect(() => {
    ask(false);
  }, [ask]);

  return { report, running, trouble, run };
}

/** The rows sorted into what the line and the dialog say about them. */
export function sorted(rows: readonly DoctorRow[]) {
  return {
    blockers: rows.filter((row) => row.status === "fail"),
    // A deferred row is a WARN too, and is not counted with the real ones: see the module doc.
    warnings: rows.filter((row) => row.status === "warn" && row.checked),
    passed: rows.filter((row) => row.status === "ok"),
    unchecked: rows.filter((row) => !row.checked),
  };
}

/** `2 blockers`, `1 warning` — or nothing, for the footer's reason. */
function counted(n: number, one: string): string | undefined {
  if (n === 0) return undefined;
  return n === 1 ? `1 ${one}` : `${n} ${one}s`;
}

/** What the button on the status line says, and what it says to a screen reader. */
export function onTheLine(doctor: DoctorState): { said?: string; tone: string; label: string } {
  const { report, running, trouble } = doctor;
  if (report === undefined) {
    if (trouble !== undefined)
      return {
        tone: "unknown",
        label: `Doctor — could not run: ${trouble}`,
      };
    return { tone: "unknown", label: running ? "Doctor — checking" : "Doctor" };
  }
  const { blockers, warnings, unchecked } = sorted(report.rows);
  const said = counted(blockers.length, "blocker") ?? counted(warnings.length, "warning");
  const tone = blockers.length > 0 ? "fail" : warnings.length > 0 ? "warn" : "quiet";
  const label =
    `Doctor: ${said ?? "nothing wrong among the checks this build runs"}` +
    (unchecked.length > 0 ? `; ${unchecked.length} not checked by this build` : "");
  return { said, tone, label };
}

/** The glyph `charter doctor`'s own table draws for each verdict, so the two read alike. */
const GLYPH: Record<DoctorRow["status"], string> = { ok: "✓", warn: "!", fail: "✗" };

function Rows({ rows }: { rows: readonly DoctorRow[] }) {
  return (
    <ul className="doctor-rows">
      {rows.map((row) => (
        <li key={row.name} className={`doctor-row doctor-${row.status}`} data-row={row.name}>
          <span className="doctor-glyph" aria-hidden="true">
            {GLYPH[row.status]}
          </span>
          <span className="doctor-name">{row.name}</span>
          <span className="doctor-detail">{row.detail}</span>
          {/* A green row's hint is carried and never drawn, as the table does. A deferred
              row's hint is the same sentence on every one of them, so it is said once, over
              the list, rather than twenty-four times inside it. */}
          {row.status !== "ok" && row.checked && row.hint && (
            <span className="doctor-hint">{row.hint}</span>
          )}
        </li>
      ))}
    </ul>
  );
}

/**
 * The status line's doctor button, and the dialog it opens.
 *
 * A Radix dialog (`docs/ui-primitives.md`), which is what makes the list keyboard-reachable
 * and the window behind it inert while it is up.
 */
export function Health({ doctor }: { doctor: DoctorState }) {
  const [open, setOpen] = useState(false);
  const { said, tone, label } = onTheLine(doctor);
  const { report, running, trouble, run } = doctor;
  const groups = report ? sorted(report.rows) : undefined;

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(now) => {
        setOpen(now);
        // Opening it is the operator asking, which is the one thing that may probe a harness.
        if (now) run(true);
      }}
    >
      <Dialog.Trigger asChild>
        <button
          type="button"
          className={`status-doctor doctor-tone-${tone}`}
          data-testid="status-doctor"
          aria-label={label}
          title={label}
        >
          {running ? (
            // It is running: this is the one state the window's only animation is for.
            <LoaderCircle aria-hidden="true" className="spinning" />
          ) : (
            <Stethoscope aria-hidden="true" />
          )}{" "}
          Doctor
          {said !== undefined && <span className="doctor-said"> · {said}</span>}
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content className="warning doctor" aria-describedby="doctor-depth">
          <Dialog.Title>Doctor</Dialog.Title>
          <p className="honest" id="doctor-depth">
            {report === undefined
              ? "charter has not answered yet."
              : report.full
                ? "Every check, with each harness profile probed — run inside this app, so every answer is the app's own environment."
                : "The preflight every session start runs; the harness profiles are not probed. Run inside this app, so every answer is the app's own environment."}
            {running && " Checking again…"}
          </p>
          {trouble !== undefined && (
            <p className="honest doctor-trouble" role="alert">
              The doctor could not run: {trouble}
            </p>
          )}
          {groups && (
            <>
              {groups.blockers.length > 0 && (
                <section aria-label="Blockers">
                  <h3>Blockers</h3>
                  <Rows rows={groups.blockers} />
                </section>
              )}
              {groups.warnings.length > 0 && (
                <section aria-label="Warnings">
                  <h3>Warnings</h3>
                  <Rows rows={groups.warnings} />
                </section>
              )}
              {groups.passed.length > 0 && (
                <section aria-label="Passed">
                  <h3>Passed</h3>
                  <Rows rows={groups.passed} />
                </section>
              )}
              {groups.unchecked.length > 0 && (
                <details className="doctor-unchecked">
                  <summary>Not checked by this build ({groups.unchecked.length})</summary>
                  {/* The one hint every deferred row carries, once. */}
                  <p className="doctor-hint">{groups.unchecked[0].hint}</p>
                  <Rows rows={groups.unchecked} />
                </details>
              )}
            </>
          )}
          {report && (
            <p className="doctor-path">
              <span className="status-label">This app&apos;s PATH</span>{" "}
              {report.path === null ? (
                <span className="none">none — the app was started with no PATH at all</span>
              ) : (
                <code>{report.path}</code>
              )}
            </p>
          )}
          <div className="answer">
            <button type="button" disabled={running} onClick={() => run(true)}>
              Check again
            </button>
            <Dialog.Close asChild>
              <button type="button">Close</button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
