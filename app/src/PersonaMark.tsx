import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type CSSProperties,
} from "react";
import {
  Anchor,
  BookOpen,
  Bot,
  Briefcase,
  Bug,
  ChartLine,
  ClipboardList,
  Cloud,
  Code,
  Compass,
  Cpu,
  Database,
  Eye,
  FlaskConical,
  GitBranch,
  Globe,
  GraduationCap,
  Hammer,
  Heart,
  KeyRound,
  Leaf,
  Lightbulb,
  Lock,
  Mail,
  Map as MapGlyph,
  Megaphone,
  Package,
  Palette,
  PenLine,
  Rocket,
  Scale,
  Search,
  Server,
  Shield,
  Star,
  Terminal,
  User,
  Users,
  Wrench,
  Zap,
} from "lucide-react";
import { commands, type PersonaImage, type PersonaMark as Mark, type PlaneId } from "./bindings";
import { inForce, onDrawn, tintVariables, type Token } from "./theme/theme";
import { PALETTE } from "./theme/tint";

/**
 * **A persona's mark** (#1449): the one thing that draws a persona, wherever one appears — a
 * chat's tab, its row in the explorer, the Personas panel, the persona's own view, a needs-you
 * item, a Notice and the new-chat picker.
 *
 * What it draws, first that applies:
 *
 * 1. **the custom image** in the persona's folder (`icon.png`);
 * 2. **the built-in icon** its definition names (`icon:`), one of {@link PERSONA_ICONS};
 * 3. **its initials**.
 *
 * Each on the persona's colour (`color:`, a workspace's vocabulary), or on one derived from its
 * name, so two personas with nothing declared are still told apart.
 *
 * # A custom image is a PNG, never markup and never a URL
 *
 * A chat can edit its own persona's folder, so the image is whatever a chat wrote. The core
 * hands over a PNG and nothing else (`purlis_core::personamark`), as bytes. They are decoded
 * onto a canvas here, as a file tab's image preview is (`PieceFiles`): nothing of the file is
 * put in the page, and the window's content security policy gives images no source to fetch
 * from. A type that is not a PNG is never decoded, whoever sent it. A window that cannot decode
 * the bytes draws the initials, and the persona's view says so. An `icon.svg` is not drawn at
 * all: the core never reads one, and the persona's view says to save a PNG.
 *
 * # How a caller uses it
 *
 * `<PersonaMark persona="devops" />` anywhere under a {@link PersonaMarks} provider, which a
 * project view supplies from {@link usePersonaMarks}. A surface outside every project (the
 * title bar's needs-you list) passes the `mark` it was handed. With neither, the mark is the
 * initials: a complete answer, never a gap.
 */

/** One glyph. Lucide's, so it is drawn in the colour of the text it sits in. */
type Glyph = React.ComponentType<{ className?: string }>;

/**
 * The icons a persona may name, as the glyphs the window ships for them
 * (`purlis_core::personamark::ICONS` names the same forty, and a test holds the two lists to
 * each other). A closed set: a name can never become a read of a file.
 */
export const PERSONA_ICONS: Readonly<Record<string, Glyph>> = {
  anchor: Anchor,
  book: BookOpen,
  bot: Bot,
  briefcase: Briefcase,
  bug: Bug,
  chart: ChartLine,
  clipboard: ClipboardList,
  cloud: Cloud,
  code: Code,
  compass: Compass,
  cpu: Cpu,
  database: Database,
  eye: Eye,
  flask: FlaskConical,
  "git-branch": GitBranch,
  globe: Globe,
  "graduation-cap": GraduationCap,
  hammer: Hammer,
  heart: Heart,
  key: KeyRound,
  leaf: Leaf,
  lightbulb: Lightbulb,
  lock: Lock,
  mail: Mail,
  map: MapGlyph,
  megaphone: Megaphone,
  package: Package,
  palette: Palette,
  pen: PenLine,
  rocket: Rocket,
  scale: Scale,
  search: Search,
  server: Server,
  shield: Shield,
  star: Star,
  terminal: Terminal,
  user: User,
  users: Users,
  wrench: Wrench,
  zap: Zap,
};

