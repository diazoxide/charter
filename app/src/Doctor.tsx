import { useCallback, useEffect, useId, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Check, LoaderCircle, Stethoscope, TriangleAlert, X, type LucideIcon } from "lucide-react";
import { Notice } from "./Notice";
import { landSettingsFocus } from "./settings/entering";
import {
  FIXES_WITH_A_FORM,
  GitIdentityForm,
  type IdentityNow,
  type IdentityRefused,
} from "./GitIdentityForm";
import {
  commands,
  type DoctorFixed,
  type DoctorReport,
  type DoctorRow,
  type PlaneId,
} from "./bindings";

/**
 * **`charter doctor`, from inside the window** — the last of ADR 0038's named gaps.
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
 * About twenty of the doctor's rows are WARNs that say *not checked (…)*, for checks this build
 * does not run (planned in OB-8, #994, and FG-2, #802). They are
 * drawn in the dialog — a doctor that dropped them would read as those problems being fixed —
 * but they are never COUNTED on the line: a count that includes them never goes below twenty,
 * and the one real warning among them is furniture on its first day. The core tells the two
 * apart (`Row::deferred`), so this does not guess from the wording.
 *
 * # One row is the app's own
 *
 * `charter doctor` answers about the plane. One question is about what THIS APP does when it
 * starts a chat — whether the chats it opens can record what a turn cost — and no CLI can
 * answer it, so the core hands it over beside the table (`DoctorReport.app_rows`). It is
 * counted with the rest, because it is a check this build runs and a warning an operator can
 * act on; the dialog draws it under its own heading so nobody goes looking for it in
 * `charter doctor`'s output.
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
 *
 * # Fix
 *
 * A row charter can fix itself carries a fix id (`DoctorRow.fix`, FX-1), and the dialog draws a
 * Fix button for it. The button calls the same core entry point `charter doctor --fix <id>`
 * does, says what the fix changed or why it was refused, and then checks again, so the row is
 * redrawn from what is true after the fix rather than assumed fixed.
 *
 * A fix that takes input (`git-identity`, FX-3) opens its form under the row instead, and is
 * applied when the form is sent: see `GitIdentityForm.tsx`.
 *
 * A finding whose fix id is in `FINDINGS_AS_NOTICES` also stands as a Notice under the strip
 * (`DoctorNotices`, #1250), and the Notice's Fix does what the row's does: the same form, the
 * same check again, so the Notice goes when the doctor no longer finds it.
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
  /** Apply the fix a row carries, by its id, then check again (the full doctor: the dialog
   *  is open). Absent where nothing can apply one. */
  fix?: (id: string) => void;
  /** Apply `git-identity` with the form's name and email, then check again. Resolves with
   *  each field's refusal, or `undefined` once the fix ran or was refused as a whole. */
  fixIdentity?: (name: string, email: string) => Promise<IdentityRefused | undefined>;
  /** git's global identity as it stands, for the form to lock what is set. */
  identityNow?: () => Promise<IdentityNow>;
  /** The fix on its way, by its id. */
  fixing?: string;
  /** What the last fix came to: its lines, or why it was refused. */
  fixed?: DoctorFixed;
  /** Forget what the last fix said, and drop the answer of one still on its way: the dialog
   *  closed, and what it said belongs to the dialog that showed it. */
  forget?: () => void;
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

  // **A fix is about the plane it was asked of.** Kept with that plane, so a window that
  // moves to another project draws none of it, and numbered as `ask` numbers its asks, so an
  // answer that lands after the project changed (or the dialog closed) is dropped, not drawn.
  const [lastFix, setLastFix] = useState<{
    plane: PlaneId;
    fixing?: string;
    fixed?: DoctorFixed;
  }>();
  const newestFix = useRef(0);
  useEffect(
    () => () => {
      newestFix.current += 1;
    },
    [plane],
  );
  const fix = useCallback(
    (id: string) => {
      const mine = ++newestFix.current;
      setLastFix({ plane, fixing: id });
      const landed = (fixed: DoctorFixed) => {
        if (mine !== newestFix.current) return;
        setLastFix({ plane, fixed });
        // Checked again whatever came of it: a fix that half-ran changed something too.
        run(true);
      };
      void commands
        .planeDoctorFix(plane, id)
        .then((answer) =>
          landed(
            answer.status === "ok"
              ? answer.data
              : { fix: id, refused: answer.error, said: [], complete: false },
          ),
        )
        .catch((err: unknown) =>
          landed({ fix: id, refused: String(err), said: [], complete: false }),
        );
    },
    [plane, run],
  );
  const fixIdentity = useCallback(
    async (name: string, email: string): Promise<IdentityRefused | undefined> => {
      const mine = ++newestFix.current;
      setLastFix({ plane, fixing: "git-identity" });
      const sent = await sendIdentity(plane, name, email);
      if ("refused" in sent) {
        // Refused field by field: nothing was written, and nothing is said over the rows.
        if (mine === newestFix.current) setLastFix(undefined);
        return sent.refused;
      }
      if (mine === newestFix.current) {
        setLastFix({ plane, fixed: sent.fixed });
        run(true);
      }
      return undefined;
    },
    [plane, run],
  );
  const identityNow = useCallback(() => identityNowOf(plane), [plane]);
  const forget = useCallback(() => {
    newestFix.current += 1;
    setLastFix(undefined);
  }, []);
  const ours = lastFix?.plane === plane ? lastFix : undefined;

  return {
    report,
    running,
    trouble,
    run,
    fix,
    fixIdentity,
    identityNow,
    fixing: ours?.fixing,
    fixed: ours?.fixed,
    forget,
  };
}

