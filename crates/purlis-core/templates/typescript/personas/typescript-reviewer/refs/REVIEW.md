# Reviewing a TypeScript change

What `typescript-reviewer` checks in every change. Edit it to match how this project works: it is
yours, and a chat reads the copy here.

- [ ] Does it typecheck with no new `any`, `@ts-ignore` or non-null `!` that hides a real case?
- [ ] Is every promise awaited or deliberately left, and is a rejection handled?
- [ ] Does a change to a public export, a CLI flag or a config shape come with a version bump and a changelog line?
- [ ] Is a new dependency needed, maintained, small enough, and licensed in a way this project accepts? Is the lockfile updated with it?
- [ ] Is user input escaped where it reaches HTML, a shell or a query?
- [ ] Do the tests describe behaviour through the public interface, and was each new one seen to fail first?
