//! The scan of an agent's commit before it is made (SQ-16): every line the commit ADDS, in any
//! repository a chat commits to, asked [`crate::secretshape::leaks`] — the same scanner a plane
//! save and a repo save use, with the rule a diff of code needs: vendor credential shapes and
//! personal data, and not the credential-assignment rule that refuses `password: String`.
//!
//! **What is read is the index being committed**, from inside git's own `pre-commit` hook
//! ([`crate::githooks`]), so `git commit -a` and `git commit <paths>` — which stage into a
//! temporary index git names in `GIT_INDEX_FILE` — are scanned as what they commit.
//!
//! **Only added lines.** A value already in the history, or one this commit removes, is not
//! something this commit publishes. A file git calls binary has no lines and is not read.
//!
//! **Each line as written, and as it reads** (#1304): a string can spell a character as an
//! escape (`\u0041` is `A`), so each added line is also asked with its escapes decoded
//! ([`crate::secretshape::escaped_leaks`]) — no parse, so a file of any name and one that
//! would not parse are read alike. What is found that way is masked, fingerprinted and
//! allowlisted by the value it spells, so an entry names a value however it is spelled. One
//! value read both ways is named once, by the value it spells, unless the allowlist lets one
//! reading through and not the other; and added lines that end in a backslash are read joined
//! to the next, as a TOML `"""` string reads them (#1315).
//!
//! **What is shown is masked** ([`crate::secretshape::masked`]): the kind, where it is, and a
//! short head of the value. Never the value.
//!
//! # What is let through (SQ-17)
//!
//! [`checked`] is [`staged`] filtered through the allowlist ([`crate::scanallow`]): charter's
//! own entries, and the repository's file as it is at `HEAD`. It also says whether the commit
//! changes that file, which a chat's commit may not.
//!
//! # Before a push (SQ-7)
//!
//! [`pushed`] is the same scan over what a push would send: every commit git's `pre-push`
//! names that the remote does not already have, each commit's added lines, filtered through the
//! same allowlist. It catches a commit that never passed a chat's `pre-commit`: one the operator
//! made in an unarmed terminal, or one `cherry-pick`, `rebase` or `am` made (ADR 0074, V26b).

use std::path::Path;

use crate::scanallow::{self, Allowlist, Entry, Seen};
use crate::secretshape;
use crate::worktree::git;

/// One thing a staged commit would publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The file, as git names it from the top of the work tree.
    pub path: String,
    /// The 1-based line in the new version of the file.
    pub line: usize,
    /// The rule that found it, by the id an allowlist entry names it with.
    pub rule: &'static str,
    /// What it looks like.
    pub kind: &'static str,
    /// The value, masked.
    pub masked: String,
    /// The value's fingerprint ([`scanallow::fingerprint`]), which an entry can name it by.
    pub fingerprint: String,
}

/// A staged commit, scanned and filtered through its allowlist.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    /// What the commit is refused for.
    pub refused: Vec<Finding>,
    /// What an entry let through, and the entry.
    pub allowed: Vec<(Finding, Entry)>,
    /// Entries of the repository's allowlist that allow nothing, and why.
    pub problems: Vec<String>,
    /// Whether the commit changes the allowlist file itself, which only a commit made outside a
    /// chat may.
    pub changes_the_allowlist: bool,
}

/// [`staged`], filtered through charter's own entries and the repository's allowlist as it is at
/// `HEAD` — never the working tree's or the index's, so an entry written and not yet committed
/// allows nothing ([`crate::scanallow`]).
pub fn checked(repo: &Path) -> Result<Scan, String> {
    let found = staged_read(repo)?;
    let at_head = |args: &[&str]| git::run_in_hook(repo, args, git::READ).ok();
    let changed = at_head(&[
        "-c",
        "core.quotePath=false",
        "diff",
        "--cached",
        "--no-renames",
        "--name-only",
        "-z",
    ])
    .filter(|run| run.code == Some(0))
    .ok_or("git could not name the files the commit changes")?;
    let mut scan = through_the_allowlist(repo, found);
    scan.changes_the_allowlist = String::from_utf8_lossy(&changed.out)
        .split('\0')
        .any(scanallow::is_the_file);
    Ok(scan)
}

/// `found`, split into what is refused and what an entry lets through: charter's own entries
/// and the repository's allowlist as it is at `HEAD`.
fn through_the_allowlist(repo: &Path, found: Read) -> Scan {
    through(allowlist_at(repo, "HEAD").unwrap_or_default(), found)
}

/// The text of the repository's allowlist at `rev`, or `None` when `rev` has none or git could
/// not show it. The purlis name wins when `rev` has it (RN-2a, V93e); the old one is read only
/// when it does not.
fn allowlist_at(repo: &Path, rev: &str) -> Option<String> {
    scanallow::spellings().find_map(|file| {
        git::run_in_hook(repo, &["show", &format!("{rev}:{file}")], git::READ)
            .ok()
            .filter(|run| run.code == Some(0))
            .map(|run| String::from_utf8_lossy(&run.out).into_owned())
    })
}

/// What a diff adds that looks like a secret or personal data, with which of its findings are
/// one value read two ways.
#[derive(Debug, Default)]
struct Read {
    found: Vec<Finding>,
    /// `twin[i] = Some(j)`: finding `i` is the line as written of the value finding `j` reads
    /// through its escapes — the same rule, ending where it ends on the line, and one of the
    /// two values ending the other (a `\n` in front of an email reads as a longer address as
    /// written).
    twin: Vec<Option<usize>>,
}

/// `found` split through charter's own entries and the allowlist file `text`.
///
/// A value read two ways ([`Read::twin`]) is named once, by the value its escapes spell, when
/// the allowlist lets both readings through or neither (#1315). When it lets one through and
/// not the other, both are kept, so an entry for one spelling never answers for the other.
fn through(text: String, found: Read) -> Scan {
    let allowlist = Allowlist::of(Allowlist::parse(&text));
    let mut scan = Scan {
        problems: allowlist.problems.clone(),
        ..Scan::default()
    };
    let allowing = |finding: &Finding| {
        allowlist.allowing(Seen {
            rule: finding.rule,
            path: &finding.path,
            fingerprint: &finding.fingerprint,
        })
    };
    let entries: Vec<Option<&Entry>> = found.found.iter().map(allowing).collect();
    for (at, finding) in found.found.into_iter().enumerate() {
        let joined = found
            .twin
            .get(at)
            .copied()
            .flatten()
            .is_some_and(|other| entries[other].is_some() == entries[at].is_some());
        if joined {
            continue;
        }
        match entries[at] {
            Some(entry) => scan.allowed.push((finding, entry.clone())),
            None => scan.refused.push(finding),
        }
    }
    scan
}