/**
 * **The git identity fix, sent with the form's name and email** (FX-3): what the core wrote, or
 * each field's refusal when it wrote nothing. A failure to ask at all is a fix refused as a
 * whole. Shared by the doctor's own form and the Alerts drawer's (#1301).
 */
export async function sendIdentity(
  plane: PlaneId,
  name: string,
  email: string,
): Promise<{ fixed: DoctorFixed } | { refused: IdentityRefused }> {
  const whole = (why: string) => ({
    fixed: { fix: "git-identity", refused: why, said: [], complete: false },
  });
  try {
    const answer = await commands.planeDoctorFixIdentity(plane, name, email);
    if (answer.status !== "ok") return whole(answer.error);
    if (answer.data.kind === "fixed") return { fixed: answer.data.fixed };
    return { refused: { name: answer.data.name, email: answer.data.email } };
  } catch (err: unknown) {
    return whole(String(err));
  }
}

/** git's global identity as it stands, for the form to lock what is set. */
export async function identityNowOf(plane: PlaneId): Promise<IdentityNow> {
  // Nothing read is nothing locked: the core reads it again before it writes anyway.
  const none = { name: "", email: "" };
  try {
    const answer = await commands.planeDoctorIdentity(plane);
    return answer.status === "ok" && answer.data ? answer.data : none;
  } catch {
    return none;
  }
}

/** Every row the verdict counts: the table's, and the app's own beside it. */
export function everyRow(report: DoctorReport): DoctorRow[] {
  return [...report.rows, ...(report.app_rows ?? [])];
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
  const { blockers, warnings, unchecked } = sorted(everyRow(report));
  const said = counted(blockers.length, "blocker") ?? counted(warnings.length, "warning");
  const tone = blockers.length > 0 ? "fail" : warnings.length > 0 ? "warn" : "quiet";
  const label =
    `Doctor: ${said ?? "nothing wrong among the checks this build runs"}` +
    (unchecked.length > 0 ? `; ${unchecked.length} not checked by this build` : "");
  return { said, tone, label };
}

/** The mark each verdict is drawn with: the icon set's for what `purlis doctor`'s own table
 *  writes as ✓, ! and ✗, so the two read alike (DS-4, #627). Decorative: the heading a row is
 *  under says its verdict in words. */
const MARK: Record<DoctorRow["status"], LucideIcon> = { ok: Check, warn: TriangleAlert, fail: X };

/** What a row's Fix button needs: the doctor's `fix`, and whether one may be pressed now. */
export type Fixer = {
  apply: (id: string) => void;
  busy: boolean;
  /** The git identity form's submit; absent where no form can be sent. */
  identity?: (name: string, email: string) => Promise<IdentityRefused | undefined>;
  /** What the identity is now, read when its form opens. */
  identityNow?: () => Promise<IdentityNow>;
};

/**
 * **A fix's form, wherever its fix is pressed** (FX-3, #1250): the Doctor row's Fix and a
 * Notice carrying the same fix id open it alike, and so does an Alerts-drawer row whose way out
 * is that fix (#1301). For a fix in {@link FIXES_WITH_A_FORM},
 * `press` opens the form (once what is set has been read, which is what it locks) or closes
 * it; for any other, it applies the fix. `form` is the form while it is open, and it closes
 * itself once the fix was applied or refused as a whole.
 */
