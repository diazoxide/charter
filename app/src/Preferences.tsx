import { useId } from "react";
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

/**
 * **Preferences** (charter-app#283): how this machine's window is drawn, in a view tab of its own
 * (`tabs.PREFERENCES_VIEW`), opened from the app menu, the palette and `⌘,`.
 *
 * **Nothing here is a project's.** Project settings (#252) is a plane's two files; this is the
 * machine's layout file (`charter/layout.json`, beside `machine.json`), so a size set here is the
 * same in every project and reaches no clone. Today it is the two text sizes; it is where the
 * next per-machine preference goes.
 *
 * **Every change applies as it is made** — the window's text the moment the slider moves, the
 * terminals refitted to theirs — and is written to the file then. There is no Save, because
 * there is nothing a half-made change could break.
 */
export function Preferences() {
  const sizes = useTextSizes();
  const where = atCreation().layout.path || "charter's layout file";
  return (
    <div className="settings" data-testid="preferences">
      <p className="settings-who">{`This machine only, in every project. Kept in ${where}.`}</p>
      <fieldset className="settings-group">
        <legend>Text</legend>
        <TextSize which="window" size={sizes.window} />
        <TextSize which="terminal" size={sizes.terminal} />
      </fieldset>
    </div>
  );
}

/** Where each size's keys work, said under its slider. */
function keysFor(which: Which): string {
  const mod = onAMac() ? "⌘" : "Ctrl+";
  const keys = `${mod}+ / ${mod}− / ${mod}0`;
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
