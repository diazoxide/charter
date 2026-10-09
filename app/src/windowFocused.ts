import { useEffect, useState } from "react";

/**
 * **How many times the window has come back into focus** since the caller mounted: a
 * dependency for a read that should follow what no watcher reports, such as a file another
 * window or a hand wrote, or a pull made while the window was away (#1407). Starts at 0 and
 * moves by one on each `focus` of the window.
 */
export function useWindowFocused(): number {
  const [focused, setFocused] = useState(0);
  useEffect(() => {
    const again = () => setFocused((n) => n + 1);
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, []);
  return focused;
}
