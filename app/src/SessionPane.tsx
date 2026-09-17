import { useEffect, useRef } from "react";
import { Channel } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { commands } from "./bindings";

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

  useEffect(() => {
    const where = holder.current;
    if (!where) return;

    const terminal = new Terminal({
      scrollback: 5_000,
      fontSize: 12,
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
      theme: { background: "#181818", foreground: "#d8d8d8" },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(where);

    const typed = terminal.onData((text) => {
      void commands.sendInput(session, text);
    });
    // The terminal decides the size, and the program is told it.
    const resized = terminal.onResize(({ cols, rows }) => {
      void commands.resizeSession(session, cols, rows);
    });
    const watching = new ResizeObserver(() => fit.fit());
    watching.observe(where);

    let view: number | undefined;
    let gone = false;
    // What the view is sent waits here until the terminal is the size that screen was drawn
    // for, so a line the session wrapped is not wrapped again somewhere else.
    const waiting: string[] = [];
    let ready = false;
    const output = new Channel<string>();
    output.onmessage = (text) => {
      if (ready) terminal.write(text);
      else waiting.push(text);
    };
    // The pane's own size first: a session nobody is showing keeps whatever size it had.
    void (async () => {
      fit.fit();
      const opened = await commands
        .watchSession(session, output)
        .catch((err: unknown) => ({ status: "error" as const, error: String(err) }));
      if (opened.status === "error") {
        terminal.write(`\r\n\x1b[31mcharter: ${opened.error}\x1b[0m\r\n`);
        return;
      }
      if (gone) {
        void commands.unwatchSession(session, opened.data.view);
        return;
      }
      view = opened.data.view;
      terminal.resize(opened.data.columns, opened.data.rows);
      ready = true;
      for (const text of waiting.splice(0)) terminal.write(text);
      // Now that the screen is drawn, the pane's own size applies again.
      fit.fit();
    })();

    return () => {
      gone = true;
      watching.disconnect();
      typed.dispose();
      resized.dispose();
      if (view !== undefined) void commands.unwatchSession(session, view);
      terminal.dispose();
    };
  }, [session]);

  return (
    <div
      className={focused ? "pane focused" : "pane"}
      data-testid="pane"
      data-session={session}
      onMouseDown={onFocus}
      onFocus={onFocus}
      ref={holder}
    />
  );
}
