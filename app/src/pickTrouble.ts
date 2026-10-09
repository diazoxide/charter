import { useCallback, useState } from "react";

/**
 * The one refusal line a folder dialog's failure shares with its surface's own `trouble`
 * (#1291), and which of the two it says: **the newer one**.
 *
 * The Opener, the first run and New project each say why the folder dialog could not open on
 * the line where they say a refused open. The surface's `trouble` comes from its window and can
 * change without the dialog: a recent row's open, a forge answer, a sign-in. A dialog failure
 * that simply outranked it would leave an old "could not open" line in place of a newer refusal.
 * So the failure is kept with the `trouble` it was said over, and gives way as soon as that
 * changes; `started` drops it when any open starts on the surface, which covers a refusal whose
 * words happen to be the same as the last one's.
 */
export function useNewerTrouble(trouble: string | undefined) {
  const [failed, setFailed] = useState<{ said: string; over: string | undefined }>();
  const pickFailed = useCallback((said: string) => setFailed({ said, over: trouble }), [trouble]);
  const started = useCallback(() => setFailed(undefined), []);
  const said = failed !== undefined && failed.over === trouble ? failed.said : trouble;
  return { said, pickFailed, started };
}
