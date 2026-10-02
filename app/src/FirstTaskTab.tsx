import { useEffect, useId, useState } from "react";
import * as RadioGroup from "@radix-ui/react-radio-group";
import {
  commands,
  type FirstTaskRun,
  type PlaneId,
  type ProfileRow,
  type StartOptions,
} from "./bindings";

/** The runs the script has (`charter_core::firsttask::RUNS`). */
const RUNS = [1, 2] as const;

/** What each chat is called on the tab: ADR 0072's first-hour words, so "chat" and not "run". */
const ORDINAL: Record<number, string> = { 1: "First chat", 2: "Second chat" };

/** The harness charter cannot type a prompt into (`Harness::ready_to_type`). */
const UNTYPED = "opencode";

/** What the plane does for the tab: starts a run and puts its chat's tab on the strip, and
 *  opens a run's diff. */
export interface FirstTaskDoes {
  /** Starts run `run` in `clone` on `profile`, as `persona`: the run, or why it did not start. */
  start: (
    clone: string,
    profile: string,
    persona: string | null,
    run: number,
  ) => Promise<FirstTaskRun | string>;
  /** Show a run's diff: a shell tab in its branch's folder with its diff command run. */
  showDiff: (run: FirstTaskRun) => void;
  /** The runs started so far, by clone and then by number. The plane's, not the tab's: a run's
   *  chat opens in front, and the tab is drawn again when the operator comes back to it. */
  runs: Readonly<Record<string, Readonly<Partial<Record<number, FirstTaskRun>>>>>;
}

/**
 * **The first task** (FR-28, #621): the guided task FR-1 measures, offered in a tab beside the
 * first chat so it costs none of W10's interrupt budget.
 *
 * The same task twice, on two harnesses or on two profiles of one, each a chat **on a branch of
 * its own** in charter's copy of the repo, with the task **typed and never sent** (ADR 0061): the
 * operator reads it and presses Enter. The task's text, each run's name and the diff command are
 * the core's (`charter_core::firsttask`), which the CI run of the script uses too.
 *
 * **The month-two moment is the second run's briefing**: the task asks each run to record what it
 * learned, and the second run starts with that lesson in its briefing, whichever harness it is on,
 * because the lesson is kept in the project and not in a harness.
 *
 * A profile whose command is not approved yet shows the command in full and is approved by the
 * press that starts the run — the picker's approval, with the same words in front of the operator
 * (ADR 0022).
 */