/// How long git may take to show what a push sends. A push is a network operation already, and
/// a branch's first push to a new remote shows its whole history.
const SHOWING_A_PUSH: std::time::Duration = git::NETWORK;

/// git's object name for "nothing": a deleted ref on the local side, a new one on the remote's.
fn is_nothing(oid: &str) -> bool {
    oid.bytes().all(|b| b == b'0')
}

/// What a push from `repo` to `remote` (at `url`) would publish, from the lines git writes on
/// `pre-push`'s standard input (`updates`: `<local ref> <local oid> <remote ref> <remote oid>`),
/// filtered through the allowlist the remote already has.
///
/// The commits read are those reachable from what is pushed and from neither what the remote
/// has for those refs nor, when the push names a configured remote, any of its remote-tracking
/// refs: what the push sends. Each is read as its own diff, root commits included whatever
/// `log.showRoot` says, so a secret one commit adds and a later one removes is found: the history
/// still publishes it. Merge commits are not read; each side's commits are.
///
/// **What is pushed must be a commit.** A ref is peeled to the commit it names (an annotated tag
/// to its commit); a ref that names a blob or a tree, directly or through a tag, has no history
/// to read, so it is refused rather than sent unread.
///
/// **The allowlist is the remote's.** A pushed range that changes `.charter-scan-allow.toml`
/// sets [`Scan::changes_the_allowlist`], which the push hook refuses: only the operator pushes an
/// allowlist change. Otherwise findings go through the file as the remote has it for each ref
/// (the remote's commit, or, for a new ref, the pushed tip, which the range leaves unchanged);
/// refs whose files differ get charter's own entries alone.
///
/// `Err` is input git did not write, a ref that is not a commit, or git failing to answer, all
/// of which the caller refuses on.
pub fn pushed(repo: &Path, remote: &str, url: &str, updates: &str) -> Result<Scan, String> {
    let commit_of = |oid: &str| {
        git::run_in_hook(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{oid}^{{commit}}"),
            ],
            git::READ,
        )
        .ok()
        .filter(|run| run.code == Some(0))
        .map(|run| String::from_utf8_lossy(&run.out).trim().to_owned())
        .filter(|oid| !oid.is_empty())
    };
    let mut sends = Vec::new();
    let mut has = Vec::new();
    // The revision whose allowlist each update is filtered through.
    let mut allowlists = Vec::new();
    for line in updates.lines().filter(|l| !l.trim().is_empty()) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [name, local, _, theirs] = fields[..] else {
            return Err(format!("a line git wrote could not be read: {line:?}"));
        };
        let is_oid = |oid: &str| !oid.is_empty() && oid.bytes().all(|b| b.is_ascii_hexdigit());
        if !is_oid(local) || !is_oid(theirs) {
            return Err(format!("a line git wrote could not be read: {line:?}"));
        }
        if is_nothing(local) {
            continue;
        }
        let Some(tip) = commit_of(local) else {
            return Err(format!(
                "{name} does not name a commit, so what it would publish has no history to \
                 scan; push without it"
            ));
        };
        let known = (!is_nothing(theirs)).then(|| commit_of(theirs)).flatten();
        if !is_nothing(theirs) {
            has.push(format!("^{theirs}"));
        }
        allowlists.push(known.unwrap_or_else(|| tip.clone()));
        sends.push(tip);
    }
    if sends.is_empty() {
        return Ok(Scan::default());
    }
    let mut range = sends;
    range.extend(has);
    // git hands a push to a URL with no remote configured the URL twice, and a URL is no
    // remote-tracking namespace.
    if remote != url {
        range.push("--not".to_owned());
        range.push(format!("--remotes={remote}"));
    }
    let ask = |head: &[&str]| {
        let mut args: Vec<&str> = head.to_vec();
        // The remote's side of a ref names a commit this clone may never have fetched.
        args.push("--ignore-missing");
        args.extend(range.iter().map(String::as_str));
        args.push("--");
        // Every name the allowlist goes by: a change to either is one to the allowlist.
        if head.first() == Some(&"rev-list") {
            for name in scanallow::spellings() {
                args.push(name);
            }
        }
        let run = git::run_in_hook(repo, &args, SHOWING_A_PUSH).map_err(|gone| gone.to_string())?;
        match run.code {
            Some(0) => Ok(String::from_utf8_lossy(&run.out).into_owned()),
            None => Err("git took too long to show what the push sends".to_owned()),
            Some(_) => Err(String::from_utf8_lossy(&run.err).trim().to_owned()),
        }
    };
    let changes_the_allowlist = !ask(&["rev-list", "--full-history"])?.trim().is_empty();
    let diff = ask(&[
        "-c",
        "core.quotePath=false",
        "log",
        "-p",
        // `log.showRoot=false` would otherwise show a root commit with no diff at all.
        "--root",
        "--no-merges",
        "--format=",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        "-U0",
    ])?;
    let texts: Vec<Option<String>> = allowlists
        .iter()
        .map(|rev| allowlist_at(repo, rev))
        .collect();
    let text = if texts.windows(2).all(|pair| pair[0] == pair[1]) {
        texts.into_iter().next().flatten().unwrap_or_default()
    } else {
        String::new()
    };
    let mut scan = through(text, in_diff(&diff)?);
    scan.changes_the_allowlist = changes_the_allowlist;
    Ok(scan)
}

/// Everything the staged diff in `repo` adds that looks like a secret or personal data, in the
/// order git lists it — a value its escapes spell as well as one written as it is, so one value
/// can be here twice; [`checked`] names it once. `Err` is git failing to answer, which the
/// caller refuses on: a commit that could not be read is not a commit that was checked.
pub fn staged(repo: &Path) -> Result<Vec<Finding>, String> {
    staged_read(repo).map(|read| read.found)
}

/// [`staged`], with which findings are one value read two ways.
fn staged_read(repo: &Path) -> Result<Read, String> {
    let run = git::run_in_hook(
        repo,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "--cached",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "-U0",
        ],
        git::READ,
    )
    .map_err(|gone| gone.to_string())?;
    if run.code != Some(0) {
        return Err(match run.code {
            None => "git took too long to show the staged diff".to_owned(),
            Some(_) => String::from_utf8_lossy(&run.err).trim().to_owned(),
        });
    }
    in_diff(&String::from_utf8_lossy(&run.out))
}

