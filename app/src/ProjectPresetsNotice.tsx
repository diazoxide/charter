import { useEffect, useState } from "react";
import { commands, type PlaneId, type PresetsChanged } from "./bindings";
import { Notice } from "./Notice";
import { SETTINGS, usePlaneChanged } from "./planeChanged";

/**
 * **The project's Internet access presets changed** (ADR 0067 §1 as amended, spec #1330, #1385):
 * the one-time Notice each teammate sees when the presets a project's committed settings turn on
 * change, or its certificate checks, so a preset that widens never widens unseen. They apply with
 * no approval; this tells, and asks nothing. It is {@link ProjectHostsNotice}'s twin.
 *
 * It names what was turned on and off, and what each one turned on widens past its hosts (the
 * core's sentences), and stands until it is read: "Got it" records the set as it was shown, on
 * this machine (`acknowledge_project_presets`), so a change made after it was shown is told
 * again. "Review Internet access" opens the Sandbox group. A change made in this window's own
 * Settings is recorded as seen there, so it is never told back to the person who made it.
 */
export function ProjectPresetsNotice({
  plane,
  onReview,
}: {
  plane: PlaneId;
  /** Opens Settings at the project's Sandbox group. */
  onReview: () => void;
}) {
  const [changed, setChanged] = useState<PresetsChanged>();

  // Read again when the project's settings change on disk (a pull, a branch switched, a hand's
  // edit, #1550), so a change is told while the window is open, before more chats start.
  const onDisk = usePlaneChanged([plane], SETTINGS);

  useEffect(() => {
    let live = true;
    void commands
      .sandboxState(plane)
      .then((said) => {
        if (live && said.status === "ok") setChanged(said.data?.presets_changed ?? undefined);
      })
      // A project whose state cannot be read says nothing here: the presets still apply.
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [plane, onDisk]);

  if (changed === undefined) return null;

  const read = () =>
    void commands
      .acknowledgeProjectPresets(plane, changed.now)
      .then((said) => {
        if (said.status === "ok") setChanged(said.data.presets_changed ?? undefined);
      })
      // Not recorded: it is told again next time, which is the safe way to be wrong.
      .catch(() => {});

  return (
    <Notice
      cause="sandbox-presets"
      label="The project's Internet access changed"
      fixes={[
        { label: "Got it", onPress: read },
        { label: "Review Internet access", onPress: onReview },
      ]}
    >
      <p>
        What chats in this project may reach has changed in the project's settings, which everyone
        who opens it follows.
        {changed.added.length > 0 && <> Turned on: {changed.added.join(", ")}.</>}
        {changed.removed.length > 0 && <> Turned off: {changed.removed.join(", ")}.</>}
        {changed.widens.map((one) => (
          <span key={one}> {one}</span>
        ))}
      </p>
    </Notice>
  );
}
