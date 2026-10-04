import { useId, useRef, type ReactNode } from "react";
import * as Checkbox from "@radix-ui/react-checkbox";
import * as RadioGroup from "@radix-ui/react-radio-group";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { useTabStop } from "../roving";

/**
 * **The house settings set** (SE-16, #1166; DS-3 #626's *expand* step; V89f): the five pieces
 * every settings screen is drawn from — {@link SettingsLayout}, {@link SettingGroup},
 * {@link SettingRow}, {@link Field} and {@link Choice}.
 *
 * `docs/ui-primitives.md` forbids a house component library and allows exactly this set, for the
 * reasons written there: charter had two hand-built form styles that had already drifted
 * (`settings-*` and the dialogs' `choices`), and one small set is how a new setting stays a few
 * lines. Each piece is a thin layer over a native element or a Radix primitive already in the
 * window — radio group, checkbox, roving focus — and **says which at its call site**: the props
 * are the primitive's words (`value`, `onValueChange`, `checked`), not new ones.
 *
 * Drawn in `App.css` under `ui-*` classes, every colour a design-system token. The old
 * `settings-*` and `choices` classes stay until the last screen has moved (expand–contract).
 */

/** One level of the switcher: You, Project, Workspace or Persona (`CONTEXT.md`, **Level**). */
export type LevelOffer = { id: string; label: string };

/** One group in the nav: its stable id, and what the nav calls it. */
export type GroupOffer = { id: string; label: string };

/**
 * **The two columns** (V89b): the level switcher and what that level is above a nav of its
 * groups, and the chosen group beside them.
 *
 * The switcher is a Radix radio group, because a level is one choice of a few; it lists only
 * the levels it is handed, so a level with nothing to set is not offered. The nav is buttons in
 * one Tab stop, moved through with the arrow keys (`roving.ts`, as the explorer's rows are), and
 * the chosen one says so with `aria-current`.
 */
export function SettingsLayout({
  levels,
  level,
  onLevelChange,
  about,
  groups,
  group,
  onGroupChange,
  children,
}: {
  levels: readonly LevelOffer[];
  level: string;
  onLevelChange: (level: string) => void;
  /** One sentence on whose settings these are and where they are kept. */
  about?: ReactNode;
  groups: readonly GroupOffer[];
  group: string;
  onGroupChange: (group: string) => void;
  /** The chosen group: a {@link SettingGroup}. */
  children: ReactNode;
}) {
  const stop = useTabStop(
    group,
    groups.map((one) => one.id),
  );
  return (
    <div className="ui-settings">
      <div className="ui-settings-top">
        <RadioGroup.Root
          className="ui-levels"
          aria-label="Level"
          orientation="horizontal"
          value={level}
          onValueChange={onLevelChange}
        >
          {levels.map((one) => (
            <RadioGroup.Item key={one.id} className="ui-level" value={one.id}>
              {one.label}
            </RadioGroup.Item>
          ))}
        </RadioGroup.Root>
        {about !== undefined && <p className="ui-settings-about">{about}</p>}
      </div>
      <RovingFocusGroup.Root asChild orientation="vertical" {...stop}>
        <nav className="ui-settings-nav" aria-label="Groups">
          {groups.map((one) => (
            <RovingFocusGroup.Item key={one.id} asChild tabStopId={one.id}>
              <button
                type="button"
                aria-current={one.id === group ? "true" : undefined}
                onClick={() => onGroupChange(one.id)}
              >
                {one.label}
              </button>
            </RovingFocusGroup.Item>
          ))}
        </nav>
      </RovingFocusGroup.Root>
      <div className="ui-settings-body">{children}</div>
    </div>
  );
}

/** **One group**, the right column: its heading, what it is for, and its rows. A region named
 *  by its heading, so a screen reader can say where it is. */
export function SettingGroup({
  label,
  help,
  children,
}: {
  label: string;
  help?: string;
  children: ReactNode;
}) {
  const heading = useId();
  return (
    <section className="ui-setting-group" aria-labelledby={heading}>
      <h3 id={heading}>{label}</h3>
      {help && <p className="ui-setting-help">{help}</p>}
      {children}
    </section>
  );
}

/** The ids a row hands its control, so the control is named and described by the row. */
export type RowIds = {
  /** The control's own id; the row's `<label for>` points at it, unless the row is `grouped`. */
  id: string;
  /** The row's name's id, for a control a `<label for>` cannot name — a radio group. */
  labelledBy: string;
  /** The help line's id, when the row has help. */
  describedBy?: string;
};

/** A row's way back to the value beneath it. */
export type Reset = { label: string; disabled: boolean; onReset: () => void };

