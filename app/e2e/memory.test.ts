import { describe, expect, it } from "vitest";
import { footprintKb, harnesses, parseProcesses, webContent } from "./memory.js";

// `ps -A -o pid=,ppid=,rss=,etime=,command=`, as macOS and Linux print it: right-aligned
// numbers, an elapsed time of `[[dd-]hh:]mm:ss`, and a command that may hold spaces.
const PS = [
  "  501     1 812345 01-21:22:06 /System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent",
  "  700   600 210000    05:10 /tmp/target/debug/charter-app --flag value",
  "  701   700  12000       00:07 /tmp/target/debug/fake-harness --name one",
  "",
].join("\n");

describe("parseProcesses", () => {
  it("reads each line of ps into a pid, a parent, resident memory, seconds alive and a command", () => {
    expect(parseProcesses(PS)).toEqual([
      {
        pid: 501,
        ppid: 1,
        rssKb: 812345,
        seconds: 1 * 86_400 + 21 * 3_600 + 22 * 60 + 6,
        command:
          "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent",
      },
      {
        pid: 700,
        ppid: 600,
        rssKb: 210000,
        seconds: 310,
        command: "/tmp/target/debug/charter-app --flag value",
      },
      {
        pid: 701,
        ppid: 700,
        rssKb: 12000,
        seconds: 7,
        command: "/tmp/target/debug/fake-harness --name one",
      },
    ]);
  });
});

const WEB_CONTENT =
  "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent";
const APP = { pid: 700, ppid: 600, rssKb: 210_000, seconds: 300, command: "/t/charter-app" };

describe("webContent", () => {
  it("on macOS, counts the WebContent processes launchd started after the app, and none started before", () => {
    const rows = [
      APP,
      // Another app's page, open since before this app started: not this window's.
      { pid: 501, ppid: 1, rssKb: 800_000, seconds: 90_000, command: WEB_CONTENT },
      { pid: 702, ppid: 1, rssKb: 400_000, seconds: 290, command: WEB_CONTENT },
      { pid: 703, ppid: 1, rssKb: 100_000, seconds: 120, command: WEB_CONTENT },
      // WebKit's networking process is not web content.
      {
        pid: 704,
        ppid: 1,
        rssKb: 50_000,
        seconds: 290,
        command: WEB_CONTENT.replace(/WebContent/g, "Networking"),
      },
    ];
    expect(webContent(rows, 700, "darwin")).toEqual({ processes: [702, 703], rssKb: 500_000 });
  });

  it("on Linux, counts the app's own WebKitWebProcess children, wherever WebKitGTK installed them", () => {
    const rows = [
      APP,
      {
        pid: 710,
        ppid: 700,
        rssKb: 300_000,
        seconds: 299,
        command: "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess 15 47",
      },
      {
        pid: 711,
        ppid: 700,
        rssKb: 40_000,
        seconds: 299,
        command: "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitNetworkProcess 16 49",
      },
      // Someone else's browser.
      {
        pid: 900,
        ppid: 899,
        rssKb: 900_000,
        seconds: 100,
        command: "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess 3 9",
      },
    ];
    expect(webContent(rows, 700, "linux")).toEqual({ processes: [710], rssKb: 300_000 });
  });
});

describe("harnesses", () => {
  it("counts each harness the app runs with everything it started, and says the largest", () => {
    const rows = [
      APP,
      { pid: 720, ppid: 700, rssKb: 10_000, seconds: 60, command: "/t/fake-harness --name a" },
      { pid: 721, ppid: 700, rssKb: 30_000, seconds: 60, command: "/t/fake-harness --name b" },
      // b's tool command, and that command's own child: b's child runs.
      { pid: 730, ppid: 721, rssKb: 5_000, seconds: 10, command: "/bin/sh -c make" },
      { pid: 731, ppid: 730, rssKb: 2_000, seconds: 9, command: "/usr/bin/make" },
      // A shell tab the app runs is not a harness.
      { pid: 740, ppid: 700, rssKb: 3_000, seconds: 60, command: "/bin/zsh -l" },
      // A harness some other process started is not this app's.
      { pid: 750, ppid: 1, rssKb: 99_000, seconds: 60, command: "/t/fake-harness --name c" },
    ];
    expect(harnesses(rows, 700, "fake-harness")).toEqual({
      processes: [720, 721],
      rssKb: 10_000 + 30_000 + 5_000 + 2_000,
      largestKb: 37_000,
    });
  });
});

describe("footprintKb", () => {
  // `footprint -p <pid>` on macOS 26, as it printed it on the operator's machine.
  const printed = (size: string) =>
    [
      "======================================================================",
      `com.apple.WebKit.WebContent [702]: 64-bit    Footprint: ${size} (16384 bytes per page)`,
      "======================================================================",
      "",
      "  Dirty      Clean  Reclaimable    Regions    Category",
      " 480 KB        0 B          0 B          7    MALLOC_SMALL",
    ].join("\n");

  it("reads the footprint macOS gives a process, in kilobytes, whatever unit it printed", () => {
    expect(footprintKb(printed("1744 KB"))).toBe(1744);
    expect(footprintKb(printed("813 MB"))).toBe(813 * 1024);
    expect(footprintKb(printed("1.5 GB"))).toBe(1.5 * 1024 * 1024);
  });

  it("says nothing when footprint printed no total", () => {
    expect(footprintKb("footprint: no process with pid 702")).toBeNull();
  });
});
