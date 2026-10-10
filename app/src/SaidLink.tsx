import type { SettingsWay } from "./actions";
import type { SettingsLink } from "./settings/links";

/**
 * **A refusal's way to the setting that puts it right** (#1201, SE-22), drawn beside the words
 * of the last action — on the line under the strip and on the palette's line, which are one
 * state drawn in two places. A link, as a Notice's is: it goes to where the problem is fixed,
 * and it changes nothing on its own.
 */
export function SaidLink({
  way,
  onFollow,
}: {
  way: SettingsWay;
  onFollow: (link: SettingsLink) => void;
}) {
  return (
    <>
      {" "}
      <button type="button" className="said-link" tabIndex={0} onClick={() => onFollow(way.link)}>
        {way.label}
      </button>
    </>
  );
}
