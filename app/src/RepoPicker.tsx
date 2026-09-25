import { useCallback, useEffect, useId, useRef, useState } from "react";
import * as Checkbox from "@radix-ui/react-checkbox";
import { LoaderCircle } from "lucide-react";
import { commands, type PlaneId, type ReachableRepos } from "./bindings";

/**
 * **The repos this operator can reach on the plane's forges, to tick** (ADR 0055).
 *
 * Asked each time it is drawn, as the operator — `reachable_repos` runs their own `gh` or
 * `glab` — and held nowhere but here: two engineers on one plane reach different repos, and a
 * list saved into the plane would be one of them speaking for both. Refresh asks again.
 *
 * A forge that did not answer says so in its CLI's own words (`gh auth login`, most often), and
 * the rest still list. Nothing here stops a workspace being made with no repos at all.
 */
const NOTHING: ReachableRepos = { repos: [], trouble: [] };

export function RepoPicker({
  plane,
  picked,
  onPicked,
}: {
  plane: PlaneId;
  /** The names ticked. */
  picked: ReadonlySet<string>;
  onPicked: (next: Set<string>) => void;
}) {
  const [found, setFound] = useState<ReachableRepos | { refused: string }>();
  const [filter, setFilter] = useState("");
  const filterId = useId();
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
        <label htmlFor={filterId}>Repos</label>
        <input
          id={filterId}
          value={filter}
          placeholder="Filter"
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => setFilter(event.target.value)}
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
        <p className="trouble" role="alert">
          {found.refused}
        </p>
      ) : (
        <>
          {found.trouble.map((line) => (
            <p key={line} className="trouble" role="alert">
              {line}
            </p>
          ))}
          {found.repos.length === 0 && found.trouble.length === 0 && (
            <p className="came-back">
              Your forge login reaches no repos under this plane&apos;s owners.
            </p>
          )}
          <ul className="repo-picks" aria-label="Repos you can reach">
            {shown.map((repo) => (
              <RepoRow
                key={repo.path}
                name={repo.name}
                path={repo.path}
                description={repo.description}
                checked={picked.has(repo.name)}
                onChecked={(on) => toggle(repo.name, on)}
              />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

function RepoRow({
  name,
  path,
  description,
  checked,
  onChecked,
}: {
  name: string;
  path: string;
  description: string;
  checked: boolean;
  onChecked: (on: boolean) => void;
}) {
  const id = useId();
  return (
    <li className="choice">
      <Checkbox.Root
        id={id}
        className="box"
        checked={checked}
        onCheckedChange={(next) => onChecked(next === true)}
        tabIndex={0}
        aria-label={name}
      >
        <Checkbox.Indicator className="box-mark">✓</Checkbox.Indicator>
      </Checkbox.Root>
      <label className="who" htmlFor={id} title={description || undefined}>
        {path}
      </label>
    </li>
  );
}
