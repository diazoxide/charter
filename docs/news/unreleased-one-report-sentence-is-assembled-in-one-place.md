---
version: unreleased
headline: A report line is contained as it is assembled — in one place, not two that had already drifted
---

Charter had two private implementations of "contain the sentence as it is assembled", the
argument #502 ended on. `contain._sentence` bounded every field at `PATH_DISPLAY_LIMIT`.
`news._report`, added one version later, bounded every field at `DISPLAY_LIMIT` and joined
a field holding several things element by element so the separator could not come from the
data. Both were correct. Both were tested. Neither could see the other.

So the two things they differed by — the budget, and what happens to a field holding
several things — had been decided twice and written down nowhere the other copy could
read. That is not a hypothetical: it had already happened, in one version, in the module
whose own opening paragraph argues that a guard covering "the sites we thought of" grows a
new hole every time a noun is added.

There is one assembler now, and two public spellings of the budget:

* `contain.sentence(template, **fields)` — `DISPLAY_LIMIT`, for a line that names a
  filename, a frontmatter value, a config key.
* `contain.path_sentence(template, **fields)` — `PATH_DISPLAY_LIMIT`, for a refusal that
  names a resolved path, because a clipped path is one the reader cannot act on.

`news`'s thirteen reports and `contain`'s thirteen refusals are call sites of those two.
`charter/news.py` now contains no `str.format` call at all, and `charter/contain.py`
contains exactly the assembler's own two — asserted by an AST walk over both modules, so a
sentence assembled at a call site with an f-string is a red test rather than a review
comment.

**Two things the merge decided rather than copied.**

*The sequence handling is shared, because it was never news-specific.* A sentence naming
several committed things is what "a list of untrusted things in one line" always needs, and
containing the *joined* string is the bug it avoids — it clips the last entries out of a
sentence whose purpose is to name every one of them. `contain._not_plane_data` stops
joining the plane's data-root names by hand and hands them over as a sequence.

*"Several things" is now a property rather than a type.* `news._report` asked
`isinstance(value, (list, tuple))`, which is a spelling: a caller handing a `set`, a
`frozenset`, a `dict`'s keys or a generator fell off the end of it and got Python's own
`repr` of the container — brackets, quotes and all — printed into charter's prose. The test
is "is this one string, or is it something that can be iterated", with `str` and `bytes`
named as the exception. That is the same finding as
[#547](https://github.com/diazoxide/charter/issues/547) and
[#558](https://github.com/diazoxide/charter/issues/558) one surface over: a check matching
a spelling instead of the property it is about.

**And two functions rather than one with a `limit=` keyword**, which is a smaller point
worth writing down because the obvious shape is the wrong one. `**fields` makes the
template's slot names and the function's own parameter names one namespace, so a template
that ever grew a `{limit}` slot would have its value silently eaten as the budget and then
raise `KeyError` out of `str.format` — inside the module whose stated rule is that nothing
in it raises. Naming the budget by which function you call is what makes a slot name unable
to collide with it.

Nothing to adopt: no output changes. The value of the change is what the third reporting
surface gets — `commands_persona`'s tables, `frame/registry`'s entry-point errors,
`mcpseen` — which is one of two written-down budgets instead of a third invented one.

[#576](https://github.com/diazoxide/charter/issues/576).