/**
 * **One setting**: its label, its control, its one line of help, and a reset when the value can
 * go back. The control is drawn by `control`, handed the ids that tie it to the label and the
 * help — a {@link Field} or a {@link Choice}.
 *
 * A `grouped` row's control is a group of controls (a radio group), which a `<label for>` cannot
 * name: its name is drawn as plain text, and the group is named by `aria-labelledby` instead.
 *
 * Which file a value goes to, and where its current value comes from, join the row with SE-18.
 */
export function SettingRow({
  label,
  help,
  reset,
  grouped = false,
  control,
}: {
  label: string;
  /** One line on what the value does. A node and not only a string, because a dialog's help
   *  names files and commands, and those are `<code>` (DS-3c). */
  help?: ReactNode;
  reset?: Reset;
  grouped?: boolean;
  control: (ids: RowIds) => ReactNode;
}) {
  const id = useId();
  const labelledBy = useId();
  const described = useId();
  const describedBy = help ? described : undefined;
  return (
    <div className="ui-setting-row">
      {grouped ? (
        <span className="ui-setting-label" id={labelledBy}>
          {label}
        </span>
      ) : (
        <label className="ui-setting-label" id={labelledBy} htmlFor={id}>
          {label}
        </label>
      )}
      <div className="ui-setting-control">
        {control({ id, labelledBy, describedBy })}
        {reset && (
          <button
            type="button"
            className="ui-setting-reset"
            // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
            tabIndex={0}
            disabled={reset.disabled}
            onClick={reset.onReset}
          >
            {reset.label}
          </button>
        )}
      </div>
      {help && (
        <p className="ui-setting-help" id={described}>
          {help}
        </p>
      )}
    </div>
  );
}

/** What every field and choice is handed: the row's ids. */
type Tied = { ids: RowIds };

/**
 * **A value typed or slid**: `text` is one line, `list` one entry per line, and `range` a whole
 * number between two bounds, with what it is at said beside it. Native elements, because the
 * browser already has each one (`ui-primitives.md`).
 */
export type FieldProps = Tied &
  (
    | {
        kind: "text";
        value: string;
        onChange: (to: string) => void;
        /** What the empty box shows: an example of the answer, never what to do — that is the
         *  row's help. */
        placeholder?: string;
        /** The input's own cap on what can be typed. */
        maxLength?: number;
        /** The input's own `disabled`: out of reach while this answer is moot or its work is
         *  running. */
        disabled?: boolean;
      }
    /** One entry per line. The text is what is typed, kept whole — an empty line while the next
     *  entry is being typed is still there — and the caller reads the entries out of it. */
    | {
        kind: "list";
        value: string;
        onChange: (to: string) => void;
        /** The fewest lines the box shows, before it grows with what is typed. Two unless said. */
        minRows?: number;
      }
    | {
        kind: "range";
        value: number;
        min: number;
        max: number;
        step?: number;
        /** What the value reads as beside the slider: `18px`. */
        shown: (value: number) => string;
        /** What a screen reader says it is, when that is not `shown`: `18 pixels`. */
        spoken?: (value: number) => string;
        onChange: (to: number) => void;
      }
  );

export function Field(props: FieldProps) {
  const { ids } = props;
  if (props.kind === "range")
    return (
      <span className="ui-field ui-field-range">
        <input
          id={ids.id}
          type="range"
          min={props.min}
          max={props.max}
          step={props.step ?? 1}
          value={props.value}
          aria-valuetext={(props.spoken ?? props.shown)(props.value)}
          aria-describedby={ids.describedBy}
          onChange={(event) => props.onChange(event.currentTarget.valueAsNumber)}
        />
        <output htmlFor={ids.id} aria-live="polite">
          {props.shown(props.value)}
        </output>
      </span>
    );
  if (props.kind === "list")
    return (
      <textarea
        id={ids.id}
        className="ui-field"
        value={props.value}
        spellCheck={false}
        rows={Math.max(props.minRows ?? 2, props.value.split("\n").length)}
        aria-describedby={ids.describedBy}
        onChange={(event) => props.onChange(event.currentTarget.value)}
      />
    );
  return (
    <input
      id={ids.id}
      className="ui-field"
      type="text"
      value={props.value}
      placeholder={props.placeholder}
      maxLength={props.maxLength}
      disabled={props.disabled}
      // A path or a name, not something the webview should offer to fill from history.
      autoComplete="off"
      spellCheck={false}
      aria-describedby={ids.describedBy}
      onChange={(event) => props.onChange(event.currentTarget.value)}
    />
  );
}

/** One option of a choice: its value, what it is called, and a line on what it does. The line
 *  is words, or a few marked-up facts (a harness's kind, command and source) that describe the
 *  option as one line. */
