//! A credential charter refuses is named by its KIND, and the matched text is never printed.
//!
//! `secretshape`'s module docstring says it ("it answers with a KIND … because every caller
//! of it prints the answer"), `handoff.rs`'s refusal says it, and `charter save`'s guard is
//! held to it by `planegit::tests`. The handoff caller had nothing checking it at all: the
//! differential scenario searched the refusal for its headline and stopped there, so a
//! refusal that had gone on to quote the brief would have passed — and would have passed on
//! BOTH sides, since the two implementations write the same sentence.
//!
//! That is the failure this file exists for. A refusal is read by the model that wrote the
//! brief and by whatever logs the turn, so a refusal that repeats the credential puts it in
//! the one place the refusal exists to keep it out of.
//!
//! Every kind `secretshape::CHECKS` knows is driven, rather than the one spelling the
//! differential happens to use: the rule is about the classifier's answer, not about one
//! brief.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// A copy of the committed `daily` fixture plane, which the Python charter wrote.
fn daily() -> tempfile::TempDir {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/planes/daily");
    let dir = tempfile::tempdir().expect("a directory");
    copy(&fixture, &dir.path().join("plane"));
    dir
}

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("a readable fixture") {
        let entry = entry.expect("an entry");
        let path = entry.path();
        if path.is_dir() {
            copy(&path, &to.join(entry.file_name()));
        } else {
            std::fs::copy(&path, to.join(entry.file_name())).expect("a copy");
        }
    }
}

fn root(tmp: &tempfile::TempDir) -> PathBuf {
    tmp.path().join("plane")
}

/// `charter handoff alpha` with `brief` on stdin.
///
/// stdin is a PIPE and is written to: a handoff refuses a brief from a terminal and a brief
/// from a closed descriptor with its own sentences, and either of those would answer this
/// test's question without ever reaching the classifier.
fn handoff(root: &Path, brief: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_charter"));
    command
        .args(["handoff", "alpha"])
        .current_dir(root)
        .env("CHARTER_ROOT", root)
        .env("CHARTER_SESSION_ID", "fixture-session-1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in [
        "CLAUDE_CODE_SESSION_ID",
        "CHARTER_WORKSPACE",
        "CHARTER_PERSONA",
        "CHARTER_HARNESS",
        // The app's socket and this chat's number: inherited, they would send the handoff
        // to whichever charter app is running the test, and the test is about this binary.
        "CHARTER_HOOK_SOCKET",
        "CHARTER_CHAT",
        "TERM_SESSION_ID",
        "TMUX_PANE",
        "STY",
        "SSH_TTY",
    ] {
        command.env_remove(name);
    }
    let mut child = command.spawn().expect("the binary runs");
    child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(brief.as_bytes())
        .expect("the brief is written");
    child.wait_with_output().expect("the binary finishes")
}

/// Everything the process said, both streams, as one string — because "which stream" is not
/// the question. A credential on stdout is as disclosed as one on stderr.
fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// `(the brief, the KIND charter answers, the run of characters that must not come back)`.
///
/// One row per rule in `secretshape::CHECKS`. The third column is the part of the brief that
/// IS the credential — for the PEM it is the header, which is all a private key shows of
/// itself before its body.
const EVERY_KIND: [(&str, &str, &str); 5] = [
    (
        "# Rotate it\n\nkey: am_us_ZZQQ1234\n",
        "AgentMail key",
        "am_us_ZZQQ1234",
    ),
    (
        "# Rotate it\n\njwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJaWlFRIn0\n",
        "JWT",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJaWlFRIn0",
    ),
    (
        "# Rotate it\n\n-----BEGIN OPENSSH PRIVATE KEY-----\n",
        "private key (PEM)",
        "BEGIN OPENSSH PRIVATE KEY",
    ),
    (
        "# Rotate it\n\naws AKIAZZQQ1234567890\n",
        "AWS access key",
        "AKIAZZQQ1234567890",
    ),
    (
        "# Rotate it\n\nAPI_KEY=zzqq5678ab\n",
        "credential assignment",
        "zzqq5678ab",
    ),
];

#[test]
fn a_credential_shaped_brief_is_refused_by_its_kind_and_the_value_never_comes_back() {
    let tmp = daily();
    let root = root(&tmp);

    for (brief, kind, credential) in EVERY_KIND {
        let out = handoff(&root, brief);
        let told = said(&out);

        assert_eq!(
            out.status.code(),
            Some(1),
            "`{kind}` was not refused: {told}"
        );
        assert!(
            told.contains("the brief looks like it carries a secret"),
            "`{kind}` was refused for something else: {told}"
        );
        assert!(
            told.contains(&format!("({kind})")),
            "the refusal did not name the kind `{kind}`: {told}"
        );
        // The whole point. A refusal that quotes what it matched hands the credential to
        // the reader of the transcript.
        assert!(
            !told.contains(credential),
            "the refusal REPEATED the credential ({kind}): {told}"
        );
    }
}

#[test]
fn a_brief_that_names_where_a_credential_lives_is_not_refused_as_one() {
    // The exemption `secretshape` exists to keep — refusing a vault reference would refuse
    // the remedy the refusal above names. Driven through the real command rather than the
    // classifier, because the sentence and the classifier are what have to agree.
    let tmp = daily();
    let root = root(&tmp);

    // The reference is the WHOLE value on its line. That is the rule, not a detail of the
    // fixture: `secretshape` matches a reference against the whole value and never searches
    // within it, so prose after it on the same line is a credential assignment again.
    let out = handoff(
        &root,
        "# Rotate it\n\nThe deploy credential lives in the vault.\ntoken: vault:forge/gh\n",
    );
    let told = said(&out);

    assert!(
        !told.contains("looks like it carries a secret"),
        "a vault reference was refused as a credential: {told}"
    );
    // It still stops, where no app answers — so the assertion above cannot be satisfied by
    // a handoff that failed earlier for some other reason.
    assert!(
        told.contains("no charter app answered this call"),
        "the brief was not read through to the app check: {told}"
    );
}
