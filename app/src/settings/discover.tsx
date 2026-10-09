import { useState } from "react";
import { commands, type DoctorFixed, type PlaneId } from "../bindings";
import type { LiveSetting } from "./groups";

/** The doctor's fix that catalogues what the project's forges list (FX-2, `doctor/fix.rs`). */
export const DISCOVER = "discover";

/**
 * **Settings › Saving's way to discover** (#1390; the spec on #1221, "Repo save policies: they
 * follow the catalogue, with a link to discover"). A repo gets a save policy here once
 * `inventory/repos.json` catalogues it, so the row that says which repos are catalogued — none,
 * or not every one — offers Discover: the doctor's `discover` fix, through the same command its
 * Fix button sends, which asks each forge the project declares which repos it lists and adds
 * them, keeping every repo already there. It goes over the network, so it runs only when
 * pressed. What it said is shown under the button, and the level is read again so each new
 * repo's settings are drawn.
 */
export function discoverRow(
  plane: PlaneId,
  catalogued: number,
  reread: (() => void) | undefined,
): LiveSetting {
  return {
    id: "project.saving.discover",
    label: "Repos not catalogued",
    help:
      catalogued === 0
        ? "No repo is catalogued in inventory/repos.json or named by either file, so none has a save policy here. Discover asks the project's forges which repos they list and catalogues them."
        : "A repo inventory/repos.json does not catalogue has no save policy here. Discover asks the project's forges which repos they list and catalogues the ones it lacks.",
    useControl: function useDiscover() {
      const [running, setRunning] = useState(false);
      const [fixed, setFixed] = useState<DoctorFixed>();
      const discover = () => {
        setRunning(true);
        setFixed(undefined);
        void commands
          .planeDoctorFix(plane, DISCOVER)
          .then((answer) =>
            answer.status === "ok"
              ? answer.data
              : { fix: DISCOVER, refused: answer.error, said: [], complete: false },
          )
          .catch((err: unknown) => ({
            fix: DISCOVER,
            refused: String(err),
            said: [],
            complete: false,
          }))
          .then((done) => {
            setRunning(false);
            setFixed(done);
            // Read again whatever came of it: a discover that half-ran catalogued something too.
            reread?.();
          });
      };
      return {
        grouped: true,
        error: fixed?.refused ? [fixed.refused] : undefined,
        control: (ids) => (
          <div
            id={ids.id}
            role="group"
            className="ui-setting-status"
            aria-labelledby={ids.labelledBy}
            aria-describedby={ids.describedBy}
            aria-busy={running}
          >
            <button type="button" tabIndex={0} disabled={running} onClick={discover}>
              {running ? "Discovering…" : "Discover repos"}
            </button>
            {fixed !== undefined &&
              fixed.said.map((line, at) => (
                <p key={at} className="ui-setting-help">
                  {line}
                </p>
              ))}
          </div>
        ),
      };
    },
  };
}
