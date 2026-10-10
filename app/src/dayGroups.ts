/**
 * The day headings the right side's lists are grouped under (#1674, B-9): **Today**,
 * **Yesterday** and **Earlier**, the way an editor's timeline says recency without a column of
 * dates.
 *
 * **Told by the row's note, and only when the notes are dates** ({@link isDated}). A memory's, a todo's and
 * a session record's note begins with the stamp the store wrote (`YYYY-MM-DD HH:MM`, local time,
 * `memstore`); a persona's says how much it remembers. A list with one note that is not a date
 * is not a list of dated things, and gets no headings: a heading over a persona would be a guess.
 */

/** The three days, in the order they are drawn. */
export const DAYS = ["Today", "Yesterday", "Earlier"] as const;
export type Day = (typeof DAYS)[number];

const STAMP = /^(\d{4})-(\d{2})-(\d{2})(?:\D|$)/;

/** `YYYY-MM-DD` of `when`, in local time, which is the time a stamp is written in. */
function dayOf(when: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${when.getFullYear()}-${two(when.getMonth() + 1)}-${two(when.getDate())}`;
}

/** The date a note begins with, as `YYYY-MM-DD`, or `undefined` when it begins with none. */
export function stampOf(note: string | null): string | undefined {
  const found = note === null ? null : STAMP.exec(note);
  return found === null ? undefined : `${found[1]}-${found[2]}-${found[3]}`;
}

/**
 * Whether a list is one of dated things: at least one note is a date, and every note there is
 * begins with one. A row with no note at all (a memory written by hand, with no stamp line) does
 * not stop a list being dated; it is drawn under Earlier.
 */
export function isDated<T>(items: readonly T[], noteOf: (item: T) => string | null): boolean {
  const notes = items.map(noteOf);
  return (
    notes.some((note) => stampOf(note) !== undefined) &&
    notes.every((note) => note === null || stampOf(note) !== undefined)
  );
}

/**
 * `items` under their days, the days in {@link DAYS} order and each day's items in the order
 * they came. A day with no items is left out. A stamp after today (a clock that moved) reads as
 * today, and an item with no stamp as Earlier.
 */
export function byDay<T>(
  items: readonly T[],
  noteOf: (item: T) => string | null,
  now: Date = new Date(),
): { day: Day; items: T[] }[] {
  const today = dayOf(now);
  const dayBefore = new Date(now);
  dayBefore.setDate(dayBefore.getDate() - 1);
  const yesterday = dayOf(dayBefore);
  const groups = new Map<Day, T[]>(DAYS.map((day) => [day, []]));
  for (const item of items) {
    const stamp = stampOf(noteOf(item));
    const day: Day =
      stamp === undefined
        ? "Earlier"
        : stamp >= today
          ? "Today"
          : stamp === yesterday
            ? "Yesterday"
            : "Earlier";
    groups.get(day)?.push(item);
  }
  return DAYS.map((day) => ({ day, items: groups.get(day) ?? [] })).filter(
    (group) => group.items.length > 0,
  );
}
