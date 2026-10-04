import { useId, type ReactNode } from "react";
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
  /** The control's own id; the row's `<label for>` points at it. */
  id: string;
  /** The label's id, for a control a `<label for>` cannot name — a radio group. */
  labelledBy: string;
  /** The help line's id. */
  describedBy: string;
};

/** A row's way back to the value beneath it. */
export type Reset = { label: string; disabled: boolean; onReset: () => void };

/**
 * **One setting**: its label, its control, its one line of help, and a reset when the value can
 * go back. The control is drawn by `control`, handed the ids that tie it to the label and the
 * help — a {@link Field} or a {@link Choice}.
 *
 * Which file a value goes to, and where its current value comes from, join the row with SE-18.
 */
export function SettingRow({
  label,
  help,
  reset,
  control,
}: {
  label: string;
  help?: string;
  reset?: Reset;
  control: (ids: RowIds) => ReactNode;
}) {
  const id = useId();
  const labelledBy = useId();
  const describedBy = useId();
  return (
    <div className="ui-setting-row">
      <label className="ui-setting-label" id={labelledBy} htmlFor={id}>
        {label}
      </label>
      <div className="ui-setting-control">
        {control({ id, labelledBy, describedBy: help ? describedBy : "" })}
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
        <p className="ui-setting-help" id={describedBy}>
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
    | { kind: "text"; value: string; onChange: (to: string) => void }
    /** One entry per line. The text is what is typed, kept whole — an empty line while the next
     *  entry is being typed is still there — and the caller reads the entries out of it. */
    | { kind: "list"; value: string; onChange: (to: string) => void }
    | {
        kind: "range";
        value: number;
        min: number;
        max: number;
        step?: number;
        /** What the value reads as, beside the slider and to a screen reader: `18px`. */
        shown: (value: number) => string;
        onChange: (to: number) => void;
      }
  );

export function Field(props: FieldProps) {
  const { ids } = props;
  const described = ids.describedBy || undefined;
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
          aria-valuetext={props.shown(props.value)}
          aria-describedby={described}
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
        rows={Math.max(2, props.value.split("\n").length)}
        aria-describedby={described}
        onChange={(event) => props.onChange(event.currentTarget.value)}
      />
    );
  return (
    <input
      id={ids.id}
      className="ui-field"
      type="text"
      value={props.value}
      spellCheck={false}
      aria-describedby={described}
      onChange={(event) => props.onChange(event.currentTarget.value)}
    />
  );
}

/** One option of a choice: its value, what it is called, and a line on what it does. */
export type Option = { value: string; label: string; says?: string };

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

export function Choice(props: ChoiceProps) {
  const { ids } = props;
  const described = ids.describedBy || undefined;
  if (props.kind === "toggle")
    return (
      <Checkbox.Root
        id={ids.id}
        className="box"
        checked={props.checked}
        aria-describedby={described}
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
        aria-describedby={described}
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
      aria-labelledby={ids.labelledBy}
      aria-describedby={described}
    >
      {props.options.map((one) => (
        <div className="ui-choice-option" key={one.value}>
          <RadioGroup.Item
            className="dot"
            value={one.value}
            id={`${ids.id}-${one.value}`}
            aria-describedby={one.says ? `${ids.id}-${one.value}-says` : undefined}
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
