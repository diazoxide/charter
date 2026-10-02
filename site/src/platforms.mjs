// Reading the supported-platforms table (FR-25) out of a Markdown page, and the runners out of
// a workflow, so a test can hold the README's copy to `docs/platforms.md` and every runner the
// table names to `ci.yml`.

/** The lines of the first Markdown table whose header row starts with `| Platform |`. */
export function platformTable(text) {
  const lines = text.split("\n");
  const start = lines.findIndex((line) => /^\|\s*Platform\s*\|/.test(line));
  if (start === -1) return [];
  const end = lines.findIndex((line, i) => i > start && !line.startsWith("|"));
  return lines.slice(start, end === -1 ? lines.length : end).map((line) => line.trimEnd());
}

/** The backticked labels in the table's `CI runner` column, in order. */
export function runnersOf(table) {
  const cells = (line) => line.split("|").slice(1, -1).map((cell) => cell.trim());
  const column = cells(table[0] ?? "").indexOf("CI runner");
  if (column === -1) return [];
  return table
    .slice(2)
    .flatMap((row) => [...(cells(row)[column] ?? "").matchAll(/`([^`]+)`/g)].map((m) => m[1]));
}

/** Every label the workflow runs on: `runs-on:` values, `os:` matrix lists and container images. */
export function runnersInWorkflow(yml) {
  const found = [];
  for (const raw of yml.split("\n")) {
    const line = raw.replace(/\s#.*$/, "").trim();
    if (line.startsWith("#")) continue;
    const value = /^(?:-\s*)?(?:runs-on|os|image|container):\s*(.+)$/.exec(line)?.[1];
    if (!value || value.includes("${{")) continue;
    const list = /^\[(.*)\]$/.exec(value);
    for (const item of list ? list[1].split(",") : [value]) {
      const label = item.trim().replace(/^["']|["']$/g, "");
      if (label) found.push(label);
    }
  }
  return [...new Set(found)];
}
