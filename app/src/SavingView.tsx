import { useId, useState } from "react";
import { CircleAlert, CircleCheck, CircleDot, LoaderCircle, Save } from "lucide-react";
import { commands, type PlaneId, type PlaneSaving, type SaveEntry } from "./bindings";
import { tellSaved, usePlaneSaving } from "./saving";

/**
 * **The Saving view** (charter-app#294, ADR 0051): where this plane's unsaved work sits, what
 * the next save takes, how far a save goes, a save button, and the last saves.
 *
 * Everything on it is the core's answer: the stage is `planegit::standing`, read from git and the
 * push record; the save is `planegit::save_as` — the function `charter save` runs — so the button
 * and the command cannot disagree about what a save does. After a save the view reads the plane
 * again rather than guessing what changed.
 */
export function SavingView({ plane, onSaved }: { plane: PlaneId; onSaved?: () => void }) {
  // The title bar's reader: fresh on focus, on plane changes, on a timer and after any save.
  const { saving } = usePlaneSaving(plane);
  /** Why the last save was refused — kept across the reads that follow it. */
  const [refused, setRefused] = useState<string | null>(null);
  const [said, setSaid] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const messageId = useId();

  const save = async () => {
    setBusy(true);
    setSaid(null);
    setRefused(null);
    const typed = message.trim();
    try {
      const got = await commands.savePlane(plane, typed === "" ? null : typed);
      if (got.status === "ok") {
        setSaid(got.data);
        setMessage("");
        onSaved?.();
      } else {
        setRefused(got.error);
      }
    } catch (err: unknown) {
      setRefused(String(err));
    } finally {
      setBusy(false);
      tellSaved();
    }
  };

  return (
    <div className="saving" data-testid="saving-view">
      {saving === undefined ? (
        <p className="pending" aria-busy="true">
          <LoaderCircle className="node-icon spinning" aria-hidden="true" />
          Reading the plane
        </p>
      ) : (
        <>
          <p className={`saving-stage saving-${saving.stage}`}>{stageText(saving)}</p>
          <p className="settings-who">{modeText(saving)}</p>
          {saving.pr !== null && (
            <p className="settings-hint">
              {"Pull request: "}
              <a href={saving.pr} target="_blank" rel="noreferrer">
                {saving.pr}
              </a>
            </p>
          )}
          {saving.changed.length > 0 && (
            <ul className="saving-files" aria-label="What the next save takes">
              {saving.changed.map((path) => (
                <li key={path}>{path}</li>
              ))}
            </ul>
          )}
          <div className="settings-field">
            <label htmlFor={messageId}>Message</label>
            <input
              id={messageId}
              type="text"
              value={message}
              placeholder="Leave empty for one that says what changed"
              onChange={(e) => setMessage(e.target.value)}
              disabled={busy}
            />
          </div>
          <div className="settings-actions">
            <button
              type="button"
              className="panel-view"
              tabIndex={0}
              onClick={() => void save()}
              disabled={busy || !savable(saving)}
            >
              {busy ? (
                <LoaderCircle className="node-icon spinning" aria-hidden="true" />
              ) : (
                <Save className="node-icon" aria-hidden="true" />
              )}
              Save
            </button>
          </div>
          {refused !== null && (
            <p className="trouble" role="alert">
              {refused}
            </p>
          )}
          {said !== null && (
            <pre className="saving-said" role="status">
              {said.join("\n")}
            </pre>
          )}
          <h3 className="saving-recent">Recent saves</h3>
          {saving.journal.length === 0 ? (
            <p className="none">No saves recorded yet.</p>
          ) : (
            <ul className="saving-journal" aria-label="Recent saves">
              {saving.journal.map((one, n) => (
                <li key={`${one.at}-${n}`}>{entryText(one)}</li>
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

/**
 * **The title bar's save indicator** (charter-app#294): the stage of the project in front, as
 * a button that opens its Saving tab, and — while there is anything to take — a save button
 * beside it. Both are `<button tabIndex={0}>`: the tag is what the drag handler stops at, and
 * the tab index is what WebKit's sequence needs (`TitleBar.tsx`).
 */
export function SaveIndicator({
  saving,
  busy,
  onOpen,
  onSave,
}: {
  saving: PlaneSaving;
  busy: boolean;
  onOpen: () => void;
  onSave: () => void;
}) {
  const said = stageText(saving);
  const Mark =
    saving.stage === "blocked" ? CircleAlert : saving.stage === "saved" ? CircleCheck : CircleDot;
  return (
    <span className="save-indicator" data-stage={saving.stage}>
      <button
        type="button"
        className="save-indicator-where"
        tabIndex={0}
        aria-label={`Saving: ${said}`}
        title={said}
        onClick={onOpen}
      >
        <Mark aria-hidden="true" />
        <span className="save-indicator-words">{said}</span>
      </button>
      {savable(saving) && (
        <button
          type="button"
          className="save-indicator-save"
          tabIndex={0}
          aria-label="Save the project"
          title="Save the project"
          disabled={busy}
          onClick={onSave}
        >
          {busy ? (
            <LoaderCircle className="spinning" aria-hidden="true" />
          ) : (
            <Save aria-hidden="true" />
          )}
        </button>
      )}
    </span>
  );
}

/** Whether pressing Save could do anything: files to commit, a blocked save to try again, or
 *  commits a push would carry. Commits on a plane whose save stops at the commit are as far as
 *  a save goes, and a button that could only say "nothing to save" is not offered. */
function savable(saving: PlaneSaving): boolean {
  return (
    saving.changed.length > 0 ||
    saving.stage === "blocked" ||
    (saving.stage === "committed" && saving.pushes)
  );
}

/** The stage, as the view and the title bar say it. */
export function stageText(saving: PlaneSaving): string {
  switch (saving.stage) {
    case "blocked":
      return `Blocked: ${saving.blocked ?? "the last save could not finish"}`;
    case "changed":
      return `${saving.changed.length} changed`;
    case "committed":
      return saving.ahead === null
        ? "Committed, not pushed"
        : `${saving.ahead} committed, not pushed`;
    case "pr-open":
      return "Pushed — waiting on its pull request";
    default:
      return "Saved";
  }
}

function modeText(saving: PlaneSaving): string {
  const mode = saving.mode === null ? "Mode: not set" : `Mode: ${saving.mode} (${saving.modeFrom})`;
  const reach = saving.pushes
    ? `a save pushes to ${saving.branch}`
    : "a save commits and goes no further";
  return `${mode} — ${reach}`;
}

function entryText(one: SaveEntry): string {
  const when = new Date((one.at ?? 0) * 1000).toLocaleString();
  const commit = one.commit === null ? "" : ` · ${one.commit.slice(0, 7)}`;
  const detail = one.detail === "" ? "" : ` — ${one.detail}`;
  const files = one.files === 1 ? "1 file" : `${one.files} files`;
  return `${when} · ${one.trigger} · ${one.outcome} · ${files}${commit}${detail}`;
}
