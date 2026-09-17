import { useCallback, useEffect, useRef } from "react";
import { Channel } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import * as bench from "./bench";
import { commands } from "./bindings";
import { draw } from "./renderer";

/**
 * One pane, drawing one session.
 *
 * A terminal here exists only while the pane is on screen. It opens a view of the session,
 * which is sent the screen the session already has and then its output, so a pane can come and
 * go without the session noticing — the core has held its terminal all along.
 */
export function SessionPane({
  session,
  focused,
  onFocus,
}: {
  session: number;
  focused: boolean;
  onFocus: () => void;
}) {
  const holder = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | undefined>(undefined);

  /** Clicking a pane puts the keyboard in it, which is the whole point of clicking it. Both
   *  events are handled: a person's press arrives as `mousedown`, and something driving the
   *  window from outside — a scenario test — may only send `click`. */
  const take = useCallback(() => {
    terminal.current?.focus();
    onFocus();
  }, [onFocus]);

  // A pane that becomes the focused one — by a split, or by its tab coming back — takes the
  // keyboard without being clicked.
  useEffect(() => {
    if (focused) terminal.current?.focus();
  }, [focused]);

  useEffect(() => {
    const where = holder.current;
    if (!where) return;

    const pane = new Terminal({
      fontSize: 12,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
      theme: { background: "#181818", foreground: "#d8d8d8" },
    });
    const fit = new FitAddon();
    pane.loadAddon(fit);
    pane.open(where);
    terminal.current = pane;
    bench.paneOpened(session, pane);

    let gone = false;
    const say = (trouble: string) => {
      if (!gone) pane.write(`\r\n\x1b[31mcharter: ${trouble}\x1b[0m\r\n`);
    };
    // The renderer loads its own code, so a pane draws with the DOM until it is there.
    void draw(pane, say, () => !gone).then((drawing) => bench.paneDrawing(session, drawing));
    const typed = pane.onData((text) => {
      // Input refused is worth seeing: the program has stopped reading it, or has ended.
      void commands.sendInput(session, text).then(
        (sent) => {
          if (sent.status === "error") say(sent.error);
        },
        (err: unknown) => say(String(err)),
      );
    });
    // The terminal decides the size, and the program is told it.
    const resized = pane.onResize(({ cols, rows }) => {
      void commands.resizeSession(session, cols, rows);
    });
    const watching = new ResizeObserver(() => fit.fit());
    watching.observe(where);

    let view: number | undefined;
    // What the view is sent waits here until the terminal is the size that screen was drawn
    // for, so a line the session wrapped is not wrapped again somewhere else. A benchmark
    // wants to know when its text reached the grid, so what it gives back travels with the
    // text it belongs to.
    const waiting: { text: string; written?: () => void }[] = [];
    let ready = false;
    const output = new Channel<string>();
    output.onmessage = (text) => {
      if (gone) return;
      const written = bench.paneSent(session, text);
      if (ready) pane.write(text, written);
      else waiting.push({ text, written });
    };
    // The pane's own size first: a session nobody is showing keeps whatever size it had.
    void (async () => {
      fit.fit();
      const opened = await commands
        .watchSession(session, output)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (opened.status === "error") {
        say(opened.error);
        return;
      }
      if (gone) {
        void commands.unwatchSession(session, opened.data.view);
        return;
      }
      view = opened.data.view;
      // The core keeps the history; the pane keeps the same, so scrolling back shows what the
      // session has rather than what this terminal happens to have seen.
      pane.options.scrollback = opened.data.scrollback;
      pane.resize(opened.data.columns, opened.data.rows);
      ready = true;
      for (const held of waiting.splice(0)) pane.write(held.text, held.written);
      // Now that the screen is drawn, the pane's own size applies again.
      fit.fit();
    })();

    return () => {
      gone = true;
      watching.disconnect();
      typed.dispose();
      resized.dispose();
      if (view !== undefined) void commands.unwatchSession(session, view);
      pane.dispose();
      terminal.current = undefined;
      bench.paneClosed(session);
    };
  }, [session]);

  return (
    <div
      className={focused ? "pane focused" : "pane"}
      data-testid="pane"
      data-session={session}
      onMouseDown={take}
      onClick={take}
      ref={holder}
    />
  );
}