export function useFixForm(fix: string, fixer: Fixer) {
  // The form is open once what is set has been read: `current` is what it locks.
  const [current, setCurrent] = useState<IdentityNow>();
  const id = useId();
  const withForm = FIXES_WITH_A_FORM.has(fix);
  const open = current !== undefined;
  const press = () => {
    if (!withForm) fixer.apply(fix);
    else if (open) setCurrent(undefined);
    else void (fixer.identityNow?.() ?? Promise.resolve({ name: "", email: "" })).then(setCurrent);
  };
  const form = withForm && current !== undefined && fixer.identity && (
    <GitIdentityForm
      id={id}
      current={current}
      busy={fixer.busy}
      onCancel={() => setCurrent(undefined)}
      submit={async (name, email) => {
        const refused = await fixer.identity?.(name, email);
        if (refused === undefined) setCurrent(undefined);
        return refused;
      }}
    />
  );
  return {
    /** Whether this fix can be pressed here: one with a form needs somewhere to send it. */
    offered: !withForm || fixer.identity !== undefined,
    press,
    /** The button's state, for a fix with a form. */
    opens: withForm ? { id, open } : undefined,
    form: form || undefined,
  };
}

/** A row's Fix button, and the form under it for a fix that takes input (FX-3). */
function FixButton({ row, fixer }: { row: DoctorRow & { fix: string }; fixer: Fixer }) {
  const { offered, press, opens, form } = useFixForm(row.fix, fixer);
  if (!offered) return null;
  return (
    <>
      <button
        type="button"
        className="doctor-fix"
        tabIndex={0}
        // Named by the row, as the operator reads it; the id is the CLI's word for the
        // same fix, kept where a pointer can read it.
        aria-label={`Fix ${row.name}`}
        aria-expanded={opens?.open}
        aria-controls={opens?.open ? opens.id : undefined}
        title={`purlis doctor --fix ${row.fix}`}
        disabled={fixer.busy}
        onClick={press}
      >
        Fix
      </button>
      {form}
    </>
  );
}

/**
 * **The fix ids whose doctor finding stands as a Notice under the strip** (#1250), and not
 * only as a row of the Doctor dialog.
 *
 * **The rule** (D-1301-1): a finding stands here only where, left alone, it silently loses work
 * or blocks every chat, and its fix is one press. Everything else is the dialog's, because a
 * band of rows on every open would be noise.
 *
 * - `git-identity` (`git identity`): without it every commit purlis makes (memory, notes,
 *   tallies) silently never happens. Its fix is a form the Notice opens where it stands.
 * - `memory-optimize` (`memory indexes`): a memory its `MEMORY.md` does not list is never read
 *   by a chat, so what was learnt is silently lost. The fix only appends the missing links.
 *
 * And why each of the others stays in the dialog:
 * - `discover`, `rename-plane`, `rename-local`, `persona-agents` and `handoff-rule` run only by
 *   their name (`by_name_only` in core doctor/fix.rs): the network, this machine's folders, or
 *   committed files every teammate pulls. One press under the strip is not how those are asked.
 * - `plugin-install` is about the chats started outside the app; the app arms its own either
 *   way, and the doctor the window runs at open reports the row as fine.
 * - `reinit` creates a missing baseline folder, which blocks nothing in the meantime.
 * - `local-ignore` is found only by the doctor the dialog runs: the one at open does not ask
 *   git, so its report would let the Notice's dismissal go while the finding stands.
 * - `workspace-reinit` is offered by no doctor row; the Alerts drawer carries it.
 *
 * Each by the name of the doctor row that finds it: a clean row carries no fix id, so the name
 * is how a report says the finding has gone (and a dismissal of it is let go).
 */
export const FINDINGS_AS_NOTICES: ReadonlyMap<string, string> = new Map([
  ["git-identity", "git identity"],
  ["memory-optimize", "memory indexes"],
]);

/** The family of cause a doctor finding's Notice is (`doctor-finding:<fix id>`). */
export const DOCTOR_FINDING = "doctor-finding";

/** One doctor finding as a Notice, with its fix: a form for a fix that takes input. */
function FindingNotice({
  row,
  doctor,
  onDismiss,
}: {
  row: DoctorRow & { fix: string };
  doctor: DoctorState & { fix: (id: string) => void };
  onDismiss: () => void;
}) {
  const fixer: Fixer = {
    apply: doctor.fix,
    busy: doctor.running || doctor.fixing !== undefined,
    identity: doctor.fixIdentity,
    identityNow: doctor.identityNow,
  };
  const { offered, press, opens, form } = useFixForm(row.fix, fixer);
  // What the last fix of this finding came to, when it did not mend it: the doctor checks
  // again either way, and a finding still standing says why.
  const fixed = doctor.fixed?.fix === row.fix ? doctor.fixed : undefined;
  const why =
    fixed !== undefined && fixed.refused !== null
      ? `The fix was refused: ${fixed.refused}`
      : fixed && !fixed.complete
        ? fixed.said.join(" ")
        : undefined;
  const cause = `${DOCTOR_FINDING}:${row.fix}`;
  return (
    <Notice
      cause={cause}
      tone={row.status === "fail" ? "trouble" : "news"}
      fixes={offered ? [{ label: "Fix", onPress: press, opens }] : undefined}
      onDismiss={onDismiss}
      under={form}
    >
      {`${row.name}: ${row.detail}`}
      {why !== undefined && <p>{why}</p>}
    </Notice>
  );
}

