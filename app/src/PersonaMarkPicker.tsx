import {
  useCallback,
  useContext,
  useMemo,
  useState,
  useSyncExternalStore,
  type CSSProperties,
} from "react";
import * as RadioGroup from "@radix-ui/react-radio-group";
import { commands, type PersonaImage, type PlaneId } from "./bindings";
import { Notice } from "./Notice";
import {
  MARK_TOKENS,
  NOT_DRAWN,
  PERSONA_COLOURS,
  PERSONA_ICONS,
  PersonaMark,
  PersonaMarks,
  ReloadPersonaMarks,
  followTheme,
  initialsOf,
} from "./PersonaMark";
import { inForce, tintVariables } from "./theme/theme";

/**
 * **Pick a persona's icon and colour**, in its view. A pick is written to the persona's
 * definition at once (`icon:` and `color:`), so it is on every surface and every teammate's
 * machine after a save, and there is no second step to forget.
 *
 * Two radio groups (Radix, ADR 0037). The first choice of each takes the key out of the
 * definition: the persona goes back to its initials, or to the colour of its name. Where its
 * folder holds a custom image, that is what is drawn, and the icon picked here is what it
 * falls back to.
 *
 * What the persona asked for and cannot have is said above the groups, each as a Notice.
 */
export function PersonaMarkPicker({ plane, persona }: { plane: PlaneId; persona: string }) {
  const mark = useContext(PersonaMarks).get(persona);
  const reload = useContext(ReloadPersonaMarks);
  const [refused, setRefused] = useState<string>();
  // Which image this window could not decode: a new one is tried, and may draw.
  const [undrawn, setUndrawn] = useState<PersonaImage | null>(null);
  const image = mark?.image ?? null;
  const notDrawn = image !== null && undrawn === image;
  const [dismissed, setDismissed] = useState<ReadonlySet<string>>(new Set());
  const imageTrouble = useCallback(() => setUndrawn(image), [image]);
  const trouble = useMemo(
    () => [
      ...(mark?.trouble ?? []),
      ...(notDrawn ? [NOT_DRAWN] : []),
      ...(refused ? [refused] : []),
    ],
    [mark, notDrawn, refused],
  );
  const pick = (icon: string | null, colour: string | null) => {
    setRefused(undefined);
    void commands
      .personaMarkSet(plane, persona, icon, colour)
      .then((said) => {
        if (said.status === "error") setRefused(said.error);
        else reload();
      })
      .catch((err: unknown) =>
        setRefused(`purlis could not write ${persona}'s icon and colour: ${String(err)}`),
      );
  };
  // A persona the plane does not have (any more) has no definition to write a pick into.
  if (mark === undefined) return null;
  const icon = mark.icon;
  const colour = mark.colour;
  const custom = colour !== null && !PERSONA_COLOURS.includes(colour);
  return (
    <section className="persona-mark-picker" aria-label="Icon and colour">
      <div className="persona-mark-shown">
        <PersonaMark persona={persona} onImageTrouble={imageTrouble} />
        {image !== null && !notDrawn && (
          <p>
            The custom image in its folder is drawn. The icon picked here is what it falls back to.
          </p>
        )}
      </div>
      {trouble
        .filter((why) => !dismissed.has(why))
        .map((why) => (
          <Notice
            key={why}
            cause={`persona-mark:${persona}`}
            tone="trouble"
            at="pane"
            onDismiss={() => setDismissed((was) => new Set(was).add(why))}
          >
            {why}
          </Notice>
        ))}
      <RadioGroup.Root
        className="persona-mark-choices"
        aria-label="Icon"
        value={icon ?? ""}
        onValueChange={(to) => pick(to === "" ? null : to, colour)}
      >
        <RadioGroup.Item
          className="persona-mark-choice"
          value=""
          aria-label="Initials"
          title="Initials"
        >
          {initialsOf(persona)}
        </RadioGroup.Item>
        {Object.entries(PERSONA_ICONS).map(([name, Icon]) => (
          <RadioGroup.Item
            key={name}
            className="persona-mark-choice"
            value={name}
            aria-label={name}
            title={name}
          >
            <Icon aria-hidden="true" />
          </RadioGroup.Item>
        ))}
      </RadioGroup.Root>
      <RadioGroup.Root
        className="persona-mark-choices"
        aria-label="Colour"
        value={custom ? "custom" : (colour ?? "")}
        onValueChange={(to) => {
          if (to !== "custom") pick(icon, to === "" ? null : to);
        }}
      >
        <RadioGroup.Item
          className="persona-mark-choice persona-mark-swatch"
          value=""
          aria-label="From its name"
          title="From its name"
        >
          <PersonaMark persona={persona} mark={{ icon: null, colour: null, image: null }} />
        </RadioGroup.Item>
        {PERSONA_COLOURS.map((name) => (
          <RadioGroup.Item
            key={name}
            className="persona-mark-choice persona-mark-swatch"
            value={name}
            aria-label={name}
            title={name}
          >
            <Swatch colour={name} />
          </RadioGroup.Item>
        ))}
        {/* A `#rrggbb` written by hand in the definition: shown as what is picked, and kept
            until another is. The window offers the palette; the file takes any hue. */}
        {custom && colour !== null && (
          <RadioGroup.Item
            className="persona-mark-choice persona-mark-swatch"
            value="custom"
            aria-label={colour}
            title={colour}
          >
            <Swatch colour={colour} />
          </RadioGroup.Item>
        )}
      </RadioGroup.Root>
    </section>
  );
}

/** One colour of the picker, drawn as a mark's ground is. */
function Swatch({ colour }: { colour: string }) {
  const theme = useSyncExternalStore(followTheme, inForce);
  return (
    <span
      className="persona-mark persona-mark-blank"
      data-colour={colour}
      style={tintVariables(theme, colour, MARK_TOKENS) as CSSProperties}
      aria-hidden="true"
    />
  );
}
