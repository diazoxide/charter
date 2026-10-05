import { useId, useRef, type ReactNode } from "react";
import * as Checkbox from "@radix-ui/react-checkbox";
import * as RadioGroup from "@radix-ui/react-radio-group";
import * as RovingFocusGroup from "@radix-ui/react-roving-focus";
import { ListFilter } from "lucide-react";
import { useTabStop } from "../roving";

/**
 * **The house settings set** (SE-16, #1166; DS-3 #626; V89f, V89j): the six pieces every
 * settings screen and every form is drawn from — {@link SettingsLayout}, {@link SettingGroup},
 * {@link SettingRow}, {@link Field}, {@link Choice} and {@link SettingActions}.
 *
 * `docs/ui-primitives.md` forbids a house component library and allows exactly this set, for the
 * reasons written there: charter had two hand-built form styles that had already drifted
 * (`settings-*` and the dialogs' `choices`), and one small set is how a new setting stays a few
 * lines. Each piece is a thin layer over a native element or a Radix primitive already in the
 * window — radio group, checkbox, roving focus — and **says which at its call site**: the props
 * are the primitive's words (`value`, `onValueChange`, `checked`), not new ones.
 *
 * Drawn in `App.css` under `ui-*` classes, every colour a design-system token. The old
 * hand-built `settings-*`, `asks`, `choices`, `choice`, `who` and `picking` classes were deleted
 * once the last screen had moved (DS-3e, #1177), and `oldFormClasses.test.ts` keeps them gone.
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
 *
 * **The filter** (SE-21, V89c) sits above the nav: a search box, labelled, whose words the
 * caller narrows its groups by — the layout draws what it is handed. While it holds words, a
 * polite live region under it says how many settings match (`found`), and when none do, the
 * right column says so where the group would be — above what the level's files refuse, which
 * stays on screen whatever the filter. Escape in the box clears it.
 *
 * **The foot of the nav** (`foot`, SE-19, V89d) is the caller's: the Project level's "Edit as
 * TOML" link per file. It is under the groups and not one of them, so the filter leaves it be.
 */
export function SettingsLayout({
  levels,
  level,
  onLevelChange,
  about,
  groups,
  group,
  onGroupChange,
  filter,
  onFilterChange,
  found,
  foot,
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
  /** The words in the filter box, as typed. */
  filter: string;
  onFilterChange: (to: string) => void;
  /** How many settings the filter leaves, while it holds words; unset while the level is still
   *  being read, so nothing is said yet. */
  found?: number;
  /** What sits under the nav, whatever the filter. */
  foot?: ReactNode;
  /** The chosen group: a {@link SettingGroup}. */
  children: ReactNode;
}) {
  const stop = useTabStop(
    group,
    groups.map((one) => one.id),
  );
  const words = filter.trim();
  const none =
    words !== "" && found === 0 ? `No setting at this level matches “${words}”.` : undefined;
  const said =
    words === "" || found === undefined
      ? ""
      : (none ?? `${found} ${found === 1 ? "setting matches" : "settings match"}`);
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
      <div className="ui-settings-side">
        <div className="ui-settings-filter">
          <ListFilter className="node-icon" aria-hidden />
          <input
            type="search"
            value={filter}
            aria-label="Filter settings"
            placeholder="Filter settings"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => onFilterChange(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key !== "Escape" || filter === "") return;
              // Taken here: an Escape that cleared the box has done its job, and nothing
              // behind the tab should act on it too.
              event.preventDefault();
              event.stopPropagation();
              onFilterChange("");
            }}
          />
        </div>
        <p className="sr-only" role="status" aria-live="polite" aria-label="Settings found">
          {said}
        </p>
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
        {foot}
      </div>
      <div className="ui-settings-body">
        {none && <p className="ui-settings-none">{none}</p>}
        {children}
      </div>
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
 * **A write the core refused is said here** (`error`, SE-17, V89e): beside the setting, as an
 * alert, and in the control's description, while the control shows what is on disk. And the
 * last change's **Undo** sits in the row it was made in.
 *
 * **Where the value comes from** (SE-18, V89d): `origin` says which level and which file — part
 * of the control's description — and `badge` marks a value that overrides another file's (a
 * Local value over a Shared one). `place` is the row's file choice, drawn under the control.
 */