/**
 * **The doctor's findings that stand as Notices** (#1250, {@link FINDINGS_AS_NOTICES}): drawn in
 * the project's band from the doctor the window already runs, and gone once the doctor checks
 * again without them.
 *
 * **Dismiss lasts until the finding changes** (V91j, `dismissals.ts`): every report the doctor
 * answers settles the `doctor-finding` family, so a finding that was mended lets its dismissal
 * go and shows again if it comes back. A report that did not check the finding's row (git could
 * not be asked) says nothing about it, and keeps its dismissal.
 */
export function DoctorNotices({
  doctor,
  dismissed,
  dismiss,
  settle,
}: {
  doctor: DoctorState;
  /** The causes dismissed in this project, and the ways to change them (`useDismissals`). */
  dismissed: ReadonlySet<string>;
  dismiss: (cause: string) => void;
  settle: (family: string, present: readonly string[]) => void;
}) {
  const { report, fix } = doctor;
  const findings = (report?.rows ?? []).filter(
    (row): row is DoctorRow & { fix: string } =>
      row.status !== "ok" && row.checked && row.fix !== null && FINDINGS_AS_NOTICES.has(row.fix),
  );

  useEffect(() => {
    if (report === undefined) return;
    const present = [...FINDINGS_AS_NOTICES].flatMap(([id, name]) => {
      const cause = `${DOCTOR_FINDING}:${id}`;
      const row = report.rows.find((one) => one.name === name);
      // Not checked, or not in this report: unknown, so it is not let go.
      if (row === undefined || !row.checked) return [cause];
      return row.status !== "ok" && row.fix === id ? [cause] : [];
    });
    settle(DOCTOR_FINDING, present);
  }, [report, settle]);

  if (fix === undefined) return null;
  return (
    <>
      {findings
        .filter((row) => !dismissed.has(`${DOCTOR_FINDING}:${row.fix}`))
        .map((row) => (
          <FindingNotice
            key={row.fix}
            row={row}
            doctor={{ ...doctor, fix }}
            onDismiss={() => dismiss(`${DOCTOR_FINDING}:${row.fix}`)}
          />
        ))}
    </>
  );
}

/** A verdict's mark, from {@link MARK}. */
function Mark({ status }: { status: DoctorRow["status"] }) {
  const Icon = MARK[status];
  return <Icon />;
}

/**
 * The rows of one heading. A row whose fix is a setting names that setting's group
 * (`DoctorRow.settings`, SE-22), and is drawn with the way into it: `onOpenSettings`. A row
 * charter can fix itself (`DoctorRow.fix`, FX-1) is drawn with a Fix button: `fixer`.
 */
