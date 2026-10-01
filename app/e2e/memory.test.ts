import { describe, expect, it } from "vitest";
import {
  HARNESS,
  harnessMemory,
  parseFootprints,
  parseProcesses,
  runs,
  webContent,
} from "./memory.js";

// `ps -A -o pid=,ppid=,rss=,etime=,command=`, as macOS and Linux print it: right-aligned
// numbers, an elapsed time of `[[dd-]hh:]mm:ss`, and a command that may hold spaces.
const PS = [
  "  501     1 812345 01-21:22:06 /System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent",
  "  700   600 210000    05:10 /Users/me/My Apps/charter-app --flag value",
  "  701   700  12000       00:07 /tmp/target/debug/fake-harness --name one",
  "",
].join("\n");

// `ps -A -o pid=,comm=`: the program itself, without its arguments. macOS prints the whole
// path, spaces and all; Linux prints the name, cut to fifteen characters.
const COMM = [
  "  501 /System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent",
  "  700 /Users/me/My Apps/charter-app",
  "  701 fake-harness",
  "",
].join("\n");

describe("parseProcesses", () => {
  it("reads each process's parent, memory, age, command and the program it runs", () => {
    expect(parseProcesses(PS, COMM)).toEqual([
      {
        pid: 501,
        ppid: 1,
        rssKb: 812345,
        seconds: 1 * 86_400 + 21 * 3_600 + 22 * 60 + 6,
        command:
          "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent",
        program: "com.apple.WebKit.WebContent",
      },
      {
        pid: 700,
        ppid: 600,
        rssKb: 210000,
        seconds: 310,
        command: "/Users/me/My Apps/charter-app --flag value",
        program: "charter-app",
      },
      {
        pid: 701,
        ppid: 700,
        rssKb: 12000,
        seconds: 7,
        command: "/tmp/target/debug/fake-harness --name one",
        program: "fake-harness",
      },
    ]);
  });
});

describe("runs", () => {
  it("says a process runs a program by its name, even when Linux cut that name to fifteen characters", () => {
    expect(runs("fake-harness", HARNESS)).toBe(true);
    expect(runs("WebKitWebProces", "WebKitWebProcess")).toBe(true);
    expect(runs("WebKitNetworkPr", "WebKitWebProcess")).toBe(false);
    expect(runs("sh", HARNESS)).toBe(false);
  });
});

const WEB_CONTENT =
  "/System/Library/Frameworks/WebKit.framework/Versions/A/XPCServices/com.apple.WebKit.WebContent.xpc/Contents/MacOS/com.apple.WebKit.WebContent";
const APP = {
  pid: 700,
  ppid: 600,
  rssKb: 210_000,
  seconds: 300,
  command: "/t/charter-app",
  program: "charter-app",
};

describe("webContent", () => {
  it("on macOS, counts the WebContent processes started after the app, and none started before", () => {
    const rows = [
      APP,
      // Another app's page, open since before this app started: not this window's.
      {
        pid: 501,
        ppid: 1,
        rssKb: 800_000,
        seconds: 90_000,
        command: WEB_CONTENT,
        program: "com.apple.WebKit.WebContent",
      },
      {
        pid: 702,
        ppid: 1,
        rssKb: 400_000,
        seconds: 290,
        command: WEB_CONTENT,
        program: "com.apple.WebKit.WebContent",
      },
      {
        pid: 703,
        ppid: 1,
        rssKb: 100_000,
        seconds: 120,
        command: WEB_CONTENT,
        program: "com.apple.WebKit.WebContent",
      },
      // WebKit's networking process is not web content.
      {
        pid: 704,
        ppid: 1,
        rssKb: 50_000,
        seconds: 290,
        command: WEB_CONTENT,
        program: "com.apple.WebKit.Networking",
      },
    ];
    expect(webContent(rows, 700, "darwin")).toEqual({ pids: [702, 703], rssKb: 500_000 });
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
        program: "WebKitWebProces",
      },
      {
        pid: 711,
        ppid: 700,
        rssKb: 40_000,
        seconds: 299,
        command: "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitNetworkProcess 16 49",
        program: "WebKitNetworkPr",
      },
      // Someone else's browser.
      {
        pid: 900,
        ppid: 899,
        rssKb: 900_000,
        seconds: 100,
        command: "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebProcess 3 9",
        program: "WebKitWebProces",
      },
    ];
    expect(webContent(rows, 700, "linux")).toEqual({ pids: [710], rssKb: 300_000 });
  });
});

