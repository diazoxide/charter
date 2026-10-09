import { useState } from "react";
import { commands } from "../bindings";
import { Choice, type Option } from "./components";
import { KINDS, valueAt, type Shown } from "./fileControls";
import type { LiveSetting } from "./groups";
import type { ProjectRead } from "./project";

/**
 * **Profiles that ask before they act** (#1522), on Settings › Project › Harness & profiles: a
 * box per profile the project offers, ticked where the Local file's `[harness] asks` names it.
 * A chat one chat starts for another (a task, a handoff, your Ask from a tab) starts only on a
 * profile that asks, so ticking one is the one click back after such a start was refused for a
 * profile nobody marked.
 *
 * The built-in `claude` and `codex` ask by their own default and are drawn ticked and fixed. A
 * table of the Local file with one of their names is not the built-in, and has a box of its own.
 *
 * **Written by its own command** (`mark_profile_asks`), which is the window's alone: the mark
 * is what lets a chat start another chat on a profile, so no link sets it, and the settings
 * save a link may call refuses a change to it.
 */

/** Where the mark is kept. */
export const ASKS = [{ key: "harness" }, { key: "asks" }];

/** The built-ins whose harness asks by its own default, which need no mark. */
const ASKS_BY_DEFAULT: readonly string[] = ["claude", "codex"];

/** The boxes, and which are ticked, as the files and the core say. */
export function asksBoxes(read: ProjectRead): {
  options: Option[];
  ticked: ReadonlySet<string>;
  marked: readonly string[];
} {
  const tables = new Set(
    (read.local.entries ?? [])
      .filter((one) => one.collection === "profiles")
      .map((one) => one.values.find((value) => value.field === "name")?.value ?? one.label),
  );
  const byDefault = (name: string) => ASKS_BY_DEFAULT.includes(name) && !tables.has(name);
  const marked = markedIn(read.local);
  const names = [...new Set([...(read.entries.profile ?? KINDS), ...tables])];
  return {
    options: names.map((name) =>
      byDefault(name)
        ? {
            value: name,
            label: `${name} (asks by its own default)`,
            disabled: true,
            title: "The built-in profile asks by its harness's own default.",
          }
        : { value: name, label: name },
    ),
    ticked: new Set([...marked, ...names.filter(byDefault)]),
    marked,
  };
}

/** The names `[harness] asks` holds in `file`. */
function markedIn(file: Shown): string[] {
  const value = valueAt(file, ASKS);
  return value?.kind === "list" ? [...value.value] : [];
}

export function asksSetting(read: ProjectRead): LiveSetting {
  return {
    id: "project.harness.local.harness.asks",
    label: "Profiles that ask before they act",
    help: `A task, a handoff or your Ask from a tab starts only on a profile ticked here. Tick one only once its harness asks you before it acts: opencode allows every action unless its own configuration says otherwise. Kept in ${read.local.file}, on this machine only.`,
    useControl: function useAsks() {
      const [error, setError] = useState<readonly string[]>();
      const [writing, setWriting] = useState(false);
      const { options, ticked } = asksBoxes(read);
      const mark = (name: string, asks: boolean) => {
        if (read.plane === undefined || writing) return;
        setWriting(true);
        setError(undefined);
        void commands
          .markProfileAsks(read.plane, read.local.exists ? read.local.text : null, name, asks)
          .then((done) => {
            if (done.status === "error") setError([done.error]);
            else if (done.data.kind === "refused") setError(done.data.reasons);
            read.reread?.();
          })
          .catch((err: unknown) => setError([`purlis could not mark the profile: ${String(err)}`]))
          .finally(() => setWriting(false));
      };
      return {
        grouped: true,
        error,
        control: (ids) => (
          <Choice
            kind="checks"
            ids={ids}
            options={options.map((one) => (writing ? { ...one, disabled: true } : one))}
            checked={ticked}
            onCheckedChange={mark}
          />
        ),
      };
    },
  };
}
