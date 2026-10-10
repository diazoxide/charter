import { useCallback, useEffect, useId, useRef, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { commands, type PlaneId, type ReachableRepos } from "./bindings";
import { Choice, Field } from "./settings/components";

/**
 * **The repos this operator can reach on the plane's forges, to tick** (ADR 0055).
 *
 * Asked each time it is drawn, as the operator — `reachable_repos` runs their own `gh` or
 * `glab` — and held nowhere but here: two engineers on one plane reach different repos, and a
 * list saved into the plane would be one of them speaking for both. Refresh asks again.
 *
 * A forge that did not answer says so in its CLI's own words (`gh auth login`, most often), and
 * the rest still list. Nothing here stops a workspace being made with no repos at all.
 *
 * Drawn from the settings set (DS-3e): the filter is a `Field`, and the repos are a `Choice` of
 * boxes, each named by the repo's name — what the workspace calls its clone — with where it
 * lives on the forge, and what the forge says it is, on the line under it.
 */
const NOTHING: ReachableRepos = { repos: [], trouble: [] };

/**
 * The window event the picker sends to have a forge's login typed in a shell tab (NO-8, #1233):
 * its project's window (`PlaneView`) hears it, as the Saving view's ways out are heard. An
 * event and not a prop, because the picker is drawn by a dialog in the window and by Settings,
 * and only the project's window opens shell tabs.
 */
export const FORGE_LOGIN = "forge-login-asked";

/** What a login asks of a project's window: which project, and the line to type. */
export type LoginAsk = { plane: string; line: string };

/** Asks the window of the project `plane` to type `line` in a shell tab at its root. */
export function askLogin(plane: string, line: string): void {
  window.dispatchEvent(new CustomEvent<LoginAsk>(FORGE_LOGIN, { detail: { plane, line } }));
}

export function RepoPicker({
  plane,
  picked,
  onPicked,
  onLeave,
}: {
  plane: PlaneId;
  /** The names ticked. */
  picked: ReadonlySet<string>;
  onPicked: (next: Set<string>) => void;
  /** Called after the picker sent the operator to a shell tab: a dialog holding it closes,
   *  so the tab is not under it. */
  onLeave?: () => void;
}) {
  const [found, setFound] = useState<ReachableRepos | { refused: string }>();
  const [filter, setFilter] = useState("");
  const filterId = useId();
  const filterLabel = useId();
  const listId = useId();
  const listLabel = useId();
  /** The newest listing out: an older answer arriving late is dropped. */
  const asking = useRef(0);
  const ask = useCallback(() => {
    const mine = ++asking.current;
    void commands
      .reachableRepos(plane)
      .then((said) => (said.status === "ok" ? (said.data ?? NOTHING) : { refused: said.error }))
      .catch((err: unknown) => ({ refused: String(err) }))
      .then((answer) => {
        if (asking.current === mine) setFound(answer);
      });
  }, [plane]);
  useEffect(ask, [ask]);
  const refresh = () => {
    setFound(undefined);
    ask();
  };

  const toggle = (name: string, on: boolean) => {
    const next = new Set(picked);
    if (on) next.add(name);
    else next.delete(name);
    onPicked(next);
  };
  const wanted = filter.trim().toLowerCase();
  const shown =
    found !== undefined && "repos" in found
      ? found.repos.filter(
          (repo) =>
            wanted === "" || repo.path.toLowerCase().includes(wanted) || picked.has(repo.name),
        )
      : [];

  return (
    <div className="repo-picker" data-testid="repo-picker">
      <div className="repo-picker-head">
        <label id={filterLabel} htmlFor={filterId}>
          Filter repos
        </label>
        <Field
          ids={{ id: filterId, labelledBy: filterLabel }}
          kind="text"
          value={filter}
          onChange={setFilter}
        />
        <button type="button" tabIndex={0} onClick={refresh} disabled={found === undefined}>
          Refresh
        </button>
      </div>
      {found === undefined ? (
        <p className="pending" aria-busy="true">
          <LoaderCircle className="node-icon spinning" />
          Asking your forge which repos you can reach…
        </p>
      ) : "refused" in found ? (
        <p className="trouble" role="status">
          {found.refused}
        </p>
      ) : (
        <>
          {found.trouble.map(({ said, login }) => (
            <div key={said}>
              <p className="trouble" role="status">
                {said}
              </p>
              {/* The forge CLI's own login, typed in a shell tab and left for the operator to
                  run (NO-8, #1233): the host is the project's, so they read it before Return. */}
              {login !== null && (
                <p className="honest">
                  Log in with <code>{login}</code>.{" "}
                  <button
                    type="button"
                    tabIndex={0}
                    onClick={() => {
                      askLogin(plane, login);
                      onLeave?.();
                    }}
                  >
                    Type it in a shell tab
                  </button>
                </p>
              )}
            </div>
          ))}
          {found.repos.length === 0 && found.trouble.length === 0 && (
            <p className="came-back">
              Your forge login reaches no repos under this project&apos;s owners.
            </p>
          )}
          {/* A filter that matched nothing says so, rather than leave an empty list (#630). */}
          {shown.length === 0 && found.repos.length > 0 && (
            <p className="came-back">No repo you can reach matches {filter.trim()}.</p>
          )}
          <span id={listLabel} hidden>
            Repos you can reach
          </span>
          <div className="repo-picks">
            <Choice
              ids={{ id: listId, labelledBy: listLabel }}
              kind="checks"
              options={shown.map((repo) => ({
                value: repo.name,
                label: repo.name,
                says: repo.description ? `${repo.path} · ${repo.description}` : repo.path,
              }))}
              checked={picked}
              onCheckedChange={toggle}
            />
          </div>
        </>
      )}
    </div>
  );
}
