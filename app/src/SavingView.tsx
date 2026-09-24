import { useEffect, useId, useState } from "react";
import { CircleAlert, CircleCheck, CircleDot, LoaderCircle, Save } from "lucide-react";
import { commands, type PlaneId, type PlaneSaving, type SaveEntry } from "./bindings";
import { PLANE_SAVED, tellSaved } from "./saving";

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
  const [saving, setSaving] = useState<PlaneSaving | null>(null);
  /** Why the plane could not be read. */
  const [trouble, setTrouble] = useState<string | null>(null);
  /** Why the last save was refused — kept across the read that follows it. */
  const [refused, setRefused] = useState<string | null>(null);
  const [said, setSaid] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const messageId = useId();

  /** Goes up after a save, so the plane is read again. */
  const [reads, setReads] = useState(0);

  useEffect(() => {
    // Keyed by the plane and the count: an answer for a read the view has moved past is dropped.
    let gone = false;
    void commands
      .planeSaving(plane)
      .then((got) => {
        if (gone) return;
        if (got.status === "ok") {
          setSaving(got.data);
          setTrouble(null);
        } else {
          setTrouble(got.error);
        }
      })
      .catch((err: unknown) => {
        if (!gone) setTrouble(String(err));
      });
    return () => {
      gone = true;
    };
  }, [plane, reads]);

  // A save from the title bar is one this view should show too.
  useEffect(() => {
    const again = () => setReads((n) => n + 1);
    window.addEventListener(PLANE_SAVED, again);
    return () => window.removeEventListener(PLANE_SAVED, again);
  }, []);

  const save = async () => {
    setBusy(true);
    setSaid(null);
    setRefused(null);
    const typed = message.trim();
    const got = await commands.savePlane(plane, typed === "" ? null : typed);
    if (got.status === "ok") {
      setSaid(got.data);
      setMessage("");
      onSaved?.();
      tellSaved();
    } else {
      setRefused(got.error);
    }
    setReads((n) => n + 1);
    setBusy(false);
  };

  return (
    <div className="saving" data-testid="saving-view">
      {saving === null ? (
        trouble === null ? (
          <p className="pending" aria-busy="true">
            <LoaderCircle className="node-icon spinning" aria-hidden="true" />
            Reading the plane
          </p>
        ) : (
          <p className="trouble" role="alert">
            {trouble}
          </p>
        )
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
          {(refused ?? trouble) !== null && (
            <p className="trouble" role="alert">
              {refused ?? trouble}
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

/** Whether pressing Save could do anything: there is work, or commits, the remote lacks. */
function savable(saving: PlaneSaving): boolean {
  return saving.stage !== "saved" && saving.stage !== "pr-open";
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
  if (saving.mode === null) return "Mode: not set — a save commits and pushes";
  return `Mode: ${saving.mode} (${saving.modeFrom})`;
}

function entryText(one: SaveEntry): string {
  const when = new Date((one.at ?? 0) * 1000).toLocaleString();
  const commit = one.commit === null ? "" : ` · ${one.commit.slice(0, 7)}`;
  const detail = one.detail === "" ? "" : ` — ${one.detail}`;
  const files = one.files === 1 ? "1 file" : `${one.files} files`;
  return `${when} · ${one.trigger} · ${one.outcome} · ${files}${commit}${detail}`;
}
