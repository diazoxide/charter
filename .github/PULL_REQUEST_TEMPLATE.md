<!-- Thank you for contributing. CONTRIBUTING.md has the details. A security fix goes through
     SECURITY.md first, not a public pull request. -->

## What this changes and why

<!-- One or two sentences. Name the issue it closes: "Closes #123". -->

## How it is tested

<!-- The test that fails without this change, or why this change cannot have one. For each
     acceptance line of the issue this meets: the line, then the test that checks it. -->

## Checklist

- [ ] Every commit is signed off (`Signed-off-by`, `git commit -s`), per the DCO in CONTRIBUTING.md (not needed when a maintainer opens the pull request).
- [ ] What CI runs passes locally (README.md, "Develop").
- [ ] Each acceptance line this pull request meets names the test that checks it, under "How it
      is tested", or says why it cannot have one.
- [ ] A change to the window: a screenshot of the real window was looked at before calling it done (light and dark, narrow and wide where it matters), and is attached or described here. The app's tests draw no pixels.
- [ ] A changelog fragment, `changes/<slug>.md` (see `changes/README.md`), if people using purlis would notice. CHANGELOG.md itself is not edited.
