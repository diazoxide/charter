---
version: unreleased
headline: `secret cp` writes to a real file or to nothing, and a `secret exec` child no longer holds every other vault's identity
---

`charter secret cp` is documented as one of the two safe ways to consume a credential:
*"materializes a secret to a 0600 file and prints only the path, never the contents."*
That sentence was true of the command and false of the destination, which is what
actually decided where the plaintext went:

```
$ charter secret cp tv API_TOKEN /dev/stdout 2>&1 | cat
FABRICATED-…✓ Wrote 'tv/API_TOKEN' to /dev/stdout (0600). Value not shown.
```

`/dev/stdout` is charter's own stdout — an agent's captured pipe. The success line is
false on its own output, and `Value not shown` is the last thing printed after showing
it. `charter secret get --reveal` has refused exactly this channel for a long time. `cp`
refused nothing at all: not a device, not a FIFO, not a symlink pointed at a file you
care about, and not an existing file, which it truncated and chmodded 0600 without a
word — a `0644` config came back as the credential, with no warning and no flag.

**The destination is now checked before the vault is read**, so a refused path never
resolves the value at all:

- a device, FIFO, socket or directory is refused — `/dev/stdout`, `/dev/stderr` and
  `/dev/fd/*` by name in the message, because they are this conversation;
- **a destination that IS one of charter's own three streams is refused by identity**,
  before existence or `--force` is considered at all (see below);
- a symlink is refused rather than followed, and the write itself uses `O_NOFOLLOW`, so
  a link planted between the check and the open cannot redirect it either;
- an existing file is refused; overwriting one takes `--force`, and charter says
  afterwards that it did;
- the write is `O_EXCL` by default, the mode is set on the open descriptor rather than
  on the path, and at most one missing parent directory is created (at 0700) instead of
  a whole tree;
- a destination inside the plane that git would track is refused, the same rule
  `charter vault add` already applies to a plain-file vault — otherwise the next
  `charter save` commits the credential.

**The first version of that guard was bypassed, which is why the list above says "by
identity".** It asked what a destination was *called*: `os.lstat` on the path. On macOS
`/dev/fd/1` is neither a symlink nor a device — it is the underlying object — so the
answer came back "an ordinary file that already exists", which is precisely the arm
`--force` switches off. `charter secret cp <vault> <key> /dev/fd/1 --force` then wrote the
credential into charter's own captured stdout and printed *Value not shown.* on top of
it: issue #421's symptom, reproduced through its own fix. Without `--force` the refusal
ended *"Pass --force to overwrite it deliberately"* — a guard printing the recipe for its
own bypass, the pattern #421 was filed about in the first place.

No list of names closes that. `/dev/stdout`, `/dev/fd/1`, `/proc/self/fd/1`, the path
`readlink` gives for a transcript log and any hardlink to it are one file with five
names. So charter compares the destination's `(st_dev, st_ino)` — taken from an `fstat`
of the descriptor it actually opened, not from the path — against `fstat` of its own
stdin, stdout and stderr. One object, one answer, however it is spelled. The comparison
runs before the vault is read and before anything is truncated, `--force` does not reach
it, and the refusal it prints names no flag at all.

**A `secret exec` child no longer inherits other vaults' identity variables.** `env =
dict(os.environ)` handed the child every credential in the caller's shell. Measured with
fabricated values, `charter secret exec <vault> --env T=K -- /usr/bin/env` returned the
one secret the model had named as `***` and every *other* vault's service-account token
in the clear — redaction cannot help, because it only knows the values that call
resolved. Binding a vault to its own service account was sold as least-privilege, and
inheriting the whole environment put the mapping back in every child's environment.
charter now removes every identity variable declared by a vault other than the one being
read — both halves of each binding, the variable the CLI reads and the variable this
machine carries it in. The vault being read keeps its own, and nothing else is touched,
so `PATH`, your locale and every unrelated setting arrive as before.

**`--stream` cleans up after every terminating signal charter can catch.** The temp-file
cleanup is a `finally`, and a `finally` unwinds an exception — a default-action signal
unwinds nothing. Python installs a handler for `SIGINT` and nothing else, so `SIGINT` was
clean and `SIGTERM` left a `-rw-------` file holding the value in the system temp
directory. The docs named only SIGKILL, which reads as vanishingly rare; SIGTERM is what
a supervisor, a `kill`, or a harness reaping a hung tool call sends at every ordinary
shutdown, and `--stream` exists for exactly the long-running children that get SIGTERMed.

Handling SIGTERM and SIGHUP and then calling SIGKILL the whole limit was still false.
SIGQUIT is Ctrl-\ from any terminal, and it left the same file; so did SIGUSR1 and
SIGXCPU. Every catchable default-terminate signal has the property, not the two that
happened to get reported — a list of signals to *catch* is a list of spellings, and the
next one is never on it. charter now derives the set by exclusion: everything in
`signal.Signals` except what cannot be caught, what does not terminate, what suspends the
process (Ctrl-Z must background a command, not kill it), and the faults. Each of the rest
raises `SystemExit(128+N)` for the duration of the command, which runs the same cleanup
an ordinary exit does and kills the child on the way out.

The stated limit is a category now rather than three examples: cleanup does not survive
SIGKILL, which no process can catch, or a fault — SIGSEGV, SIGBUS, SIGABRT — which
charter deliberately does not intercept, because a handler running on a process whose
state is already wrong can turn a crash into a hang.

**And charter no longer says it shreds anything.** `_safe_unlink` is `os.unlink`, and it
should stay that way: an overwrite pass is meaningless on a copy-on-write filesystem,
where the rewritten bytes land in a new block and the old one stays wherever the drive
left it. Six comments, help strings and skill lines said "shred" about a plain delete.
The word was wrong, not the code, so the word changed — and a test now keeps it changed,
because it is an attractive word and the next person to describe this cleanup will reach
for it again.
