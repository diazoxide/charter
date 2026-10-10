import { useEffect, useState } from "react";
import { ChevronRight, CircleCheck, FolderGit2, GitBranch, GitMerge } from "lucide-react";
import { commands, type AheadBehind, type PlaneId } from "./bindings";
import type { BranchRef, StatusRead } from "./branchStatus";
import type { Catalogued, Offer } from "./actions";
import type { Piece } from "./bindings";
import type { Place } from "./pieceViews";
import type { WorkspaceState } from "./workspaceState";

/**
 * **The branch cockpit's own parts** (FM-5, #1108; #1103 V86 F2): what the explorer draws above
 * one branch once it is focused on it — a breadcrumb back out, and the branch's state with
 * Merge and Done. The chats and the files under them are the explorer's own rows
 * (`Explorer.tsx`), narrowed to the branch, so the cockpit's tree is the tree.
 */

/**
 * Whether the branch a window focused still stands in the workspace it is in — the ONE rule the
 * explorer and the window both read (FM-5). It stands while its repo's branches are still being
 * listed, so a focus a launch put back is the cockpit from the first frame; it does not when the
 * listing came back without it, when the listing was refused, or when the repo is gone from the
 * workspace.
 *
 * **A focus on a repo's own folder** (#1152) stands while the workspace holds the clone: before
 * the plane is read, and while its repos list it. Its branches' listing says nothing of it.
 *
 * @returns The branch as listed (absent while its repo is still being listed), or `undefined`
 *   when the focus does not stand.
 */
export function focusStands(
  state: WorkspaceState,
  focus: Place,
): { piece: Piece | undefined } | undefined {
  const repos = state.panels?.repos;
  if (repos !== undefined && !repos.includes(focus.repo)) return undefined;
  if (focus.piece === null) return { piece: undefined };
  if (state.piecesRefused[focus.repo] !== undefined) return undefined;
  const listed = state.pieces[focus.repo];
  if (listed === undefined) return { piece: undefined };
  const piece = listed.find((one) => one.piece === focus.piece);
  return piece === undefined ? undefined : { piece };
}

/** How far a branch is from its base, or why it could not be read. Absent while it is read. */
export type AheadBehindRead = { apart?: AheadBehind; trouble?: string };

/**
 * How far `branch` is from the branch it was cut from: read when it is focused, and again each
 * time what it changed is read again (`status`), which is when an agent's writes landed. Read by
 * the core's bounded reader in-process, starting no git (V88a, D-88f).
 */
