import { useId, useState } from "react";
import { CircleAlert, CircleCheck, CircleDot, LoaderCircle, Save } from "lucide-react";
import { commands, type PlaneId, type PlaneSaving, type SaveEntry } from "./bindings";
import { askWayOut, tellSaved, usePlaneSaving } from "./saving";

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
          {saving.live.length > 0 && (
            <p className="settings-hint">
              {`Live workspaces, published by every save: ${saving.live.join(", ")}`}
            </p>
          )}
          {saving.stage === "blocked" && (
            <div className="saving-question" role="group" aria-label="Ways out">
              {saving.conflicts.length > 0 && (
                <ul className="saving-files" aria-label="Where it conflicts">
                  {saving.conflicts.map((path) => (
                    <li key={path}>{path}</li>
                  ))}
                </ul>
              )}
              <div className="settings-actions">
                <button
                  type="button"
                  className="panel-view"
                  tabIndex={0}
                  onClick={() => askWayOut(plane, "chat")}
                >
                  Resolve in a chat
                </button>
                <button
                  type="button"
                  className="panel-view"
                  tabIndex={0}
                  onClick={() => askWayOut(plane, "terminal")}
                >
                  Open terminal here
                </button>
              </div>
            </div>
          )}
          {saving.pushFailed !== null && (
            <p className="settings-hint">{`The last push did not land: ${saving.pushFailed}`}</p>
          )}
          {saving.mode === null && <ModeQuestion plane={plane} branch={saving.branch} />}
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
 * **The question a project with no mode is asked once** (ADR 0051): how far its saves go. The
 * answer is written as `[plane] mode` in `charter.toml` by the core's own writer, and until
 * there is one, nothing saves the project by itself. The pull request modes are set in Project
 * settings; this asks the three a person can answer without knowing the repository's rules.
 */
function ModeQuestion({ plane, branch }: { plane: PlaneId; branch: string }) {
  const [refused, setRefused] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const choose = async (mode: string) => {
    setBusy(true);
    setRefused(null);
    try {
      const got = await commands.choosePlaneMode(plane, mode);
      if (got.status === "error") setRefused(got.error);
    } catch (err: unknown) {
      setRefused(String(err));
    } finally {
      setBusy(false);
      tellSaved();
    }
  };
  const choices: { mode: string; label: string; says: string }[] = [
    {
      mode: "push",
      label: `Push to ${branch || "the remote"}`,
      says: "Every save is committed and pushed, so the team has it.",
    },
    {
      mode: "commit",
      label: "Commit only",
      says: "Saves stay on this machine until you push them yourself.",
    },
    { mode: "off", label: "Off", says: "charter commits nothing here; you use git yourself." },
  ];
  return (
    <div className="saving-question" role="group" aria-label="How should this project be saved?">
      <p className="saving-stage">How should this project be saved?</p>
      {choices.map((one) => (
        <button
          key={one.mode}
          type="button"
          className="panel-view"
          tabIndex={0}
          disabled={busy}
          onClick={() => void choose(one.mode)}
        >
          {`${one.label} — ${one.says}`}
        </button>
      ))}
      {refused !== null && (
        <p className="trouble" role="alert">
          {refused}
        </p>
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

/** The stage, as the view and the title bar say it, and what came in and was not pulled. */
export function stageText(saving: PlaneSaving): string {
  const said = stageWords(saving);
  return saving.behind !== null && saving.behind > 0 ? `${said} · ${saving.behind} incoming` : said;
}

function stageWords(saving: PlaneSaving): string {
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

/**
 * **A project's unsaved mark** (charter-app#302), on its tab in the project strip: a dot when
 * it has work a save would take — files, a blocked save, commits a push would carry — and
 * nothing when a save has nothing left to do. Named for a screen reader; the Saving tab says
 * what exactly.
 */
export function UnsavedMark({ saving, name }: { saving: PlaneSaving | undefined; name: string }) {
  if (saving === undefined) return null;
  const blocked = saving.stage === "blocked";
  if (!blocked && !savable(saving)) return null;
  return (
    <span
      className="project-unsaved"
      data-stage={saving.stage}
      role="img"
      aria-label={blocked ? `saving is blocked in ${name}` : `unsaved work in ${name}`}
      title={stageText(saving)}
    />
  );
}