export type Option = { value: string; label: string; says?: ReactNode };

/**
 * **A value picked**: `radio` one of a few, each with a line on what it does (a Radix radio
 * group); `select` one of many (the native `<select>`); `toggle` on or off (a Radix checkbox).
 */
export type ChoiceProps = Tied &
  (
    | {
        kind: "radio";
        options: readonly Option[];
        value: string | undefined;
        onValueChange: (to: string) => void;
        /** The group's own `disabled`: held while what it feeds is being done. */
        disabled?: boolean;
      }
    | {
        kind: "select";
        options: readonly Option[];
        value: string | undefined;
        /** What the empty option says; none is offered without it. */
        unset?: string;
        onValueChange: (to: string) => void;
      }
    | { kind: "toggle"; checked: boolean; onCheckedChange: (to: boolean) => void }
  );

/** The keys a radio group moves its pick with (Radix's own list). */
const ARROWS = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"];

export function Choice(props: ChoiceProps) {
  const { ids } = props;
  /**
   * Whether an arrow key is down in the radio group, so the option it moves to is picked.
   *
   * Radix means a radio's pick to follow the arrow keys, and learns that one is down from a
   * `keydown` listener on `document`, which hears the key only after React's handlers (on the
   * root and on each portal, below `document`) have set the focus moving. So one press moved
   * the focus and picked nothing, and only a held key picked (`docs/ui-primitives.md`, "A radio
   * group's pick does not follow the arrow keys on its own"). Heard here, in the capture phase,
   * it is known before the focus moves, and forgotten when the key comes up or the focus leaves
   * the group. A choice that writes something with no Undo, or starts something, still holds
   * the pick and acts on a button (DS-3b, DS-3d).
   */
  const arrowing = useRef(false);
  if (props.kind === "toggle")
    return (
      <Checkbox.Root
        id={ids.id}
        className="box"
        // #186: Radix's checkbox is a `<button>`, and WebKit leaves one out of the Tab order
        // without `tabIndex` — New project's box was reachable by nothing until it had one.
        tabIndex={0}
        checked={props.checked}
        aria-describedby={ids.describedBy}
        onCheckedChange={(to) => props.onCheckedChange(to === true)}
      >
        <Checkbox.Indicator>✓</Checkbox.Indicator>
      </Checkbox.Root>
    );
  if (props.kind === "select")
    return (
      <select
        id={ids.id}
        className="ui-field"
        // #190: WebKit leaves a control out of the Tab order without `tabIndex`.
        tabIndex={0}
        value={props.value ?? ""}
        aria-describedby={ids.describedBy}
        onChange={(event) => props.onValueChange(event.currentTarget.value)}
      >
        {props.unset !== undefined && <option value="">{props.unset}</option>}
        {props.options.map((one) => (
          <option key={one.value} value={one.value}>
            {one.label}
          </option>
        ))}
      </select>
    );
  return (
    <RadioGroup.Root
      id={ids.id}
      className="ui-choice-radio"
      value={props.value ?? ""}
      onValueChange={props.onValueChange}
      disabled={props.disabled}
      aria-labelledby={ids.labelledBy}
      aria-describedby={ids.describedBy}
      onKeyDownCapture={(event) => {
        if (ARROWS.includes(event.key)) arrowing.current = true;
      }}
      onBlurCapture={(event) => {
        // Out of the group, an arrow held on the way out is no longer a move between options.
        if (!event.currentTarget.contains(event.relatedTarget as Node | null))
          arrowing.current = false;
      }}
      onKeyUpCapture={() => {
        // After the move, not before it: the roving focus moves on a timer of its own, set
        // when the key went down, so a quick press is up before the focus arrives.
        setTimeout(() => {
          arrowing.current = false;
        });
      }}
    >
      {props.options.map((one) => (
        <div className="ui-choice-option" key={one.value}>
          <RadioGroup.Item
            className="dot"
            value={one.value}
            id={`${ids.id}-${one.value}`}
            aria-describedby={one.says ? `${ids.id}-${one.value}-says` : undefined}
            onFocus={() => {
              if (arrowing.current && !props.disabled && one.value !== props.value)
                props.onValueChange(one.value);
            }}
          >
            <RadioGroup.Indicator className="dot-mark" />
          </RadioGroup.Item>
          <label htmlFor={`${ids.id}-${one.value}`}>{one.label}</label>
          {one.says && (
            <span className="ui-choice-says" id={`${ids.id}-${one.value}-says`}>
              {one.says}
            </span>
          )}
        </div>
      ))}
    </RadioGroup.Root>
  );
}