describe("harnessMemory", () => {
  const process = (pid: number, ppid: number, rssKb: number, command: string, program: string) => ({
    pid,
    ppid,
    rssKb,
    seconds: 60,
    command,
    program,
  });

  it("counts each harness the app runs with everything it started, and says the largest", () => {
    const rows = [
      APP,
      process(720, 700, 10_000, "/t/fake-harness --name a", "fake-harness"),
      process(721, 700, 30_000, "/t/fake-harness --name b", "fake-harness"),
      // b's tool command, and that command's own child: b's child runs.
      process(730, 721, 5_000, "/bin/sh -c make", "sh"),
      process(731, 730, 2_000, "/usr/bin/make", "make"),
      // A shell tab the app runs is not a harness.
      process(740, 700, 3_000, "/bin/zsh -l", "zsh"),
      // A harness some other process started is not this app's.
      process(750, 1, 99_000, "/t/fake-harness --name c", "fake-harness"),
    ];
    expect(harnessMemory(rows, 700)).toEqual({
      pids: [720, 721],
      rssKb: 10_000 + 30_000 + 5_000 + 2_000,
      largestKb: 37_000,
    });
  });

  it("finds the harnesses the session host runs too, which launchd or systemd started, not the app", () => {
    const rows = [
      APP,
      process(800, 1, 40_000, "/Users/me/bin/charter serve", "charter"),
      process(801, 800, 20_000, "/t/fake-harness --name d", "fake-harness"),
      // `charter` doing anything else is not the host.
      process(810, 1, 9_000, "/Users/me/bin/charter save", "charter"),
      process(811, 810, 15_000, "/t/fake-harness --name e", "fake-harness"),
    ];
    expect(harnessMemory(rows, 700)).toEqual({ pids: [801], rssKb: 20_000, largestKb: 20_000 });
  });
});

describe("parseFootprints", () => {
  // `footprint -p 34496 -p 34498` on macOS 26, cut to the lines that matter.
  const BOTH = [
    "======================================================================",
    "com.apple.WebKit.WebContent [34498]: 64-bit    Footprint: 3007 MB (16384 bytes per page)",
    "======================================================================",
    "",
    "Shared with com.apple.WebKit.Networking [34496]:",
    "  48 KB      720 KB          0 B          8    TOTAL",
    "",
    "Auxiliary data:",
    "    phys_footprint: 3007 MB",
    "    phys_footprint_peak: 4096 MB",
    "",
    "======================================================================",
    "com.apple.WebKit.Networking [34496]: 64-bit    Footprint: 7009 KB (16384 bytes per page)",
    "",
    "Shared with com.apple.WebKit.WebContent [34498]:",
    "Auxiliary data:",
    "    phys_footprint: 7041 KB",
    "    phys_footprint_peak: 8513 KB",
    "",
    "======================================================================",
    "Shared with com.apple.WebKit.WebContent [34498], com.apple.WebKit.Networking [34496]:",
  ].join("\n");

  it("reads each process's footprint and its peak, in kilobytes, from one call for several", () => {
    expect(parseFootprints(BOTH, [34498, 34496])).toEqual({
      byPid: {
        34498: { footprintKb: 3007 * 1024, peakKb: 4096 * 1024 },
        34496: { footprintKb: 7041, peakKb: 8513 },
      },
      error: null,
    });
  });

  it("says which pid footprint could not read, in footprint's own words", () => {
    const missing = [
      "footprint: Unable to find pid for process matching '999999'",
      ...BOTH.split("\n").slice(0, 11),
    ].join("\n");
    expect(parseFootprints(missing, [34498, 999999])).toEqual({
      byPid: { 34498: { footprintKb: 3007 * 1024, peakKb: 4096 * 1024 } },
      error: "footprint: Unable to find pid for process matching '999999'",
    });
  });

  it("names the pids it got nothing for when footprint said nothing about them", () => {
    expect(parseFootprints("", [702])).toEqual({
      byPid: {},
      error: "footprint printed nothing for pid 702",
    });
  });
});
