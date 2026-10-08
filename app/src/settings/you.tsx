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
import { setChatsListPrefs, useChatsListPrefs } from "../chatsListPrefs";
import { Choice, Field } from "./components";
import type { Setting, SettingsGroup } from "./groups";
import { thisMachineGroup } from "./thisMachine";

/**
 * **The You level's groups** (V89h): Text and Editor, which were the Preferences tab's
 * (charter-app#283, RC-20), and This machine (ST-2), which is the machine store's. Nothing here is a project's: each value is this machine's, kept in
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
      help: "The editor purlis hands a file to.",
      settings: [editor],
    },
    {
      id: "you.chats",
      label: "Chats list",
      help: "How the list of chats in the sidebar is drawn, on this machine.",
      settings: [chatRows, chatsGrouped, tasksTabbed],
    },
    thisMachineGroup(),
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
            spoken={(px) => `${px} pixels`}
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
      grouped: true,
    };
  },
};

/** Whether a row of the Chats list is two lines or one (#1499, V100-73). */
const chatRows: Setting = {
  id: "you.chats.rows",
  label: "Rows",
  help: "How much each chat's row says.",
  useControl: function useChatRows() {
    const { lines } = useChatsListPrefs();
    return {
      control: (ids) => (
        <Choice
          kind="radio"
          ids={ids}
          options={[
            {
              value: "2",
              label: "Two lines",
              says: "Name and state, then where it works and for how long.",
            },
            {
              value: "1",
              label: "One line",
              says: "Name and state only. What the second line said is the row's tooltip.",
            },
          ]}
          value={String(lines)}
          onValueChange={(to) => setChatsListPrefs({ lines: to === "1" ? 1 : 2 })}
        />
      ),
      grouped: true,
    };
  },
};

/** Whether a pressed task opens in a tab of its own (#1489, V100-74). */
const tasksTabbed: Setting = {
  id: "you.chats.tabbed",
  label: "Open tasks in their own tabs",
  help: "A task you press opens as a tab of its own, with − in place of ×: − sends it back into its session's tab and ends nothing. Off, the session's tab is switched to the task. Either way the session's tab lists its tasks.",
  useControl: function useTasksTabbed() {
    const { tabbed } = useChatsListPrefs();
    return {
      control: (ids) => (
        <Choice
          kind="toggle"
          ids={ids}
          checked={tabbed}
          onCheckedChange={(to) => setChatsListPrefs({ tabbed: to })}
        />
      ),
    };
  },
};

/** Whether the Chats list groups its sessions by workspace (#1499, V100-73). */
const chatsGrouped: Setting = {
  id: "you.chats.grouped",
  label: "Group by workspace",
  help: "The sessions of one workspace stand together, the workspaces by name. Off, the list is one order: needs you, working, then idle.",
  useControl: function useChatsGrouped() {
    const { grouped } = useChatsListPrefs();
    return {
      control: (ids) => (
        <Choice
          kind="toggle"
          ids={ids}
          checked={grouped}
          onCheckedChange={(to) => setChatsListPrefs({ grouped: to })}
        />
      ),
    };
  },
};
