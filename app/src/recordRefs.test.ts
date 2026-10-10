import { describe, expect, it } from "vitest";
import { codeSpans, recordRef } from "./recordRefs";

describe("a file named in a session record (#984, D-984-3)", () => {
  it("names a repo by its first segment, and the line after the colon", () => {
    expect(recordRef("svc/src/lib.rs:42", "alpha", ["svc", "tool"])).toEqual({
      place: { workspace: "alpha", repo: "svc", piece: null },
      path: "src/lib.rs",
      line: 42,
    });
  });

  it("is at line 1 when it names no line", () => {
    expect(recordRef("tool/README.md", "alpha", ["svc", "tool"])).toEqual({
      place: { workspace: "alpha", repo: "tool", piece: null },
      path: "README.md",
      line: 1,
    });
  });

  it("is in the workspace's only repo when it names none", () => {
    expect(recordRef("src/lib.rs:7", "alpha", ["svc"])).toEqual({
      place: { workspace: "alpha", repo: "svc", piece: null },
      path: "src/lib.rs",
      line: 7,
    });
    expect(recordRef("Cargo.toml", "alpha", ["svc"])?.path).toBe("Cargo.toml");
  });

  it("is nothing when the workspace has several repos and none is named", () => {
    expect(recordRef("src/lib.rs:7", "alpha", ["svc", "tool"])).toBeUndefined();
    expect(recordRef("src/lib.rs:7", "alpha", [])).toBeUndefined();
  });

  it("is nothing for code that is not a path", () => {
    for (const code of [
      "npm test",
      "true",
      "foo()",
      "svc",
      "svc/",
      "/etc/passwd",
      "~/notes.md",
      "svc/../secret.txt",
      "svc/./lib.rs",
      "svc//lib.rs",
      "C:\\src\\lib.rs",
      "https://example.com/a.md",
      "svc/src/lib.rs:0",
      "svc/src/lib.rs:",
      "svc/src/lib.rs:12:3",
      "svc/src/lib.rs:x",
    ])
      expect(recordRef(code, "alpha", ["svc"]), code).toBeUndefined();
  });

  it("names the repo itself, not a folder of the only repo, when the first segment is the repo", () => {
    expect(recordRef("svc/lib.rs", "alpha", ["svc"])?.path).toBe("lib.rs");
  });
});

describe("a record's code spans", () => {
  it("are each inline span once, in order, and none from a fenced block", () => {
    const body = [
      "Read `a.rs:1` and `b.rs`, then `a.rs:1` again.",
      "```",
      "`c.rs` inside a fence",
      "```",
      "~~~~",
      "```",
      "`d.rs` still inside",
      "~~~~",
      "Last, `e.rs`.",
    ].join("\n");
    expect(codeSpans(body)).toEqual(["a.rs:1", "b.rs", "e.rs"]);
  });
});
