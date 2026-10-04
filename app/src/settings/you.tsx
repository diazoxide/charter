import type { YourEditor } from "../bindings";
import { onAMac } from "../tabKeys";
import {
  DEFAULT_TEXT,
  LEAST_TEXT,
  MOST_TEXT,
  resetText,
  setTextSize,
  TEXT_NAMES,
  useTextSizes,
  type Which,
} from "../textSize";
import { EDITORS, setYourEditor, useYourEditor } from "../yourEditor";
import { Choice, Field } from "./components";
import type { Setting, SettingsGroup } from "./groups";

/**
 * **The You level's groups** (V89h): Text and Editor, which were the Preferences tab's
 * (charter-app#283, RC-20). Nothing here is a project's: each value is this machine's, kept in
 * its layout file (`charter/layout.json`, beside `machine.json`), so it is the same in every
 * project and reaches no clone.
 *
 * **Every change applies as it is made**, as it did on Preferences: the window's text the
 * moment the slider moves, the terminals refitted to theirs, the editor the moment it is picked.
 */
export function youGroups(): SettingsGroup[] {
  return [
    {
      id: "you.text",
      label: "Text",
      help: "How big the text is drawn, on this machine.",
      settings: [textSize("window"), textSize("terminal")],
    },
    {
      id: "you.editor",
      label: "Editor",
      help: "The editor charter hands a file to.",
      settings: [editor],
    },
  ];
}

/** Where each size's keys work, said under its slider. */
function keysFor(which: Which): string {
  const keys = onAMac() ? "⌘= / ⌘- / ⌘0" : "Ctrl+= / Ctrl+- / Ctrl+0";
  return which === "window"
    ? `Everything but the terminals. ${keys} change it from anywhere outside a terminal.`
    : `Every chat's terminal, refitted to the new size. ${keys} change it from inside one.`;
}

function textSize(which: Which): Setting {
  const name = TEXT_NAMES[which];
  return {
    id: `you.text.${which}`,
    label: name.charAt(0).toUpperCase() + name.slice(1),
    help: keysFor(which),
    useControl: function useTextSize() {
      const size = useTextSizes()[which];
      return {
        control: (ids) => (
          <Field
            kind="range"
            ids={ids}
            min={LEAST_TEXT}
            max={MOST_TEXT}
            value={size}
            shown={(px) => `${px}px`}
            onChange={(to) => setTextSize(which, to)}
          />
        ),
        reset: {
          label: `Reset to ${DEFAULT_TEXT[which]}px`,
          disabled: size === DEFAULT_TEXT[which],
          onReset: () => resetText(which),
        },
      };
    },
  };
}

/** Which editor *Open in your editor* hands a file and a line to (RC-20, ADR 0081 §3). */
const editor: Setting = {
  id: "you.editor.yours",
  label: "Your editor",
  help: "Where Open in your editor sends a file, at the line you are reading.",
  useControl: function useEditor() {
    const chosen = useYourEditor();
    return {
      control: (ids) => (
        <Choice
          kind="radio"
          ids={ids}
          options={EDITORS.map((one) => ({ value: one.id, label: one.name, says: one.says }))}
          value={chosen}
          onValueChange={(to) => setYourEditor(to as YourEditor)}
        />
      ),
    };
  },
};