/// What [`staged`] finds in a unified diff's text, or why the text could not be read — which the
/// caller refuses the commit on, since a file whose name could not be read is a file nobody
/// scanned.
///
/// Each added line as written and as it reads ([`on_line`]). Added lines in a row that end in
/// a backslash are also read as one text (#1315): a TOML `"""` string joins such a line to the
/// next, so a credential split across them reads whole. Only a value that no single line shows
/// is taken from the joined text, named on the line it starts on. Each line is joined into one
/// text at most, what was seen is a set and the line a finding starts on is a binary search,
/// so the time grows with the diff and not with its square.
fn in_diff(diff: &str) -> Result<Read, String> {
    let mut read = Read::default();
    let mut path: Option<String> = None;
    let mut at = 0usize;
    let mut in_hunk = false;
    let mut run = Joined::default();
    for line in diff.lines() {
        let added = if in_hunk {
            line.strip_prefix('+')
        } else {
            None
        };
        if added.is_none() {
            run.flush(&mut read);
        }
        if line.starts_with("diff --git ") {
            path = None;
            in_hunk = false;
        } else if !in_hunk && let Some(new) = line.strip_prefix("+++ ") {
            path = new_path(new)?;
        } else if let Some(hunk) = line.strip_prefix("@@ ") {
            in_hunk = true;
            at = hunk
                .split_whitespace()
                .find_map(|part| part.strip_prefix('+'))
                .and_then(|new| new.split(',').next())
                .and_then(|start| start.parse().ok())
                .ok_or_else(|| format!("a hunk header git wrote could not be read: {line:?}"))?;
        } else if let Some(added) = added {
            let Some(path) = &path else {
                return Err("git showed added lines with no file named for them".to_owned());
            };
            let found = on_line(added);
            let first = read.found.len();
            for (leak, value, twin) in &found {
                read.found.push(Finding {
                    path: path.clone(),
                    line: at,
                    rule: leak.rule,
                    kind: leak.kind,
                    masked: secretshape::masked(value),
                    fingerprint: scanallow::fingerprint(value),
                });
                read.twin.push(twin.map(|other| first + other));
            }
            run.push(
                path,
                at,
                added,
                found.into_iter().map(|(leak, value, _)| (leak.rule, value)),
            );
            // A blank added line inside a join is swallowed by it too, as TOML's line-ending
            // backslash swallows every blank line up to the next that is not.
            let blank = added.bytes().all(|b| matches!(b, b' ' | b'\t' | b'\r'));
            if !secretshape::joins_the_next_line(added) && !(blank && run.is_joining()) {
                run.flush(&mut read);
            }
            at += 1;
        } else if in_hunk && line.starts_with(' ') {
            at += 1;
        }
    }
    run.flush(&mut read);
    Ok(read)
}

/// What `added` holds as written, then what it spells through its escapes, in the order they
/// stand on the line: each with its value, and — for one read as written — the index of the
/// finding its escapes make it (see [`Read::twin`]).
fn on_line(added: &str) -> Vec<(secretshape::Leak, String, Option<usize>)> {
    let mut found: Vec<(secretshape::Leak, String, bool)> = secretshape::leaks(added)
        .into_iter()
        .map(|leak| {
            let value = added[leak.span.clone()].to_owned();
            (leak, value, false)
        })
        .chain(
            secretshape::escaped_leaks(added)
                .into_iter()
                .map(|(leak, value)| (leak, value, true)),
        )
        .collect();
    found.sort_by_key(|(leak, ..)| leak.span.start);
    // One value read two ways ends where the line ends it either way: the escape is in front.
    // Looked up by where each ends, so a line of many findings is still read in one pass.
    let spelled: std::collections::HashMap<(&str, usize), usize> = found
        .iter()
        .enumerate()
        .filter(|(_, (.., escaped))| *escaped)
        .map(|(at, (leak, ..))| ((leak.rule, leak.span.end), at))
        .collect();
    let twin = |(leak, value, escaped): &(secretshape::Leak, String, bool)| {
        if *escaped {
            return None;
        }
        spelled
            .get(&(leak.rule, leak.span.end))
            .copied()
            .filter(|at| {
                let other = &found[*at].1;
                value.ends_with(other.as_str()) || other.ends_with(value.as_str())
            })
    };
    let twins: Vec<Option<usize>> = found.iter().map(twin).collect();
    found
        .into_iter()
        .zip(twins)
        .map(|((leak, value, _), twin)| (leak, value, twin))
        .collect()
}

