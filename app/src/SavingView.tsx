import { useId, useRef, useState } from "react";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { CircleAlert, CircleCheck, CircleDot, LoaderCircle, Save } from "lucide-react";
import {
  commands,
  type PlaneId,
  type PlaneSaving,
  type RepoSaving,
  type SaveEntry,
} from "./bindings";
import {
  askWayOut,
  behindness,
  repoSavable,
  repoSaveGoesTo,
  repoStageText,
  saveAll,
  tellSaved,
  usePlaneSaving,
  useRepoSaving,
} from "./saving";

/**
 * **The Saving view** (charter-app#294, ADR 0051): where this plane's unsaved work sits, what
 * the next save takes, how far a save goes, a save button, and the last saves.
 *
 * Everything on it is the core's answer: the stage is `planegit::standing`, read from git and the
 * push record; the save is `planegit::save_as` — the function `charter save` runs — so the button
 * and the command cannot disagree about what a save does. After a save the view reads the plane
 * again rather than guessing what changed.
 */
export function SavingView({
  plane,
  workspace,
  onSaved,
}: {
  plane: PlaneId;
  /** The workspace whose repos get a row each (charter-app#299); none outside every one. */
  workspace?: string;
  onSaved?: () => void;
}) {
  // The title bar's reader: fresh on focus, on plane changes, on a timer and after any save.
  const { saving } = usePlaneSaving(plane);
  const repos = useRepoSaving(plane, workspace);
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

  /** Save all's confirmation, while it is open: every tree it would save, as read when Save all
   *  was pressed. Held still, so what is saved is exactly what the operator read — the rereads
   *  that keep the view fresh never add a repo to a list already on screen. */
  const [confirming, setConfirming] = useState<{
    plane: PlaneSaving | null;
    repos: RepoSaving[];
  }>();

  /** Save all: the plane and every repo with something to take, one after another. Only ever
   *  run from its confirmation (ADR 0051, amended 2026-09-25). */
  const saveEverything = async (what: { plane: PlaneSaving | null; repos: RepoSaving[] }) => {
    setConfirming(undefined);
    setBusy(true);
    setSaid(null);
    setRefused(null);
    const typed = message.trim();
    try {
      const got = await saveAll(
        plane,
        what.plane !== null,
        typed === "" ? null : typed,
        workspace,
        what.repos,
      );
      if (got.said.length > 0) setSaid(got.said);
      if (got.refused.length > 0) setRefused(got.refused.join("\n"));
      else {
        setMessage("");
        onSaved?.();
      }
    } finally {
      setBusy(false);
      tellSaved();
    }
  };

  /** One repo's own Save. The message box is the plane's; a repo's commit says its files. */
  const saveOne = async (name: string) => {
    if (workspace === undefined) return;
    setBusy(true);
    setSaid(null);
    setRefused(null);
    try {
      const got = await commands.saveRepo(plane, workspace, name, null);
      if (got.status === "ok") setSaid(got.data);
      else setRefused(got.error);
    } catch (err: unknown) {
      setRefused(String(err));
    } finally {
      setBusy(false);
      tellSaved();
    }
  };

  const anyToSave = saving !== undefined && (savable(saving) || (repos ?? []).some(repoSavable));

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
          {saving.notice !== null && <p className="settings-hint">{saving.notice}</p>}
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
            {workspace !== undefined && (
              <button
                type="button"
                className="panel-view"
                tabIndex={0}
                onClick={() =>
                  setConfirming({
                    plane: savable(saving) ? saving : null,
                    repos: (repos ?? []).filter(repoSavable),
                  })
                }
                disabled={busy || !anyToSave}
              >
                <Save className="node-icon" aria-hidden="true" />
                Save all
              </button>
            )}
          </div>
          {confirming !== undefined && workspace !== undefined && (
            <ConfirmSaveAll
              plane={confirming.plane}
              workspace={workspace}
              repos={confirming.repos}
              onSave={() => void saveEverything(confirming)}
              onCancel={() => setConfirming(undefined)}
            />
          )}
          {workspace !== undefined && repos !== undefined && repos.length > 0 && (
            <RepoRows
              workspace={workspace}
              repos={repos}
              busy={busy}
              onSave={(name) => void saveOne(name)}
            />
          )}
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
 * **One row per repo in the workspace** (charter-app#299): its stage, the branch it is on, the
 * pull request it waits on, and its own Save. A table for the reason the bottom bar's is one:
 * "which of these is unsaved" is a column question.
 */
function RepoRows({
  workspace,
  repos,
  busy,
  onSave,
}: {
  workspace: string;
  repos: readonly RepoSaving[];
  busy: boolean;
  onSave: (name: string) => void;
}) {
  return (
    <table className="saving-repos" aria-label={`Repos in ${workspace}`}>
      <thead>
        <tr>
          <th scope="col">Repo</th>
          <th scope="col">Branch</th>
          <th scope="col">Stage</th>
          <th scope="col">Pull request</th>
          <th scope="col">Mode</th>
          <th scope="col">Save goes to</th>
          <th scope="col">
            <span className="sr-only">Save</span>
          </th>
        </tr>
      </thead>
      <tbody>
        {repos.map((repo) => (
          <tr key={repo.name} data-repo={repo.name} data-stage={repo.stage}>
            <td>{repo.name}</td>
            <td>{repo.branch ?? "no branch"}</td>
            <td>{repoStageText(repo)}</td>
            <td>
              {repo.pr === null ? (
                "—"
              ) : (
                <a href={repo.pr} target="_blank" rel="noreferrer">
                  {repo.pr}
                </a>
              )}
            </td>
            <td>{`${repo.mode} (${repo.modeFrom})${repo.autosave ? " · auto-save" : ""}`}</td>
            <td data-testid="save-goes-to">{repoSaveGoesTo(repo, workspace)}</td>
            <td>
              <button
                type="button"
                className="panel-view"
                tabIndex={0}
                aria-label={`Save ${repo.name}`}
                title={`Save ${repo.name}: ${repoSaveGoesTo(repo, workspace)}`}
                disabled={busy || !repoSavable(repo)}
                onClick={() => onSave(repo.name)}
              >
                <Save className="node-icon" aria-hidden="true" />
              </button>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/**
 * **Save all's confirmation** (ADR 0051, amended 2026-09-25): every tree it would save, each
 * repo with the branch it is on, what it would take, and where its save goes — because a repo
 * is a developer's, and a save commits every file changed in it and may push it. Nothing is
 * saved until the operator says so here.
 *
 * Radix's `AlertDialog`, as `DeleteWorkspace` is: an action that reaches past this machine is
 * one the operator confirms, and focus starts on Cancel.
 */
function ConfirmSaveAll({
  plane,
  workspace,
  repos,
  onSave,
  onCancel,
}: {
  /** The project's standing, when it has something to save; else `null`. */
  plane: PlaneSaving | null;
  workspace: string;
  repos: readonly RepoSaving[];
  onSave: () => void;
  onCancel: () => void;
}) {
  const cancel = useRef<HTMLButtonElement>(null);
  const count = (plane === null ? 0 : 1) + repos.length;
  const reposSaid = `${repos.length} ${repos.length === 1 ? "repo" : "repos"}`;
  const title =
    plane === null
      ? `Save ${reposSaid}?`
      : repos.length === 0
        ? "Save the project?"
        : `Save the project and ${reposSaid}?`;
  return (
    <AlertDialog.Root
      open
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="asking" />
        <AlertDialog.Content
          className="warning"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            cancel.current?.focus();
          }}
        >
          <AlertDialog.Title>{title}</AlertDialog.Title>
          <AlertDialog.Description className="came-back">
            A repo&apos;s save commits every file changed in it on the branch it is on, then goes as
            far as its mode says.
          </AlertDialog.Description>
          <ul className="saving-files" aria-label="What Save all saves">
            {plane !== null && <li>{`The project — ${planeTakes(plane)}`}</li>}
            {repos.map((repo) => (
              <li key={repo.name}>
                {`${repo.name} on ${repo.branch ?? "no branch"} — ${repoTakes(repo)} — ${repoSaveGoesTo(repo, workspace)}`}
              </li>
            ))}
          </ul>
          <div className="doing">
            <button type="button" className="ends-it" tabIndex={0} onClick={onSave}>
              {`Save all ${count}`}
            </button>
            <AlertDialog.Cancel asChild>
              <button type="button" tabIndex={0} ref={cancel}>
                Cancel
              </button>
            </AlertDialog.Cancel>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}

/** "1 file changed", "3 files changed". */
function filesSaid(n: number): string {
  return `${n} ${n === 1 ? "file" : "files"} changed`;
}

/** What the project's save would take: its changed files, else the commits a push would carry,
 *  else the blocked save it tries again. */
function planeTakes(plane: PlaneSaving): string {
  if (plane.changed.length > 0) return filesSaid(plane.changed.length);
  if (plane.stage === "blocked") return "trying its blocked save again";
  if (plane.ahead === null) return "a branch never pushed";
  return `${plane.ahead} ${plane.ahead === 1 ? "commit" : "commits"} not pushed`;
}

/** What a repo's save would take: its changed files, else the commits a push would carry. */
function repoTakes(repo: RepoSaving): string {
  if (repo.changed > 0) return filesSaid(repo.changed);
  if (repo.stage === "blocked") return "trying its blocked save again";
  if (repo.ahead === null) return "a branch never pushed";
  return `${repo.ahead} ${repo.ahead === 1 ? "commit" : "commits"} not pushed`;
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
  repos,
  busy,
  onOpen,
  onSave,
}: {
  saving: PlaneSaving;
  /** The active workspace's repos (charter-app#299): the indicator shows the furthest-back
   *  stage across them and the plane. Its save is the project's only — a repo is saved from
   *  the Saving tab, where it says where the save goes (ADR 0051, amended 2026-09-25). */
  repos?: readonly RepoSaving[];
  busy: boolean;
  onOpen: () => void;
  onSave: () => void;
}) {
  const { stage, said } = furthestBack(saving, repos ?? []);
  const Mark = stage === "blocked" ? CircleAlert : stage === "saved" ? CircleCheck : CircleDot;
  return (
    <span className="save-indicator" data-stage={stage}>
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

/**
 * The furthest-back stage across the plane and its repos (ADR 0051), and how the title bar
 * says it: the plane's own words when the plane is that far back, else how many repos are.
 * A repo charter never saves (`off`) is not counted.
 */
export function furthestBack(
  saving: PlaneSaving,
  repos: readonly RepoSaving[],
): { stage: string; said: string } {
  const counted = repos.filter((repo) => repo.stage !== "off");
  const stage = counted.reduce(
    (far, repo) => (behindness(repo.stage) < behindness(far) ? repo.stage : far),
    saving.stage,
  );
  if (behindness(saving.stage) <= behindness(stage)) return { stage, said: stageText(saving) };
  const n = counted.filter((repo) => repo.stage === stage).length;
  const words: Record<string, [string, string]> = {
    blocked: ["blocked", "blocked"],
    changed: ["changed", "changed"],
    committed: ["committed, not pushed", "committed, not pushed"],
    "pr-open": ["waiting on its pull request", "waiting on their pull requests"],
  };
  const [one, many] = words[stage] ?? [stage, stage];
  const said = n === 1 ? `1 repo ${one}` : `${n} repos ${many}`;
  const incoming =
    saving.behind !== null && saving.behind > 0 ? ` · ${saving.behind} incoming` : "";
  return { stage, said: `${said}${incoming}` };
}

/** Whether pressing Save could do anything: files to commit, a blocked save to try again, or
 *  commits a push would carry. Commits on a plane whose save stops at the commit are as far as
 *  a save goes, and a button that could only say "nothing to save" is not offered. */
export function savable(saving: PlaneSaving): boolean {
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
  const reach = !saving.pushes
    ? "a save commits and goes no further"
    : saving.mode === "pr" || saving.mode === "pr-merge"
      ? `a save pushes to this machine's save branch and keeps a pull request open into ${saving.branch}${
          saving.mode === "pr-merge" ? ", set to merge when its checks pass" : ""
        }`
      : `a save pushes to ${saving.branch}`;
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