function Rows({
  rows,
  fixer,
  onOpenSettings,
}: {
  rows: readonly DoctorRow[];
  fixer?: Fixer;
  onOpenSettings?: (group: string) => void;
}) {
  return (
    <ul className="doctor-rows">
      {rows.map((row) => (
        <li key={row.name} className={`doctor-row doctor-${row.status}`} data-row={row.name}>
          <span className="doctor-glyph" aria-hidden="true">
            <Mark status={row.status} />
          </span>
          <span className="doctor-name">{row.name}</span>
          <span className="doctor-detail">{row.detail}</span>
          {/* A green row's hint is carried and never drawn, as the table does. A deferred
              row's hint is the same sentence on every one of them, so it is said once, over
              the list, rather than twenty-four times inside it. */}
          {row.status !== "ok" && row.checked && row.hint && (
            <span className="doctor-hint">{row.hint}</span>
          )}
          {row.settings !== null && onOpenSettings && (
            <button
              type="button"
              className="doctor-settings"
              tabIndex={0}
              onClick={() => onOpenSettings(row.settings as string)}
            >
              Fix it in Settings
            </button>
          )}
          {row.status !== "ok" && row.fix && fixer && (
            <FixButton row={{ ...row, fix: row.fix }} fixer={fixer} />
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
export function Health({
  doctor,
  onOpenSettings,
}: {
  doctor: DoctorState;
  /** Opens Settings at a group (`settings/links.ts`, SE-22): what a row naming a setting's
   *  "Fix it in Settings" does, once the dialog has closed. */
  onOpenSettings?: (group: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const settings =
    onOpenSettings &&
    ((group: string) => {
      setOpen(false);
      onOpenSettings(group);
    });
  // Where the keyboard goes when a fix answers: the button it was pressed on is gone once the
  // row is clean, and focus left on nothing falls back to the page.
  const fixedAt = useRef<HTMLElement>(null);
  useEffect(() => {
    if (doctor.fixed !== undefined) fixedAt.current?.focus();
  }, [doctor.fixed]);
  const { said, tone, label } = onTheLine(doctor);
  const { report, running, trouble, run, fixing, fixed } = doctor;
  const groups = report ? sorted(report.rows) : undefined;
  const fixer: Fixer | undefined = doctor.fix && {
    apply: doctor.fix,
    busy: running || fixing !== undefined,
    identity: doctor.fixIdentity,
    identityNow: doctor.identityNow,
  };
  const ours = report?.app_rows ?? [];

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(now) => {
        setOpen(now);
        // Opening it is the operator asking, which is the one thing that may probe a harness.
        if (now) run(true);
        else doctor.forget?.();
      }}
    >
      <Dialog.Trigger asChild>
        <button
          type="button"
          className={`status-doctor doctor-tone-${tone}`}
          // WebKit leaves a `<button>` out of the tab sequence unless its `tabindex` is written
          // down (`docs/ui-primitives.md`, charter-app#189).
          tabIndex={0}
          data-testid="status-doctor"
          aria-label={label}
          title={label}
        >
          {running ? (
            // It is running: this is the state the spin is for, *still happening*.
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
        <Dialog.Content
          className="warning doctor"
          aria-describedby="doctor-depth"
          onCloseAutoFocus={(event) => {
            // A row's "Fix it in Settings" sent the keyboard into Settings: Radix would hand it
            // back to the doctor's button (#1206).
            if (landSettingsFocus()) event.preventDefault();
          }}
        >
          <Dialog.Title>Doctor</Dialog.Title>
          <p className="honest" id="doctor-depth">
            {report === undefined
              ? "purlis has not answered yet."
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
          {fixed !== undefined && (
            <section
              aria-label="Fix"
              className={`doctor-fixed${fixed.complete ? "" : " doctor-trouble"}`}
              role="status"
              ref={fixedAt}
              tabIndex={-1}
            >
              {fixed.refused !== null ? (
                <p className="honest">
                  Fix {fixed.fix} was refused: {fixed.refused}
                </p>
              ) : (
                <ul className="doctor-fixed-lines">
                  {fixed.said.map((line, i) => (
                    <li key={i}>{line}</li>
                  ))}
                </ul>
              )}
            </section>
          )}
          {ours.length > 0 && (
            <section aria-label="This app">
              <h3>This app</h3>
              {/* Said rather than left to be noticed: an operator comparing this with
                  `purlis doctor` in a terminal has to know why one row is not there. */}
              <p className="honest">
                What the chats this window starts can do. `purlis doctor` does not print these.
              </p>
              <Rows rows={ours} onOpenSettings={settings} />
            </section>
          )}
          {groups && (
            <>
              {groups.blockers.length > 0 && (
                <section aria-label="Blockers">
                  <h3>Blockers</h3>
                  <Rows rows={groups.blockers} fixer={fixer} onOpenSettings={settings} />
                </section>
              )}
              {groups.warnings.length > 0 && (
                <section aria-label="Warnings">
                  <h3>Warnings</h3>
                  <Rows rows={groups.warnings} fixer={fixer} onOpenSettings={settings} />
                </section>
              )}
              {groups.passed.length > 0 && (
                <section aria-label="Passed">
                  <h3>Passed</h3>
                  <Rows rows={groups.passed} fixer={fixer} onOpenSettings={settings} />
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
          {/* `tabIndex={0}` on both, per `docs/ui-primitives.md` (charter-app#186). This
              dialog has three tabbables when a row is unchecked — the `<summary>` above,
              `Check again`, and `Close` — and `Check again` sat between the two edges Radix's
              focus scope handles, on an engine that leaves a `<button>` out of the tab
              sequence unless its `tabindex` says otherwise. It could not be reached at all. */}
          <div className="answer">
            <button type="button" tabIndex={0} disabled={running} onClick={() => run(true)}>
              Check again
            </button>
            <Dialog.Close asChild>
              <button type="button" tabIndex={0}>
                Close
              </button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
