import { useId } from "react";
import * as RadioGroup from "@radix-ui/react-radio-group";
import type { YourEditor as Editor } from "./bindings";
import { onAMac } from "./tabKeys";
import {
  DEFAULT_TEXT,
  LEAST_TEXT,
  MOST_TEXT,
  resetText,
  setTextSize,
  TEXT_NAMES,
  useTextSizes,
  type Which,
} from "./textSize";
import { atCreation } from "./windowprefs";
import { EDITORS, setYourEditor, useYourEditor } from "./yourEditor";

/**
 * **Preferences** (charter-app#283): how this machine's window is drawn, in a view tab of its own
 * (`tabs.PREFERENCES_VIEW`), opened from the app menu, the palette and `⌘,`.
 *
 * **Nothing here is a project's.** Project settings (#252) is a plane's two files; this is the
 * machine's layout file (`charter/layout.json`, beside `machine.json`), so a size set here is the
 * same in every project and reaches no clone. Today it is the two text sizes and your editor
 * (RC-20); it is where the next per-machine preference goes.
 *
 * **Every change applies as it is made** — the window's text the moment the slider moves, the
 * terminals refitted to theirs — and is written to the file then. There is no Save, because
 * there is nothing a half-made change could break.
 */
export function Preferences() {
  const sizes = useTextSizes();
  const where = atCreation().layout.path || "the layout file";
  return (
    <div className="settings" data-testid="preferences">
      <p className="settings-who">{`This machine only, in every project. Kept in ${where}.`}</p>
      <fieldset className="settings-group">
        <legend>Text</legend>
        <TextSize which="window" size={sizes.window} />
        <TextSize which="terminal" size={sizes.terminal} />
      </fieldset>
      <YourEditorChoice />
    </div>
  );
}

/** Where each size's keys work, said under its slider. */
function keysFor(which: Which): string {
  const keys = onAMac() ? "⌘= / ⌘- / ⌘0" : "Ctrl+= / Ctrl+- / Ctrl+0";
  return which === "window"
    ? `Everything but the terminals. ${keys} change it from anywhere outside a terminal.`
    : `Every chat's terminal, refitted to the new size. ${keys} change it from inside one.`;
}

function TextSize({ which, size }: { which: Which; size: number }) {
  const id = useId();
  const name = TEXT_NAMES[which];
  const label = name.charAt(0).toUpperCase() + name.slice(1);
  return (
    <div className="settings-field">
      <label htmlFor={id}>{label}</label>
      <div className="settings-actions text-size">
        <input
          id={id}
          type="range"
          min={LEAST_TEXT}
          max={MOST_TEXT}
          step={1}
          value={size}
          aria-valuetext={`${size} pixels`}
          onChange={(e) => setTextSize(which, e.currentTarget.valueAsNumber)}
        />
        <output htmlFor={id} aria-live="polite">{`${size}px`}</output>
        <button
          type="button"
          // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
          tabIndex={0}
          disabled={size === DEFAULT_TEXT[which]}
          onClick={() => resetText(which)}
        >
          {`Reset to ${DEFAULT_TEXT[which]}px`}
        </button>
      </div>
      <p className="settings-hint">{keysFor(which)}</p>
    </div>
  );
}

/** Which editor *Open in your editor* hands a file and a line to (RC-20, ADR 0081 §3). */
function YourEditorChoice() {
  const id = useId();
  const editor = useYourEditor();
  return (
    <fieldset className="settings-group">
      <legend id={id}>Your editor</legend>
      <p className="settings-hint">
        Where Open in your editor sends a file, at the line you are reading.
      </p>
      <RadioGroup.Root
        className="choices"
        name="your-editor"
        value={editor ?? ""}
        onValueChange={(value) => setYourEditor(value as Editor)}
        aria-labelledby={id}
      >
        {EDITORS.map((one) => (
          <div className="choice" key={one.id}>
            <RadioGroup.Item
              className="dot"
              value={one.id}
              id={`${id}-${one.id}`}
              aria-describedby={`${id}-${one.id}-says`}
            >
              <RadioGroup.Indicator className="dot-mark" />
            </RadioGroup.Item>
            <label className="who" htmlFor={`${id}-${one.id}`}>
              {one.name}
            </label>
            <span className="meta" id={`${id}-${one.id}-says`}>
              <span className="what">{one.says}</span>
            </span>
          </div>
        ))}
      </RadioGroup.Root>
    </fieldset>
  );
}