/// Added lines in a row, each but the last ending in a backslash, read as one text.
#[derive(Default)]
struct Joined {
    path: String,
    /// The file line each joined line is, and where it starts in `text`.
    starts: Vec<(usize, usize)>,
    text: String,
    /// What the lines showed one at a time, by rule and value: a set, so many lines of many
    /// values are still read in one pass.
    seen: std::collections::HashSet<(&'static str, String)>,
}

impl Joined {
    fn push(
        &mut self,
        path: &str,
        line: usize,
        added: &str,
        seen: impl Iterator<Item = (&'static str, String)>,
    ) {
        if self.starts.is_empty() {
            self.path = path.to_owned();
        } else {
            self.text.push('\n');
        }
        self.starts.push((line, self.text.len()));
        self.text.push_str(added);
        self.seen.extend(seen);
    }

    /// Whether a line before the last one pushed joined it: the run is still open.
    fn is_joining(&self) -> bool {
        self.starts.len() > 1
    }

    /// What only the joined text shows, into `read`; then start again.
    fn flush(&mut self, read: &mut Read) {
        let joined = std::mem::take(self);
        if joined.starts.len() < 2 {
            return;
        }
        for (leak, value) in secretshape::escaped_leaks(&joined.text) {
            if joined.seen.contains(&(leak.rule, value.clone())) {
                continue;
            }
            // The last line that starts at or before it: `starts` is in order.
            let after = joined
                .starts
                .partition_point(|(_, start)| *start <= leak.span.start);
            let line = joined.starts[after.saturating_sub(1)].0;
            read.found.push(Finding {
                path: joined.path.clone(),
                line,
                rule: leak.rule,
                kind: leak.kind,
                masked: secretshape::masked(&value),
                fingerprint: scanallow::fingerprint(&value),
            });
            read.twin.push(None);
        }
    }
}

/// The file a `+++ ` header names: `b/<path>`, or git's C-quoted `"b/<path>"` for a name
/// holding a quote, a backslash, a control character or (with `core.quotePath` off) nothing
/// else; `None` for `/dev/null`, a deletion. Anything else is an error.
///
/// git ends a name that holds a space with a tab (`diff.c`, so `patch` can find where the name
/// stops); that one tab is not part of the name.
fn new_path(header: &str) -> Result<Option<String>, String> {
    let header = header.strip_suffix('\t').unwrap_or(header);
    if header == "/dev/null" {
        return Ok(None);
    }
    let name = if header.starts_with('"') {
        unquoted(header)
            .ok_or_else(|| format!("a file name git quoted could not be read: {header:?}"))?
    } else {
        header.to_owned()
    };
    name.strip_prefix("b/")
        .map(|path| Some(path.to_owned()))
        .ok_or_else(|| format!("a file header git wrote could not be read: {header:?}"))
}

/// git's C-quoted name, unquoted (`quote.c`'s `unquote_c_style`): `\"`, `\\`, `\a` `\b` `\t`
/// `\n` `\v` `\f` `\r`, and three octal digits for any other byte. `None` for anything that is
/// not exactly one such quoted string.
fn unquoted(quoted: &str) -> Option<String> {
    let inner = quoted.strip_prefix('"')?.strip_suffix('"')?;
    let mut bytes = Vec::with_capacity(inner.len());
    let mut rest = inner.as_bytes().iter().copied();
    while let Some(byte) = rest.next() {
        match byte {
            b'"' => return None,
            b'\\' => {
                let escaped = rest.next()?;
                bytes.push(match escaped {
                    b'"' => b'"',
                    b'\\' => b'\\',
                    b'a' => 0x07,
                    b'b' => 0x08,
                    b't' => b'\t',
                    b'n' => b'\n',
                    b'v' => 0x0b,
                    b'f' => 0x0c,
                    b'r' => b'\r',
                    b'0'..=b'3' => {
                        let (two, three) = (rest.next()?, rest.next()?);
                        let digit = |d: u8| (b'0'..=b'7').contains(&d).then(|| d - b'0');
                        (escaped - b'0') * 64 + digit(two)? * 8 + digit(three)?
                    }
                    _ => return None,
                });
            }
            other => bytes.push(other),
        }
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// What a chat's commit that changes the allowlist is refused with.
pub const ALLOWLIST_REFUSAL: &str = "purlis: commit refused — it changes .charter-scan-allow.toml, the \
     scan's allowlist, which only a commit made outside a chat may change. Take the file out of \
     the commit (`git restore --staged .charter-scan-allow.toml`); the operator reviews and \
     commits an entry. Do not use --no-verify; the operator has been told.\n";

/// What a chat's push of a range that changes the allowlist is refused with.
pub const PUSH_ALLOWLIST_REFUSAL: &str = "purlis: push refused — it would publish a change to \
     .charter-scan-allow.toml, the scan's allowlist. The operator pushes an allowlist change \
     themselves. Do not use --no-verify; the operator has been told.\n";

/// The most findings a refusal lists; the rest are counted.
const LISTED: usize = 20;

/// What a refusal stops: a chat's commit, at `pre-commit` or `pre-merge-commit`, or its push,
/// at `pre-push`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Commit,
    Push,
}

impl Stopped {
    fn word(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Push => "push",
        }
    }
}

/// What the commit or push is refused with, on standard error: every finding, masked.
pub fn refusal(stopped: Stopped, found: &[Finding]) -> String {
    let mut out = format!(
        "purlis: {} refused — it would publish what looks like a secret or personal data:\n",
        stopped.word()
    );
    for finding in found.iter().take(LISTED) {
        out.push_str(&format!("  {}\n", one_line(finding)));
    }
    if found.len() > LISTED {
        out.push_str(&format!("  … and {} more\n", found.len() - LISTED));
    }
    out.push_str(match stopped {
        Stopped::Commit => "Take it out of the change and commit again. ",
        Stopped::Push => {
            "It is in a commit the push would send, which the commit scan never saw. Take it out \
             of that commit's history before pushing (amend it, or rebase and edit the commit \
             that added it), and if it is a live credential, revoke it: removing it from the \
             files alone leaves it in history. "
        }
    });
    out.push_str(
        "A credential belongs in a vault (`purlis secret`), and personal data does not belong \
         in a repository. If a finding is not what it looks like, `purlis scan --explain` names \
         its rule and the entry that would let it through; tell the operator, who adds it to \
         .charter-scan-allow.toml in a commit of their own. Do not use --no-verify; the \
         operator has been told.\n",
    );
    out
}

/// What the app's needs-you item says about a refusal in `repo`: what was stopped, the
/// repository's name and the first finding, masked, with how many more there are.
pub fn summary(stopped: Stopped, repo: &Path, found: &[Finding]) -> String {
    let name = repo.file_name().map_or_else(
        || repo.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let first = found.first().map(one_line).unwrap_or_default();
    let what = stopped.word();
    match found.len() {
        0 | 1 => format!("{what} refused in {name}: {first}"),
        n => format!("{what} refused in {name}: {first} (and {} more)", n - 1),
    }
}

/// One finding as a line: where, what, and the value masked.
pub fn one_line(finding: &Finding) -> String {
    format!(
        "{}:{}  {}  {}",
        finding.path, finding.line, finding.kind, finding.masked
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgit;

    fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(dir.path()).unwrap();
        testgit::run(&repo, &["init", "-q", "-b", "main", "."]);
        testgit::run(&repo, &["config", "user.name", "t"]);
        testgit::run(&repo, &["config", "user.email", "t@example.invalid"]);
        (dir, repo)
    }

    fn key() -> String {
        ["ghp", "_0123456789abcdefABCDEF"].concat()
    }

    #[test]
    fn a_planted_key_and_email_in_the_staged_diff_are_found_masked_with_path_and_line() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("app.py"), "print('hi')\n").unwrap();
        testgit::run(&repo, &["add", "app.py"]);
        testgit::run(&repo, &["commit", "-q", "-m", "one"]);
        std::fs::write(
            repo.join("app.py"),
            format!(
                "print('hi')\nTOKEN = '{}'\nOWNER = 'ada@lovelace.dev'\n",
                key()
            ),
        )
        .unwrap();
        testgit::run(&repo, &["add", "app.py"]);

        let found = staged(&repo).unwrap();
        assert_eq!(
            found,
            vec![
                Finding {
                    path: "app.py".into(),
                    line: 2,
                    rule: "forge-token",
                    kind: "a token by its forge's prefix",
                    masked: "ghp_**********************".into(),
                    fingerprint: scanallow::fingerprint(&key()),
                },
                Finding {
                    path: "app.py".into(),
                    line: 3,
                    rule: "email",
                    kind: "an email address",
                    masked: "ad**************".into(),
                    fingerprint: scanallow::fingerprint("ada@lovelace.dev"),
                },
            ]
        );
        let said = refusal(Stopped::Commit, &found);
        assert!(
            !said.contains(&key()) && !said.contains("ada@lovelace.dev"),
            "{said}"
        );
        assert!(
            said.contains("app.py:2") && said.contains("app.py:3"),
            "{said}"
        );
    }

