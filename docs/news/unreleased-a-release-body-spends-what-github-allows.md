---
version: unreleased
headline: 'Release notes stop being shortened for headroom that was not needed — the budget is measured in the units GitHub might count, and its two guards read one window instead of two that could cross'
---

**Four notes of this release were about to publish as a headline and a link, and GitHub
would have taken the whole body.** So were four of 0.52.0's, whose published Release carries
all 24 in full.

The bound had a floor and a ceiling and they had crossed: no value of `news._BODY_BUDGET`
satisfied both. The floor counted every release GitHub would have accepted whole, across all
history, and required all but one to render whole — a demand that only grows. The ceiling
was a fixed fraction of the limit and never moved.

Most of that fraction was the units. GitHub refuses with `body is too long (maximum is
125000 characters)`, and whether the validator behind it counts code points or bytes is not
something charter can run and find out — so 15% was held back in case they differed.
`news.sent_length` settles it on the string: it measures the encoded body, which is never
below the character count, so the bound holds either way. Measured across every version
charter has cut, that costs between 0.25% and 1.22%.

What is left to reserve for is one thing — something added to the body that charter did not
render, landing after the PyPI upload. It is 3,000 now, a subtraction rather than a
percentage, and the budget is 122,000.

Both guards read one `CEILING`, and the floor asks only about releases that fit *under it*,
so the most it can demand is the ceiling itself. The ceiling reads no entries at all: it used
to measure the staged body and skip when nothing was staged, which is the tree `charter news
stamp` leaves behind — so the case objecting to a raise sat out every commit where a raise
would be proposed.
