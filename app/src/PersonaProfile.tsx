import { useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import type { PersonaProfile as Read } from "./bindings";
import { Choice, SettingActions, SettingRow } from "./settings/components";

/** The value that stands for "names no profile": no profile is called the empty string. */
const NONE = "";

/**
 * The profile a persona's chats start on (#1445), set from the persona view.
 *
 * **Only a profile the project offers can be picked.** The rows are the project's own profiles
 * on this machine, read by the core, and the core refuses any other name whatever this dialog
 * sends: the line it writes is read back as what a chat starts on. A persona's definition is a
 * file a chat can edit, so a name there the project does not offer is shown here as what it
 * is, picked by nothing, and cleared by any choice made.
 *
 * "None" means none: a chat handed to the persona then starts on the asking chat's profile.
 * For a persona that names its own profile that removes the line. For one that inherits a
 * profile from the persona it extends, the dialog says whose it is, and the core writes
 * `profile: none` in this persona's own definition, so it opts out and its parent is untouched.
 *
 * It writes one `profile:` line in the persona's own `persona.md`, a change to commit like any
 * other edit of that file.
 */
export function PersonaProfile({
  persona,
  read,
  unreadable,
  trouble,
  saving,
  onSave,
  onCancel,
}: {
  persona: string;
  /** What the definition names and what the project offers; absent while it is being read. */
  read?: Read;
  /** Why it could not be read, in the core's words. */
  unreadable?: string;
  /** Why the last save wrote nothing, in the core's words. */
  trouble?: string;
  saving: boolean;
  /** The profile picked, or `null` for none. */
  onSave: (profile: string | null) => void;
  onCancel: () => void;
}) {
  // What the person picked here; until they pick, the row the definition names, where the
  // project offers it.
  const [picked, setPicked] = useState<string>();
  const named = read?.named ?? null;
  const offered = read?.offered ?? [];
  const unknown = named !== null && !offered.includes(named);
  const parent = read?.inherited_from ?? null;
  const value = picked ?? (named !== null && !unknown ? named : unknown ? undefined : NONE);
  const cancel = useRef<HTMLButtonElement>(null);
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !saving) onCancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="asking" />
        <Dialog.Content
          className="warning"
          aria-describedby={undefined}
          onInteractOutside={(e) => e.preventDefault()}
          onOpenAutoFocus={(e) => {
            e.preventDefault();
            cancel.current?.focus();
          }}
        >
          <Dialog.Title>Profile of {persona}</Dialog.Title>
          <p className="where">
            Written as <code>profile:</code> in <code>personas/{persona}/persona.md</code>
          </p>

          {!read && !unreadable && (
            <p className="pending" aria-busy="true">
              Reading the profile {persona} names…
            </p>
          )}
          {unreadable && (
            <p className="trouble" role="alert">
              {unreadable}
            </p>
          )}
          {unknown && (
            <p className="honest">
              Its definition names <code>{named}</code>, which this project does not offer, so no
              chat starts on it. Pick one of the project&apos;s profiles, or none.
            </p>
          )}

          {parent !== null && (
            <p className="honest">
              Inherited from <code>{parent}</code>, which this persona extends. A choice here is
              written in this persona&apos;s own definition, and <code>{parent}</code> keeps its
              profile.
            </p>
          )}

          {read && (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                if (value !== undefined && !saving) onSave(value === NONE ? null : value);
              }}
            >
              <SettingRow
                label="Profile"
                grouped
                help={
                  "The harness profile this persona's chats start on. With none, a chat handed " +
                  "to it starts on the asking chat's profile."
                }
                control={(ids) => (
                  <Choice
                    ids={ids}
                    kind="radio"
                    options={[
                      { value: NONE, label: "None" },
                      ...offered.map((name) => ({ value: name, label: name })),
                    ]}
                    value={value}
                    onValueChange={setPicked}
                  />
                )}
              />

              {trouble && (
                <p className="trouble" role="alert">
                  {trouble}
                </p>
              )}

              <SettingActions>
                <button type="submit" tabIndex={0} disabled={value === undefined || saving}>
                  {saving ? "Saving…" : "Save profile"}
                </button>
                <button
                  type="button"
                  tabIndex={0}
                  ref={cancel}
                  disabled={saving}
                  onClick={onCancel}
                >
                  Cancel
                </button>
              </SettingActions>
            </form>
          )}
          {!read && (
            <SettingActions>
              <button type="button" tabIndex={0} ref={cancel} onClick={onCancel}>
                Cancel
              </button>
            </SettingActions>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
