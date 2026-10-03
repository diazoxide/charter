### Security

- **The guards treat the two forge CLIs alike.** A persona that declares `gh` is now asked
  before running one of its delete or remove commands, as one declaring `glab` already was. The
  leak guard now refuses a vault file that `glab` would upload to the forge, as it did for `gh`,
  and reads more of the ways either CLI uploads a local file. Every guard table that names a
  forge CLI pairs each row with the other CLI's, or says why that CLI has none, and a test fails
  on a row added for one forge CLI alone (#1068, #1080).