    #[test]
    fn only_added_lines_count_and_a_clean_diff_finds_nothing() {
        let (_dir, repo) = repo();
        // Already committed, and removed: neither is this commit publishing it.
        std::fs::write(repo.join("old.txt"), format!("{}\nkeep\n", key())).unwrap();
        std::fs::write(repo.join("owners"), "ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "."]);
        testgit::run(&repo, &["commit", "-q", "-m", "one"]);
        std::fs::write(repo.join("old.txt"), "keep\n").unwrap();
        std::fs::write(repo.join("owners"), "ada@lovelace.dev\nplain\n").unwrap();
        testgit::run(&repo, &["add", "."]);
        assert_eq!(staged(&repo).unwrap(), Vec::new());
    }

    #[test]
    fn a_new_file_and_a_binary_file_are_read_as_git_shows_them() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("new.env"), format!("\n\nX={}\n", key())).unwrap();
        std::fs::write(repo.join("blob.bin"), b"\0\0ada@lovelace.dev\0").unwrap();
        testgit::run(&repo, &["add", "."]);
        let found = staged(&repo).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|f| (f.path.as_str(), f.line))
                .collect::<Vec<_>>(),
            vec![("new.env", 3)]
        );
    }

    #[test]
    fn a_file_whose_name_git_quotes_is_scanned_under_its_own_name() {
        let (_dir, repo) = repo();
        for name in [
            "say \"hi\".txt",
            "tab\there.txt",
            "back\\slash.txt",
            "plain space.txt",
        ] {
            std::fs::write(repo.join(name), format!("{}\n", key())).unwrap();
        }
        testgit::run(&repo, &["add", "."]);

        let mut named: Vec<String> = staged(&repo).unwrap().into_iter().map(|f| f.path).collect();
        named.sort();

        assert_eq!(
            named,
            [
                "back\\slash.txt",
                "plain space.txt",
                "say \"hi\".txt",
                "tab\there.txt"
            ]
        );
    }

    /// A diff that adds `lines` to `path`, one hunk from line 1.
    fn adding(path: &str, lines: &[&str]) -> String {
        let mut diff = format!(
            "diff --git a/{path} b/{path}\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
            lines.len()
        );
        for line in lines {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        diff
    }

    #[test]
    fn an_escape_in_front_of_an_email_names_it_once_by_the_value_it_spells() {
        // `\n` and `\0` are letters to the scan as written, so it reads a longer address than
        // the one the line spells; both are the same finding (#1315).
        // Built at run time, so this file carries no address the scan would stop.
        let address = ["ada", "@", "lovelace.dev"].concat();
        for line in [
            format!(r#""owner": "the deploy\n{address}""#),
            format!(r#""owner": "the deploy\0{address}""#),
        ] {
            let found = in_diff(&adding("owners.json", &[&line])).unwrap();
            assert_eq!(found.found.len(), 2, "{line}: {:?}", found.found);
            let scan = through(String::new(), found);
            assert_eq!(scan.refused.len(), 1, "{line}: {:?}", scan.refused);
            assert_eq!(
                scan.refused[0].fingerprint,
                scanallow::fingerprint(&address),
                "{line}"
            );
        }
        // Only the spelling where the escape is: the same longer address elsewhere on the line
        // is its own finding.
        let two = format!(r#""a": "n{address}", "b": "\n{address}""#);
        let scan = through(
            String::new(),
            in_diff(&adding("owners.json", &[&two])).unwrap(),
        );
        assert_eq!(scan.refused.len(), 2, "{:?}", scan.refused);
    }

    #[test]
    fn one_spelling_an_entry_lets_through_and_one_it_does_not_are_both_kept() {
        let finding = |value: &str| Finding {
            path: "fixture.txt".into(),
            line: 1,
            rule: "forge-token",
            kind: "a token by its forge's prefix",
            masked: secretshape::masked(value),
            fingerprint: scanallow::fingerprint(value),
        };
        let token = crate::secretshape::escaped::token();
        let longer = format!("{token}AB");
        let read = Read {
            found: vec![finding(&longer), finding(&token)],
            twin: vec![Some(1), None],
        };
        let allowing_the_shorter = format!(
            "[[allow]]\nfingerprint = \"{}\"\nreason = \"a revoked fixture\"\n",
            scanallow::fingerprint(&token)
        );
        let scan = through(allowing_the_shorter, read);
        assert_eq!(scan.refused.len(), 1, "{:?}", scan.refused);
        assert_eq!(scan.refused[0].fingerprint, scanallow::fingerprint(&longer));
        assert_eq!(scan.allowed.len(), 1);
    }

    #[test]
    fn a_credential_split_by_a_line_ending_backslash_is_read_across_the_added_lines() {
        let token = crate::secretshape::escaped::token();
        let diff = adding(
            "deploy/settings",
            &[
                "[deploy]",
                &format!("note = \"\"\"{}\\", &token[..8]),
                &format!("    {}\"\"\"", &token[8..]),
            ],
        );
        let found = in_diff(&diff).unwrap().found;
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].line, found[0].rule), (2, "forge-token"));
        assert_eq!(found[0].fingerprint, scanallow::fingerprint(&token));
        // A line that does not end in one is not joined to the next.
        let apart = adding(
            "deploy/settings",
            &[&format!("{}\\ x", &token[..8]), &token[8..]],
        );
        assert_eq!(in_diff(&apart).unwrap().found, Vec::new());
    }

    #[test]
    fn a_line_ending_backslash_with_blanks_after_it_joins_across_blank_lines() {
        // TOML's line-ending backslash swallows the blanks after it on its line and every
        // blank line up to the next that is not: the shape the train's CI found (#1315).
        let token = crate::secretshape::escaped::token();
        let first = format!("note = \"\"\"{}\\  ", &token[..8]);
        let last = format!("    {}\"\"\"", &token[8..]);
        let found = in_diff(&adding("deploy", &[&first, "", " \t", &last, "note = 1"]))
            .unwrap()
            .found;
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].line, found[0].rule), (1, "forge-token"));
        assert_eq!(found[0].fingerprint, scanallow::fingerprint(&token));
        // A blank line with no join before it starts nothing.
        let apart = in_diff(&adding("deploy", &["", &token[..8], &token[8..]]))
            .unwrap()
            .found;
        assert_eq!(apart, Vec::new());
    }

    #[test]
    fn many_escaped_addresses_and_many_joined_lines_read_in_linear_time() {
        let started = std::time::Instant::now();
        // Every value its own, so no seen list is short-cut by a repeat.
        let line: String = (0..1 << 13)
            .map(|i| format!(r"\na{i}x@lovelace{i}.dev "))
            .collect();
        let scan = through(String::new(), in_diff(&adding("a.txt", &[&line])).unwrap());
        assert_eq!(scan.refused.len(), 1 << 13);
        let joined: Vec<String> = (0..1 << 14)
            .map(|i| format!("b{i}x@lovelace{i}.dev a{i}\\"))
            .collect();
        let joined: Vec<&str> = joined.iter().map(String::as_str).collect();
        let found = in_diff(&adding("b.toml", &joined)).unwrap().found;
        assert!(found.len() >= 1 << 14, "{}", found.len());
        let took = started.elapsed();
        assert!(took < std::time::Duration::from_secs(10), "{took:?}");
    }

    #[test]
    fn a_header_that_cannot_be_read_refuses_rather_than_skips() {
        for diff in [
            "diff --git a/x b/x\n+++ \"b/unterminated\n@@ -0,0 +1 @@\n+x\n",
            "diff --git a/x b/x\n+++ elsewhere/x\n@@ -0,0 +1 @@\n+x\n",
            "diff --git a/x b/x\n+++ b/x\n@@ nonsense @@\n+x\n",
            "diff --git a/x b/x\n@@ -0,0 +1 @@\n+x\n",
        ] {
            assert!(in_diff(diff).is_err(), "{diff:?}");
        }
        assert_eq!(
            unquoted(r#""b/a\"b\\c\td\303\251""#).as_deref(),
            Some("b/a\"b\\c\tdé")
        );
    }

    /// Commits `.charter-scan-allow.toml` with `text` at `HEAD`, as the operator would.
    fn allowlist_at_head(repo: &std::path::Path, text: &str) {
        std::fs::write(repo.join(scanallow::FILE), text).unwrap();
        testgit::run(repo, &["add", scanallow::FILE]);
        testgit::run(repo, &["commit", "-q", "-m", "allowlist"]);
    }

    const DOCS_EMAIL: &str = "[[allow]]\nrule = \"email\"\npaths = [\"docs/**\"]\nreason = \"the docs name their authors\"\n";

    #[test]
    fn an_allowlisted_fixture_passes_and_says_which_entry_let_it() {
        let (_dir, repo) = repo();
        allowlist_at_head(&repo, DOCS_EMAIL);
        std::fs::create_dir(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/intro.md"), "Written by ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "docs/intro.md"]);

        let scan = checked(&repo).unwrap();

        assert!(scan.refused.is_empty(), "{:?}", scan.refused);
        assert_eq!(scan.allowed.len(), 1);
        assert_eq!(scan.allowed[0].1.origin, scanallow::Origin::File(1));
        assert!(!scan.changes_the_allowlist);
    }

    #[test]
    fn an_entry_written_and_not_committed_allows_nothing() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("README.md"), "one\n").unwrap();
        testgit::run(&repo, &["add", "README.md"]);
        testgit::run(&repo, &["commit", "-q", "-m", "one"]);
        // In the working tree only: what an agent could write and not commit.
        std::fs::write(repo.join(scanallow::FILE), DOCS_EMAIL).unwrap();
        std::fs::create_dir(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/intro.md"), "ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "docs/intro.md"]);

        let scan = checked(&repo).unwrap();

        assert_eq!(scan.refused.len(), 1);
        // And staging it is the change a chat's commit may not make.
        testgit::run(&repo, &["add", scanallow::FILE]);
        assert!(checked(&repo).unwrap().changes_the_allowlist);
    }

    #[test]
    fn an_author_in_a_manifest_is_let_through_with_no_allowlist_at_all() {
        let (_dir, repo) = repo();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nauthors = [\"Ada <ada@lovelace.dev>\"]\n",
        )
        .unwrap();
        std::fs::write(
            repo.join("src.rs"),
            format!("// ada@lovelace.dev {}\n", key()),
        )
        .unwrap();
        testgit::run(&repo, &["add", "."]);

        let scan = checked(&repo).unwrap();

        assert_eq!(
            scan.allowed
                .iter()
                .map(|(f, _)| f.path.as_str())
                .collect::<Vec<_>>(),
            ["Cargo.toml"]
        );
        assert_eq!(
            scan.refused.iter().map(|f| f.rule).collect::<Vec<_>>(),
            ["email", "forge-token"]
        );
    }

    #[test]
    fn a_broken_allowlist_says_so_and_allows_nothing() {
        let (_dir, repo) = repo();
        allowlist_at_head(&repo, "[[allow]]\nrule = \"email\"\n");
        std::fs::write(repo.join("notes.md"), "ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "notes.md"]);

        let scan = checked(&repo).unwrap();

        assert_eq!(scan.refused.len(), 1);
        assert!(scan.problems[0].contains("reason"), "{:?}", scan.problems);
    }

    #[test]
    fn moving_the_allowlist_away_is_a_change_to_it() {
        let (_dir, repo) = repo();
        allowlist_at_head(&repo, DOCS_EMAIL);
        testgit::run(&repo, &["mv", scanallow::FILE, "elsewhere.toml"]);

        assert!(checked(&repo).unwrap().changes_the_allowlist);
    }

    // The rename window (RN-2a, V93e): the allowlist under either name.

    #[test]
    fn an_allowlist_under_the_purlis_name_is_read_and_wins_over_the_old_one() {
        let (_dir, repo) = repo();
        // The old file allows nothing in docs/; the purlis one does, and wins.
        std::fs::write(repo.join(".charter-scan-allow.toml"), "").unwrap();
        std::fs::write(repo.join(".purlis-scan-allow.toml"), DOCS_EMAIL).unwrap();
        testgit::run(&repo, &["add", "."]);
        testgit::run(&repo, &["commit", "-q", "-m", "allowlists"]);
        std::fs::create_dir(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/intro.md"), "Written by ada@lovelace.dev\n").unwrap();
        testgit::run(&repo, &["add", "docs/intro.md"]);

        let scan = checked(&repo).unwrap();

        assert!(scan.refused.is_empty(), "{:?}", scan.refused);
        assert_eq!(scan.allowed[0].1.origin, scanallow::Origin::File(1));
    }

    #[test]
    fn a_staged_allowlist_under_either_name_is_a_change_to_it() {
        for name in [".charter-scan-allow.toml", ".purlis-scan-allow.toml"] {
            let (_dir, repo) = repo();
            std::fs::write(repo.join("README.md"), "one\n").unwrap();
            testgit::run(&repo, &["add", "README.md"]);
            testgit::run(&repo, &["commit", "-q", "-m", "one"]);
            std::fs::write(repo.join(name), DOCS_EMAIL).unwrap();
            testgit::run(&repo, &["add", name]);

            assert!(checked(&repo).unwrap().changes_the_allowlist, "{name}");
        }
    }

    // `pushed` (SQ-7).

    const NOTHING: &str = "0000000000000000000000000000000000000000";

    fn commit_all(repo: &Path, message: &str) -> String {
        testgit::run(repo, &["add", "-A"]);
        testgit::run(repo, &["commit", "-q", "-m", message]);
        testgit::run(repo, &["rev-parse", "HEAD"])
            .out
            .trim()
            .to_owned()
    }

    fn update(local: &str, theirs: &str) -> String {
        format!("refs/heads/main {local} refs/heads/main {theirs}\n")
    }

    #[test]
    fn a_push_to_a_url_scans_what_the_remote_lacks_and_not_what_it_has() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("old.sh"), format!("T={}\n", key())).unwrap();
        let published = commit_all(&repo, "published long ago");
        std::fs::write(repo.join("owners"), "ada@lovelace.dev\n").unwrap();
        let tip = commit_all(&repo, "new");

        let url = "https://forge.invalid/o/r.git";
        let scan = pushed(&repo, url, url, &update(&tip, &published)).unwrap();

        assert_eq!(
            scan.refused
                .iter()
                .map(|f| (f.path.as_str(), f.rule))
                .collect::<Vec<_>>(),
            [("owners", "email")]
        );
    }

    #[test]
    fn a_new_branch_is_scanned_back_to_its_root_and_a_deletion_sends_nothing() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("old.sh"), format!("T={}\n", key())).unwrap();
        let tip = commit_all(&repo, "root");

        let url = "/elsewhere.git";
        assert_eq!(
            pushed(&repo, url, url, &update(&tip, NOTHING))
                .unwrap()
                .refused
                .len(),
            1
        );
        assert!(
            pushed(&repo, url, url, &update(NOTHING, &tip))
                .unwrap()
                .refused
                .is_empty()
        );
    }

    #[test]
    fn the_remotes_commit_this_clone_never_fetched_does_not_stop_the_scan() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("owners"), "ada@lovelace.dev\n").unwrap();
        let tip = commit_all(&repo, "new");
        let unknown = "1234567890abcdef1234567890abcdef12345678";

        let scan = pushed(&repo, "/r.git", "/r.git", &update(&tip, unknown)).unwrap();

        assert_eq!(scan.refused.len(), 1);
    }

    #[test]
    fn input_git_did_not_write_is_refused_not_waved_through() {
        let (_dir, repo) = repo();
        for updates in ["refs/heads/main abc", "a b c d e", "r not-an-oid r 0000"] {
            assert!(pushed(&repo, "o", "/o", updates).is_err(), "{updates}");
        }
        assert!(pushed(&repo, "o", "/o", "").unwrap().refused.is_empty());
    }

    #[test]
    fn a_push_refusal_says_the_value_is_in_history_and_never_shows_it() {
        let finding = Finding {
            path: "deploy.sh".into(),
            line: 1,
            rule: "forge-token",
            kind: "a token by its forge's prefix",
            masked: "ghp_****".into(),
            fingerprint: String::new(),
        };
        let said = refusal(Stopped::Push, std::slice::from_ref(&finding));
        assert!(said.starts_with("purlis: push refused"), "{said}");
        assert!(said.contains("history"), "{said}");
        assert_eq!(
            summary(Stopped::Push, Path::new("/x/app"), &[finding]),
            "push refused in app: deploy.sh:1  a token by its forge's prefix  ghp_****"
        );
    }

    #[test]
    fn a_new_branchs_root_commit_is_read_even_where_log_hides_root_diffs() {
        let (_dir, repo) = repo();
        testgit::run(&repo, &["config", "log.showRoot", "false"]);
        std::fs::write(repo.join("deploy.sh"), format!("T={}\n", key())).unwrap();
        let root = commit_all(&repo, "root");

        let scan = pushed(&repo, "/r.git", "/r.git", &update(&root, NOTHING)).unwrap();

        assert_eq!(scan.refused.len(), 1, "{scan:?}");
    }

    #[test]
    fn a_tag_that_names_a_blob_is_refused_and_an_annotated_tag_is_read_as_its_commit() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("loose"), format!("T={}\n", key())).unwrap();
        let blob = testgit::run(&repo, &["hash-object", "-w", "loose"])
            .out
            .trim()
            .to_owned();
        let tag = format!("refs/tags/x {blob} refs/tags/x {NOTHING}\n");
        let why = pushed(&repo, "/r.git", "/r.git", &tag).unwrap_err();
        assert!(
            why.contains("refs/tags/x") && why.contains("not name a commit"),
            "{why}"
        );

        std::fs::remove_file(repo.join("loose")).unwrap();
        std::fs::write(repo.join("owners"), "ada@lovelace.dev\n").unwrap();
        commit_all(&repo, "owners");
        testgit::run(&repo, &["tag", "-a", "-m", "v1", "v1"]);
        let annotated = testgit::run(&repo, &["rev-parse", "v1"])
            .out
            .trim()
            .to_owned();
        let tag = format!("refs/tags/v1 {annotated} refs/tags/v1 {NOTHING}\n");
        assert_eq!(
            pushed(&repo, "/r.git", "/r.git", &tag)
                .unwrap()
                .refused
                .len(),
            1
        );
    }

    #[test]
    fn a_pushed_range_that_changes_the_allowlist_says_so() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("README"), "x\n").unwrap();
        let published = commit_all(&repo, "published");
        allowlist_at_head(&repo, DOCS_EMAIL);
        let tip = testgit::run(&repo, &["rev-parse", "HEAD"])
            .out
            .trim()
            .to_owned();

        let scan = pushed(&repo, "/r.git", "/r.git", &update(&tip, &published)).unwrap();

        assert!(scan.changes_the_allowlist);
        assert!(
            !pushed(&repo, "/r.git", "/r.git", &update(&published, NOTHING))
                .unwrap()
                .changes_the_allowlist,
            "a range that does not touch the file"
        );
    }

    #[test]
    fn findings_go_through_the_allowlist_the_remote_has_not_the_checkouts() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("README"), "x\n").unwrap();
        let published = commit_all(&repo, "published");
        // The remote's commit has no allowlist; the branch pushed carries a docs email.
        std::fs::create_dir(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/intro.md"), "By ada@lovelace.dev\n").unwrap();
        let tip = commit_all(&repo, "docs");
        // The checkout is elsewhere, with an allowlist that would let it through.
        testgit::run(&repo, &["checkout", "-q", "-b", "elsewhere", &published]);
        allowlist_at_head(&repo, DOCS_EMAIL);

        let scan = pushed(&repo, "/r.git", "/r.git", &update(&tip, &published)).unwrap();

        assert_eq!(scan.refused.len(), 1, "{scan:?}");
        assert!(scan.allowed.is_empty());
    }

    #[test]
    fn an_allowlist_the_remote_already_has_lets_a_finding_through() {
        let (_dir, repo) = repo();
        allowlist_at_head(&repo, DOCS_EMAIL);
        let published = testgit::run(&repo, &["rev-parse", "HEAD"])
            .out
            .trim()
            .to_owned();
        std::fs::create_dir(repo.join("docs")).unwrap();
        std::fs::write(repo.join("docs/intro.md"), "By ada@lovelace.dev\n").unwrap();
        let tip = commit_all(&repo, "docs");

        let scan = pushed(&repo, "/r.git", "/r.git", &update(&tip, &published)).unwrap();

        assert!(scan.refused.is_empty(), "{scan:?}");
        assert_eq!(scan.allowed.len(), 1);
    }

    /// The value an escaped shape spells: the AWS key or the forge token it carries.
    fn spelled(kind: &str) -> String {
        if kind == "AWS access key" {
            ["AKIA", "IOSFODNN7EXAMPLE"].concat()
        } else {
            crate::secretshape::escaped::token()
        }
    }

    #[test]
    fn a_credential_spelled_with_escapes_on_an_added_line_is_found_by_the_value_it_spells() {
        // Every bare-credential spelling of #1295, NO-7 and #1304, one added line at a time:
        // the escapes decoded with no parse, in a file of any name, parsed or not.
        for shape in crate::secretshape::escaped::shapes()
            .into_iter()
            .filter(|shape| shape.token)
        {
            let (_dir, repo) = repo();
            let file = repo.join(shape.path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, &shape.text).unwrap();
            testgit::run(&repo, &["add", "."]);

            let found = staged(&repo).unwrap();
            let value = spelled(shape.kind);
            assert_eq!(found.len(), 1, "{}: {found:?}", shape.text);
            let one = &found[0];
            assert_eq!(
                (one.path.as_str(), one.kind),
                (shape.path, shape.kind),
                "{}",
                shape.text
            );
            assert_eq!(
                one.fingerprint,
                scanallow::fingerprint(&value),
                "{}",
                shape.text
            );
            assert_eq!(one.masked, secretshape::masked(&value), "{}", shape.text);
            let line = shape.text.lines().nth(one.line - 1).unwrap();
            assert!(line.contains('\\'), "{}: line {}", shape.text, one.line);
            let said = refusal(Stopped::Commit, &found);
            assert!(!said.contains(&value[4..]), "{said}");
        }
    }

    #[test]
    fn a_push_of_a_credential_spelled_with_escapes_is_refused() {
        let (_dir, repo) = repo();
        std::fs::write(repo.join("README.md"), "one\n").unwrap();
        let published = commit_all(&repo, "one");
        let tail = &crate::secretshape::escaped::token()[1..];
        std::fs::write(
            repo.join("events.jsonl"),
            format!("{{\"n\": \"\\u0067{tail}\"}}\n"),
        )
        .unwrap();
        let tip = commit_all(&repo, "new");

        let url = "https://forge.invalid/o/r.git";
        let scan = pushed(&repo, url, url, &update(&tip, &published)).unwrap();

        assert_eq!(
            scan.refused
                .iter()
                .map(|f| (f.path.as_str(), f.rule))
                .collect::<Vec<_>>(),
            [("events.jsonl", "forge-token")]
        );
    }

    #[test]
    fn an_entry_naming_a_value_lets_it_through_however_it_is_spelled() {
        let (_dir, repo) = repo();
        let value = crate::secretshape::escaped::token();
        allowlist_at_head(
            &repo,
            &format!(
                "[[allow]]\nfingerprint = \"{}\"\nreason = \"a revoked fixture\"\n",
                scanallow::fingerprint(&value)
            ),
        );
        std::fs::write(
            repo.join("fixture.json"),
            format!("{{\"t\": \"\\u0067{}\"}}\n", &value[1..]),
        )
        .unwrap();
        testgit::run(&repo, &["add", "fixture.json"]);

        let scan = checked(&repo).unwrap();

        assert!(scan.refused.is_empty(), "{:?}", scan.refused);
        assert_eq!(scan.allowed.len(), 1);
    }

    #[test]
    fn a_file_with_escapes_and_no_credential_finds_nothing() {
        for (path, text) in crate::secretshape::escaped::clean() {
            let (_dir, repo) = repo();
            let file = repo.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, &text).unwrap();
            testgit::run(&repo, &["add", "."]);
            assert_eq!(staged(&repo).unwrap(), Vec::new(), "{text}");
        }
    }
}