/** What a mark is, without the persona's name: what a caller outside a project hands over. */
export type PersonaMarkData = Pick<Mark, "icon" | "colour" | "image">;

/** Every persona's mark in the project a surface is drawn in, by name. */
export const PersonaMarks = createContext<ReadonlyMap<string, Mark>>(new Map());

/** Reads that project's marks again: what the picker calls once a pick is written. */
export const ReloadPersonaMarks = createContext<() => void>(() => undefined);

/**
 * The marks of the plane's personas, read at once and again whenever `changed` moves (a
 * project view passes its count of changes on disk, so an `icon.png` a chat writes is drawn
 * without a restart). `reload` reads them again after a pick here.
 *
 * A read that fails leaves what was drawn: a mark is decoration, and every persona still has
 * its initials.
 */
export function usePersonaMarks(
  plane: PlaneId,
  changed: number,
): { marks: ReadonlyMap<string, Mark>; reload: () => void } {
  const [marks, setMarks] = useState<ReadonlyMap<string, Mark>>(() => new Map());
  const [asked, setAsked] = useState(0);
  useEffect(() => {
    let gone = false;
    void commands
      .personaMarks(plane)
      .then((said) => {
        if (gone || said.status !== "ok" || !Array.isArray(said.data)) return;
        setMarks(new Map(said.data.map((one) => [one.name, one])));
      })
      .catch(() => undefined);
    return () => {
      gone = true;
    };
  }, [plane, changed, asked]);
  const reload = useCallback(() => setAsked((was) => was + 1), []);
  return { marks, reload };
}

/**
 * A persona's initials: the first letters of its first two words (`docs-writer` is DW), or
 * the first two letters of a name that is one word (`steward` is ST).
 */
export function initialsOf(persona: string): string {
  const words = persona.split(/[^\p{L}\p{N}]+/u).filter(Boolean);
  const letters =
    words.length > 1
      ? [...words[0]][0] + [...words[1]][0]
      : [...(words[0] ?? "")].slice(0, 2).join("");
  return letters.toUpperCase();
}

/** The palette's names, in the order the picker offers them. */
export const PERSONA_COLOURS: readonly string[] = Object.keys(PALETTE);

/**
 * The colour a persona that declares none is drawn on: one of the palette's, picked by its
 * name, so it is the same on every machine and in every window.
 */
export function colourOfName(persona: string): string {
  // FNV-1a over the name's code units: small, stable, and spread enough for eight colours.
  let hash = 0x811c9dc5;
  for (let at = 0; at < persona.length; at++) {
    hash ^= persona.charCodeAt(at);
    hash = Math.imul(hash, 0x01000193);
  }
  return PERSONA_COLOURS[(hash >>> 0) % PERSONA_COLOURS.length];
}

/** What a persona's colour is drawn on: the mark's ground and what stands on it. */
export const MARK_TOKENS: readonly Token[] = ["accent.base", "accent.surface"];

/** The image type the core hands over, and so the only one decoded. */
const IMAGE_TYPES: ReadonlySet<string> = new Set(["image/png"]);

/** How many pixels a side an image is decoded to: a mark is a line of text tall, and this is
 *  that at twice the largest size the window draws one, for a dense screen. */
const IMAGE_SIDE = 64;

/** Every image decoded, by its bytes: fifty tabs of one persona decode it once. */
const decoded = new Map<string, Promise<ImageBitmap>>();
/** The most decodings held. Past it the oldest go: a persona's image changing leaves one behind. */
const MOST_DECODED = 64;

