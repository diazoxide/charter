import { useState } from "react";

/**
 * Whether `value` was **arrived at** while this component was on screen, rather than being
 * what it was already showing when it mounted.
 *
 * **Why the motion layer needs this.** A mark that animates when it changes has to tell a
 * change from a mount, and CSS alone cannot: an animation plays when its element appears, so
 * a "settle" keyed to a finished pipeline would also play on every one of them at launch, and
 * a "waiting on you" pulse would play on every waiting chat each time the workspace strip
 * brought a workspace's tabs back into view. That is motion on every render, which is the one
 * thing motion here may not be (`docs/design-system.md`). So the component says, and the
 * stylesheet only draws what it is told.
 *
 * It stays true once it is true: after the first change, every later value was also arrived at.
 * A mark that goes `running → waiting → running` has arrived at each, and the stylesheet keys
 * each state to its own animation, so each arrival plays its own.
 *
 * Written with React's "adjusting state when a prop changes" pattern rather than a ref read in
 * render, which is what React 19's rules — and `eslint-plugin-react-hooks` — ask for.
 */
export function useArrived<T>(value: T): boolean {
  const [held, setHeld] = useState(value);
  const [moved, setMoved] = useState(false);
  if (!Object.is(held, value)) {
    setHeld(value);
    setMoved(true);
    return true;
  }
  return moved;
}
