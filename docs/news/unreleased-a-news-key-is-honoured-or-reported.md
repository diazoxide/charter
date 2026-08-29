---
version: unreleased
headline: A news entry is read the way its author wrote it, or charter says so — and an entry's filename can no longer write a line of charter's own report
security: true
---

**Affected: 0.53.0 and earlier. Fixed here.** Nothing on your plane is exposed by this;
what was at risk is charter's own disclosure process — a security entry could sink out of
the release notes in silence, and a committed filename could write a line of charter's own
report in charter's voice.

Two defects in the same subsystem, and the same shape twice: **a value crossing from a
committed file into something somebody reads, where the code handled one spelling and not
its neighbours.**

## A key charter does not read is a sentence now, not a shrug

`Security: true` used to sink an entry in silence. charter's frontmatter parser keeps a key
exactly as written, so that line put `Security` in the dict, the lookup for `security`
found nothing, and the entry was **absent** — not wrong, not reported, absent. It rendered
below the ordinary entries, `charter news --for` exited 0 with an empty stderr, and the
release published notes that did not carry what the entry declared.

That is the defect the ordering fields were added to prevent, restored through the *key*
instead of the value. The value half was already careful: a `security: yes` charter cannot
read is reported, never read as false. The key half had no such treatment, because an
unfound key leaves no value to report — which makes it the quieter of the two failures, and
the release notes are the one document nobody re-derives.

**The fix is not case-insensitivity.** Folding the lookup answers `Security:` and nothing
else: `securiy:`, `leads:`, `security-fix:` and `sec urity:` all parse cleanly, are looked
up by nothing, and sink an entry in exactly the same silence. It would also have to be done
in the shared frontmatter parser, whose dict is read by key for `role:`, `vault:`,
`extends:` and `tools:` — one of which decides which credentials a persona reaches — and it
would owe an answer to two keys that differ only in case.

So the six keys a news entry may declare are a closed set — `version`, `headline`, `check`,
`adopt`, `lead`, `security` — and anything else is **reported at the release gate**. Case
stops being a special case of anything, because there is no unspoken key of any kind left.
charter never guesses which field you meant; it tells you which one it does not have.

The sibling with worse teeth came with it. A miscased `Version:` does not sink an entry, it
**deletes** it: charter finds no version, drops the file before any view sees it, and the
release guard — which answers "does this version have an entry?" from *filenames* — waves
it through. Every `.md` in the news directory is now either an entry or a sentence naming
what is wrong with it.

Nothing that ships today changes: all 105 entries in the tree declare only keys charter
reads.

## And the report saying so is charter's own output

The message that reports an unreadable ordering value was built out of two spans:

```
<filename>: `security: <value>` is not a value charter reads — …
```

The **value** was contained. The committed **filename** three inches to its left was
interpolated raw — and a news entry's name is chosen by whoever writes the commit, exactly
as its frontmatter is. An entry named with a newline in it printed *two* lines where charter
emitted one, and the second was the author's sentence sitting in a CI log in charter's own
voice, above the release a human was about to publish.

The value was guarded because the value was what that commit was about. The filename was
not judged safe; it was not judged.

Four sites were named in the report and there were more, so the guard is no longer at the
spans somebody enumerated — it is at the **assembly**. Every sentence `news` writes about an
entry is built by handing a template and its fields to one function that bounds all of them,
so a field added to one of those templates tomorrow is contained by having been passed
there. That caught three spans nobody had listed: the version in the duplicate-`lead:`
message, the `check:` echoed by a probe that could not run, and the pair of filenames in
`charter news stamp`'s **success** line — the one that runs at every release. The slug,
headline and `adopt:` command printed by `charter news` and `charter news --pending` are
bounded too.

Escaped, never dropped: the value is still shown, spelled so it cannot restructure the
report it appears in.

## What this is worth checking

Nothing to adopt — upgrading is the whole of it. If you have written a news entry against
this charter and it did not appear in the notes you expected, `charter news --for <version>`
now says why instead of exiting 0.