export function SettingRow({
  label,
  help,
  reset,
  grouped = false,
  error,
  undo,
  origin,
  badge,
  place,
  control,
}: {
  label: string;
  /** One line on what the value does. A node and not only a string, because a dialog's help
   *  names files and commands, and those are `<code>` (DS-3c). */
  help?: ReactNode;
  reset?: Reset;
  grouped?: boolean;
  /** Why the last write of this setting was refused, in the core's words. */
  error?: readonly string[];
  /** Puts back what this setting was before the last change, when that change was here. */
  undo?: () => void;
  /** Which level and which file the value shown comes from, or that none here holds it. */
  origin?: string;
  /** What the value overrides, as a badge beside its name: "Overrides charter.toml". */
  badge?: string;
  /** Which file the value is kept in, as a choice (SE-18). */
  place?: ReactNode;
  control: (ids: RowIds) => ReactNode;
}) {
  const id = useId();
  const labelledBy = useId();
  const described = useId();
  const from = useId();
  const refused = useId();
  const failed = error !== undefined && error.length > 0;
  const describedBy =
    [help ? described : undefined, origin ? from : undefined, failed ? refused : undefined]
      .filter(Boolean)
      .join(" ") || undefined;
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
      {badge && <span className="ui-setting-badge">{badge}</span>}
      <div className="ui-setting-control">
        {control({ id, labelledBy, describedBy })}
        {reset && (
          <button
            type="button"
            className="ui-setting-reset"
            // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
            tabIndex={0}
            disabled={reset.disabled}
            onClick={() => {
              reset.onReset();
              // The button goes once the value is gone; the focus goes back to the control.
              document.getElementById(id)?.focus();
            }}
          >
            {reset.label}
          </button>
        )}
        {undo && (
          <button
            type="button"
            className="ui-setting-reset"
            // #190: WebKit leaves a button out of the tab sequence without `tabIndex`.
            tabIndex={0}
            onClick={() => {
              undo();
              // The button goes once it is pressed; the focus goes back to what it undid.
              document.getElementById(id)?.focus();
            }}
          >
            Undo
          </button>
        )}
      </div>
      {help && (
        <p className="ui-setting-help" id={described}>
          {help}
        </p>
      )}
      {origin && (
        <p className="ui-setting-origin" id={from}>
          {origin}
        </p>
      )}
      {place}
      {failed && (
        <div className="ui-setting-error" id={refused} role="alert">
          {error.map((why, at) => (
            <p key={at}>{why}</p>
          ))}
        </div>
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
 *
 * A typed value is **committed** when the field is left, and a `text` one on Enter too
 * (`onCommit`): a value written as it is changed (V89e) is written once it is typed, not at
 * every key — half a branch name is not a value to write, or to refuse.
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
        onCommit?: () => void;
      }
    /** One entry per line. The text is what is typed, kept whole — an empty line while the next
     *  entry is being typed is still there — and the caller reads the entries out of it. */
    | {
        kind: "list";
        value: string;
        onChange: (to: string) => void;
        /** The fewest lines the box shows, before it grows with what is typed. Two unless said. */
        minRows?: number;
        onCommit?: () => void;
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
        onBlur={props.onCommit}
      />
    );
  const { onCommit } = props;
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
      onBlur={onCommit}
      onKeyDown={(event) => {
        if (event.key === "Enter" && onCommit) {
          event.preventDefault();
          onCommit();
        }
      }}
    />
  );
}

/** One option of a choice: its value, what it is called, and a line on what it does. The line
 *  is words, or a few marked-up facts (a harness's kind, command and source) that describe the
 *  option as one line. A radio's or a box's option can be out of reach on its own (`disabled`),
 *  with why on its `title` — the primitive's own words (DS-3e). */
export type Option = {
  value: string;
  label: string;
  says?: ReactNode;
  disabled?: boolean;
  title?: string;
};