export function FirstTaskTab({
  plane,
  clone,
  does,
}: {
  plane: PlaneId;
  /** The repo's clone, where each run's branch is cut. */
  clone: string;
  does?: FirstTaskDoes;
}) {
  const [options, setOptions] = useState<StartOptions>();
  const [trouble, setTrouble] = useState<string>();
  const [picked, setPicked] = useState<Partial<Record<number, string>>>({});
  const [starting, setStarting] = useState<number>();
  const groupId = useId();

  useEffect(() => {
    let gone = false;
    void commands
      .startOptions(plane)
      .then((answer) => {
        if (gone) return;
        if (answer.status === "error") setTrouble(answer.error);
        else setOptions(answer.data);
      })
      .catch((err: unknown) => {
        if (!gone) setTrouble(String(err));
      });
    return () => {
      gone = true;
    };
  }, [plane]);

  const runs = does?.runs[clone] ?? {};
  const profiles = options?.profiles ?? [];
  const chosen = (run: number): ProfileRow | undefined => {
    const first = run === 1 ? undefined : chosen(1);
    const name =
      picked[run] ??
      suggested(profiles, run, first && { kind: runs[1]?.harness ?? first.kind, name: first.name });
    return profiles.find((one) => one.name === name);
  };

  async function start(run: number) {
    const profile = chosen(run);
    if (!profile || !options) return;
    setStarting(run);
    setTrouble(undefined);
    if (profile.approval !== null) {
      const approved = await commands
        .approveProfile(plane, profile.name, profile.shown)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (approved.status === "error") {
        setStarting(undefined);
        setTrouble(approved.error);
        return;
      }
    }
    const persona =
      options.persona !== null && options.personas.includes(options.persona)
        ? options.persona
        : null;
    const answer = does
      ? await does.start(clone, profile.name, persona, run)
      : "This tab cannot start a chat here.";
    setStarting(undefined);
    if (typeof answer === "string") {
      setTrouble(answer);
      return;
    }
    if (profile.approval !== null) {
      // Approved now: the row's next read says so, and a second press asks nothing.
      setOptions(
        (was) =>
          was && {
            ...was,
            profiles: was.profiles.map((one) =>
              one.name === profile.name ? { ...one, approval: null } : one,
            ),
          },
      );
    }
  }

  return (
    <div className="first-task">
      <p className="came-back">
        Give one small, real task to two chats, each started with something different below, and
        compare what each did. Each chat works on a branch of its own in charter&apos;s copy of your
        repo, and the task is typed in for you to read before you send it.
      </p>
      {RUNS.map((run) => {
        const done = runs[run];
        const profile = chosen(run);
        const labelId = `${groupId}-${run}`;
        return (
          <section className="first-task-run" key={run} aria-labelledby={labelId}>
            <h3 id={labelId}>{ORDINAL[run]}</h3>
            {run === 2 && (
              <p className="came-back">
                Start it once the first chat has finished. It starts knowing what the first one
                learned: the lesson the first chat recorded is in its memory, whatever it is started
                with.
              </p>
            )}
            {done ? (
              <p className="came-back">
                {`Started on the branch ${done.branch}.`}{" "}
                <button type="button" tabIndex={0} onClick={() => does?.showDiff(done)}>
                  Show its diff
                </button>
              </p>
            ) : (
              <>
                <RadioGroup.Root
                  className="choices"
                  name={`first-task-${run}`}
                  value={profile?.name ?? ""}
                  onValueChange={(name) => setPicked((was) => ({ ...was, [run]: name }))}
                  aria-labelledby={labelId}
                  disabled={starting !== undefined}
                >
                  {profiles.map((one) => (
                    <div className="choice" key={one.name}>
                      <RadioGroup.Item
                        className="dot"
                        value={one.name}
                        id={`${labelId}-${one.name}`}
                        disabled={one.kind === UNTYPED}
                        aria-describedby={`${labelId}-${one.name}-says`}
                      >
                        <RadioGroup.Indicator className="dot-mark" />
                      </RadioGroup.Item>
                      <label className="who" htmlFor={`${labelId}-${one.name}`}>
                        {one.name}
                      </label>
                      <span className="meta" id={`${labelId}-${one.name}-says`}>
                        <span className="what">{profileSays(one)}</span>
                      </span>
                    </div>
                  ))}
                </RadioGroup.Root>
                <div className="doing">
                  <button
                    type="button"
                    tabIndex={0}
                    disabled={profile === undefined || starting !== undefined}
                    onClick={() => void start(run)}
                  >
                    {profile?.approval != null
                      ? `Approve and start the ${ORDINAL[run].toLowerCase()}`
                      : `Start the ${ORDINAL[run].toLowerCase()}`}
                  </button>
                </div>
              </>
            )}
          </section>
        );
      })}
      {trouble && (
        <p className="trouble said-in-full" role="alert">
          {trouble}
        </p>
      )}
    </div>
  );
}

/**
 * The profile a run starts on until the operator picks one: for run 1 the project's default, and
 * for run 2 one of another harness than run 1's (`first`), else another profile, so the two runs
 * differ. Never a profile charter cannot type into.
 */
export function suggested(
  profiles: readonly ProfileRow[],
  run: number,
  first?: { kind: string; name: string },
): string | undefined {
  const typed = profiles.filter((one) => one.kind !== UNTYPED);
  const byDefault = typed.find((one) => one.is_default) ?? typed[0];
  if (run === 1 || first === undefined) return byDefault?.name;
  const other =
    typed.find((one) => one.kind !== first.kind) ?? typed.find((one) => one.name !== first.name);
  return (other ?? byDefault)?.name;
}

function profileSays(row: ProfileRow): string {
  if (row.kind === UNTYPED) return "charter cannot type the task into opencode";
  if (row.approval !== null) return `starting it approves its command: ${row.shown}`;
  return row.shown;
}