export function useAheadBehind(
  plane: PlaneId | undefined,
  workspace: string | undefined,
  branch: BranchRef | undefined,
  status: StatusRead | undefined,
): AheadBehindRead | undefined {
  const key =
    plane === undefined || workspace === undefined || branch === undefined
      ? undefined
      : `${String(plane)}\0${workspace}\0${branch.repo}\0${branch.piece ?? ""}`;
  const [held, setHeld] = useState<{ key: string; read: AheadBehindRead }>();
  useEffect(() => {
    if (key === undefined || plane === undefined || workspace === undefined || !branch) return;
    let gone = false;
    const told = (read: AheadBehindRead) => {
      if (!gone) setHeld({ key, read });
    };
    void commands
      .branchAheadBehind(plane, workspace, branch.repo, branch.piece)
      .then((said) => told(said.status === "ok" ? { apart: said.data } : { trouble: said.error }))
      .catch((err: unknown) => told({ trouble: String(err) }));
    return () => {
      gone = true;
    };
    // `status` is a dependency for its identity alone: a new read of what the branch changed is
    // the moment to count again. `branch` is named by `key`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, status]);
  return held !== undefined && held.key === key ? held.read : undefined;
}

/** The most commits the core counts exactly on either side (`files::status::MOST_COUNTED`,
 *  #1152). It counts to one past it, so a count past it means more than that. */
export const MOST_COUNTED = 10_000;

/** A count of commits as the header says it: exactly the core's cap is said as it is, and only a
 *  count past it as "10,000+". */
export function commitsSaid(n: number): string {
  return n > MOST_COUNTED ? `${MOST_COUNTED.toLocaleString("en")}+` : n.toLocaleString("en");
}

/** What the ahead/behind reads as. */
function apartSaid(read: AheadBehindRead | undefined): string {
  if (read === undefined) return "Counting…";
  if (read.trouble !== undefined) return read.trouble;
  const apart = read.apart;
  if (apart === undefined || apart.base === null) return "No base recorded";
  return `${commitsSaid(apart.ahead)} ahead, ${commitsSaid(apart.behind)} behind ${apart.base}`;
}

/** How many files the branch changed, from FM-4's status. */
function changesSaid(read: StatusRead | undefined): string {
  if (read === undefined) return "Reading changes…";
  if (read.trouble !== undefined) return read.trouble;
  const count = read.status?.folders.find((one) => one.folder === "")?.count ?? 0;
  if (count === 0) return "No changes";
  return `${count.toLocaleString("en")} ${count === 1 ? "change" : "changes"}`;
}

/**
 * The way back out: the workspace, the repo and the branch, the branch being where the explorer
 * is. The workspace and the repo each step back out to the whole workspace, as Esc does. On a
 * repo's own folder (#1152, `name` absent) the repo is where the explorer is.
 */
export function Breadcrumb({
  workspace,
  repo,
  name,
  onLeave,
}: {
  workspace: string;
  repo: string;
  /** The branch; absent for the repo's own folder. */
  name?: string;
  onLeave: () => void;
}) {
  if (name === undefined)
    return (
      <nav className="cockpit-crumbs" aria-label="Breadcrumb">
        <ol>
          <li>
            <button type="button" onClick={onLeave} title="Back to the whole workspace (Esc)">
              {workspace}
            </button>
          </li>
          <li>
            <ChevronRight className="node-icon" />
            <span aria-current="location">{repo}</span>
          </li>
        </ol>
      </nav>
    );
  return (
    <nav className="cockpit-crumbs" aria-label="Breadcrumb">
      <ol>
        <li>
          <button type="button" onClick={onLeave} title="Back to the whole workspace (Esc)">
            {workspace}
          </button>
        </li>
        <li>
          <ChevronRight className="node-icon" />
          <button type="button" onClick={onLeave} title="Back to the whole workspace (Esc)">
            {repo}
          </button>
        </li>
        <li>
          <ChevronRight className="node-icon" />
          <span aria-current="location">{name}</span>
        </li>
      </ol>
    </nav>
  );
}

/**
 * The branch's state: its name, how far it is from its base, how many files it changed, and the
 * explorer's own Merge and Done rows (`worktree.merge:` and `worktree.done:` in the catalogue),
 * pressed as the row menu presses them. A row the catalogue cannot run is drawn disabled, with
 * why.
 *
 * **On a repo's own folder** (#1152, `piece` null) it says the branch the clone has checked out
 * and how far that is from its upstream, and has no Merge or Done: both act on a branch folder
 * purlis cut, and the clone is the person's own checkout (D-1152-3).
 */
export function CockpitHeader({
  name,
  repo,
  piece,
  apart,
  status,
  offers,
  onPress,
}: {
  /** What the branch is called: git's branch name, or the folder's when it is on none. For a
   *  repo's own folder, the branch it has checked out, where git said. */
  name: string;
  repo: string;
  /** The branch folder, or `null` for the repo's own folder. */
  piece: string | null;
  apart: AheadBehindRead | undefined;
  status: StatusRead | undefined;
  offers: Catalogued;
  onPress: (offer: Offer) => void;
}) {
  if (piece === null)
    return (
      <section className="cockpit-head" aria-label={`Repo ${repo}`}>
        <h2 className="cockpit-name">
          <FolderGit2 className="node-icon" />
          <span>{repo}</span>
        </h2>
        <p className="cockpit-facts">
          <ApartLine apart={apart} on={name === "" ? undefined : name} />
          <span className="cockpit-changes">{changesSaid(status)}</span>
        </p>
      </section>
    );
  const at = `${repo}/${piece}`;
  const action = (id: string, label: string, Icon: typeof GitMerge) => {
    const offer = offers.get(id);
    if (offer === undefined) return null;
    return (
      <button
        type="button"
        className="cockpit-action"
        disabled={!offer.available}
        title={offer.available ? offer.title : offer.reason}
        onClick={() => onPress(offer)}
      >
        <Icon className="node-icon" />
        {label}
      </button>
    );
  };
  return (
    <section className="cockpit-head" aria-label={`Branch ${name}`}>
      <h2 className="cockpit-name">
        <GitBranch className="node-icon" />
        <span>{name}</span>
      </h2>
      <p className="cockpit-facts">
        <ApartLine apart={apart} />
        <span className="cockpit-changes">{changesSaid(status)}</span>
      </p>
      <div className="cockpit-actions">
        {action(`worktree.merge:${at}`, "Merge", GitMerge)}
        {action(`worktree.done:${at}`, "Done", CircleCheck)}
      </div>
    </section>
  );
}

/** How far the branch is from its base, an alert where it could not be read; on a repo's own
 *  folder, after the branch it has checked out (`on`, #1152). */
function ApartLine({ apart, on }: { apart: AheadBehindRead | undefined; on?: string }) {
  return (
    <span className="cockpit-apart" role={apart?.trouble !== undefined ? "alert" : undefined}>
      {on === undefined ? apartSaid(apart) : `on ${on} · ${apartSaid(apart)}`}
    </span>
  );
}