/**
 * **A value picked**: `radio` one of a few, each with a line on what it does (a Radix radio
 * group); `select` one of many (the native `<select>`); `toggle` on or off (a Radix checkbox);
 * `checks` any of a few, each ticked on its own (a Radix checkbox per option, in a group the row
 * names — the repos a workspace takes, the files offered to memory; DS-3e).
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
    | {
        kind: "toggle";
        checked: boolean;
        onCheckedChange: (to: boolean) => void;
        /** The box's own `disabled`: held while what it feeds is being done. */
        disabled?: boolean;
      }
    | {
        kind: "checks";
        options: readonly Option[];
        /** The values ticked. */
        checked: ReadonlySet<string>;
        onCheckedChange: (value: string, on: boolean) => void;
      }
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
        disabled={props.disabled}
        aria-describedby={ids.describedBy}
        onCheckedChange={(to) => props.onCheckedChange(to === true)}
      >
        <Checkbox.Indicator>✓</Checkbox.Indicator>
      </Checkbox.Root>
    );
  if (props.kind === "checks")
    return (
      <div
        id={ids.id}
        role="group"
        className="ui-choice-checks"
        aria-labelledby={ids.labelledBy}
        aria-describedby={ids.describedBy}
      >
        {props.options.map((one, at) => (
          // By place, not by value: two options can share one (two owners' `api`, D-DS3e-10).
          <div className="ui-choice-option" key={at}>
            <Checkbox.Root
              id={`${ids.id}-${at}`}
              className="box"
              // #186: WebKit leaves a `<button>` out of the Tab order without `tabIndex`.
              tabIndex={0}
              checked={props.checked.has(one.value)}
              disabled={one.disabled}
              title={one.title}
              aria-describedby={one.says ? `${ids.id}-${at}-says` : undefined}
              onCheckedChange={(to) => props.onCheckedChange(one.value, to === true)}
            >
              <Checkbox.Indicator>✓</Checkbox.Indicator>
            </Checkbox.Root>
            <label htmlFor={`${ids.id}-${at}`}>{one.label}</label>
            {one.says && (
              <div className="ui-choice-says" id={`${ids.id}-${at}-says`}>
                {one.says}
              </div>
            )}
          </div>
        ))}
      </div>
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
      {props.options.map((one, at) => (
        // By place, not by value: two options can share one (two owners' `api`, D-DS3e-10).
        <div className="ui-choice-option" key={at}>
          <RadioGroup.Item
            className="dot"
            value={one.value}
            id={`${ids.id}-${at}`}
            disabled={one.disabled}
            title={one.title}
            aria-describedby={one.says ? `${ids.id}-${at}-says` : undefined}
            onFocus={() => {
              if (arrowing.current && !props.disabled && !one.disabled && one.value !== props.value)
                props.onValueChange(one.value);
            }}
          >
            <RadioGroup.Indicator className="dot-mark" />
          </RadioGroup.Item>
          <label htmlFor={`${ids.id}-${at}`}>{one.label}</label>
          {one.says && (
            <span className="ui-choice-says" id={`${ids.id}-${at}-says`}>
              {one.says}
            </span>
          )}
        </div>
      ))}
    </RadioGroup.Root>
  );
}

/**
 * **A form's buttons** (V89j, DS-3e #1177): the row a form ends in — Save and Cancel, Use this,
 * Move, Restore, Retry — under its rows, or a dialog's answers under its form.
 *
 * A row and nothing more: the buttons are native `<button>`s the caller writes, with their own
 * `type`, `disabled` and `onClick`, so the call site still says what each one is. The row draws
 * them all alike, from the same tokens as a row's reset; a button that destroys something says
 * so with `className="ends-it"`, as the dialogs' answers do.
 *
 * Every button in it still needs `tabIndex={0}`: WebKit leaves a `<button>` out of the Tab order
 * without one (#190), and the row cannot add it to buttons it does not draw.
 */
export function SettingActions({ children }: { children: ReactNode }) {
  return <div className="ui-setting-actions">{children}</div>;
}