/** `image` as a bitmap, decoded from its bytes. Loads nothing: there is no URL in this. */
function bitmapOf(image: PersonaImage): Promise<ImageBitmap> {
  const key = `${image.mime}\u0000${image.base64}`;
  const known = decoded.get(key);
  if (known !== undefined) return known;
  const made = (async () => {
    if (!IMAGE_TYPES.has(image.mime)) throw new Error(`${image.mime} is not an image it draws`);
    const bytes = Uint8Array.from(atob(image.base64), (c) => c.charCodeAt(0));
    return createImageBitmap(new Blob([bytes], { type: image.mime }), {
      resizeWidth: IMAGE_SIDE,
      resizeHeight: IMAGE_SIDE,
      resizeQuality: "high",
    });
  })();
  decoded.set(key, made);
  // A decoding that failed is not held: the next draw asks again, and says so again.
  made.catch(() => decoded.delete(key));
  if (decoded.size > MOST_DECODED) decoded.delete(decoded.keys().next().value as string);
  return made;
}

/** What the persona's view says when the window could not decode a custom image. */
export const NOT_DRAWN =
  "This window could not draw the custom image, so purlis shows the initials instead.";

/** The custom image, on a canvas. Tells `onTrouble` when it cannot be drawn. */
function Drawn({ image, onTrouble }: { image: PersonaImage; onTrouble: () => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let gone = false;
    void bitmapOf(image)
      .then((bitmap) => {
        const to = canvas.current;
        if (gone || to === null) return;
        to.width = IMAGE_SIDE;
        to.height = IMAGE_SIDE;
        to.getContext("2d")?.drawImage(bitmap, 0, 0, IMAGE_SIDE, IMAGE_SIDE);
      })
      .catch(() => {
        if (!gone) onTrouble();
      });
    return () => {
      gone = true;
    };
  }, [image, onTrouble]);
  return <canvas ref={canvas} />;
}

/** The theme the window draws, which a mark's colour is a hue shift of. */
export const followTheme = (follow: () => void) => onDrawn(follow);

/**
 * One persona's mark.
 *
 * Hidden from a screen reader and named in a tooltip: every place that draws it says the
 * persona's name in words beside it or in its own accessible name, and a mark that added the
 * name again would have a tab read "devops devops 1".
 */
export function PersonaMark({
  persona,
  mark,
  className,
  onImageTrouble,
}: {
  persona: string;
  /** Its mark, where the caller holds it. Left out, it is looked up in {@link PersonaMarks}. */
  mark?: PersonaMarkData | null;
  className?: string;
  /** Told when a custom image could not be decoded here, so the persona's view can say so. */
  onImageTrouble?: () => void;
}) {
  const known = useContext(PersonaMarks).get(persona);
  const given = mark ?? known;
  const theme = useSyncExternalStore(followTheme, inForce);
  const image = given?.image ?? null;
  // Which image failed, so a new one is tried: a refusal is about those bytes, not the persona.
  const [failed, setFailed] = useState<PersonaImage>();
  const failedNow = useCallback(() => {
    setFailed(image ?? undefined);
    onImageTrouble?.();
  }, [image, onImageTrouble]);
  const Icon =
    given?.icon != null && Object.prototype.hasOwnProperty.call(PERSONA_ICONS, given.icon)
      ? PERSONA_ICONS[given.icon]
      : undefined;
  // An image that cannot be drawn is the initials, never the icon the persona also named: that
  // is what the core does with one it refuses, and the persona's view says which happened.
  const shows =
    image !== null ? (image === failed ? "initials" : "image") : Icon ? "icon" : "initials";
  const colour = given?.colour ?? colourOfName(persona);
  return (
    <span
      className={className ? `persona-mark ${className}` : "persona-mark"}
      data-persona={persona}
      data-mark={shows}
      data-icon={shows === "icon" ? (given?.icon ?? undefined) : undefined}
      // Drawn by the stylesheet from this attribute and never as text of the page: a tab's
      // words are its name, and two letters in front of them would be read as part of it.
      data-initials={shows === "initials" ? initialsOf(persona) : undefined}
      data-colour={colour}
      style={tintVariables(theme, colour, MARK_TOKENS) as CSSProperties}
      title={persona}
      aria-hidden="true"
    >
      {shows === "image" && image !== null ? (
        <Drawn image={image} onTrouble={failedNow} />
      ) : Icon && shows === "icon" ? (
        <Icon />
      ) : null}
    </span>
  );
}
