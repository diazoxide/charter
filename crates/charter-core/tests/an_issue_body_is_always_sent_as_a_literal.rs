//! `WorkItems::create`'s argv, as the CLI transport sends it to `gh` and `glab`, for a todo whose
//! text starts with `@`.
//!
//! `gh api -F name=@file` and `glab api -F` read a value starting with `@` as a file to upload
//! (charter #323). A todo's text is the operator's, or an agent's, so `-F body=@/etc/passwd`
//! would send that file to the forge. Every value from a todo goes as `-f`, a literal.

use std::sync::{Arc, Mutex};

use charter_core::forge::backend::NewWorkItem;
use charter_core::forge::transport::{Call, NoAnswer, Reply, Transport};
use charter_core::forge::{Caller, Forge, ForgeError, Kind, cli};

/// A transport that writes down the argv each call would be sent as, and answers `answer`.
struct Argv {
    seen: Mutex<Vec<Vec<String>>>,
    answer: String,
}

impl Transport for Argv {
    fn send(&self, forge: &Forge, call: &Call) -> Result<Reply, NoAnswer> {
        self.seen.lock().unwrap().push(cli::argv(forge, call));
        Ok(Reply {
            code: 0,
            out: self.answer.clone(),
            err: String::new(),
            status: None,
            headers: Vec::new(),
        })
    }

    fn check_auth(&self, _forge: &Forge) -> Result<(), ForgeError> {
        Ok(())
    }
}

fn argv_of(kind: Kind, answer: &str) -> Vec<String> {
    let transport = Arc::new(Argv {
        seen: Mutex::new(Vec::new()),
        answer: answer.to_string(),
    });
    let backend = Forge::default_of(kind).backend_over(transport.clone());
    backend
        .create(
            &Caller::command(),
            "acme/api",
            &NewWorkItem {
                title: "@/etc/hosts".into(),
                body: "@/etc/passwd".into(),
                workspace_label: Some("alpha".into()),
            },
        )
        .unwrap();
    let seen = transport.seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "one call");
    seen[0].clone()
}

#[test]
fn a_github_issue_is_opened_with_every_todo_value_as_a_literal_field() {
    charter_core::unsteered!();
    let argv = argv_of(
        Kind::GitHub,
        r#"{"id": 1, "node_id": "I_1", "number": 12, "title": "t", "state": "open",
            "html_url": "https://github.com/acme/api/issues/12"}"#,
    );
    assert_eq!(
        argv,
        [
            "api",
            "--hostname",
            "github.com",
            "-X",
            "POST",
            "repos/acme/api/issues",
            "-f",
            "title=@/etc/hosts",
            "-f",
            "body=@/etc/passwd",
            "-f",
            "labels[]=ws:alpha",
        ]
    );
}

#[test]
fn a_gitlab_issue_is_opened_with_every_todo_value_as_a_literal_field() {
    charter_core::unsteered!();
    let argv = argv_of(
        Kind::GitLab,
        r#"{"id": 84012, "iid": 12, "web_url": "https://gitlab.com/acme/api/-/issues/12",
            "references": {"full": "acme/api#12"}}"#,
    );
    assert_eq!(
        argv,
        [
            "--hostname",
            "gitlab.com",
            "api",
            "-X",
            "POST",
            "projects/acme%2Fapi/issues",
            "-f",
            "title=@/etc/hosts",
            "-f",
            "description=@/etc/passwd",
            "-f",
            "labels=charter::ws::alpha",
        ]
    );
}
